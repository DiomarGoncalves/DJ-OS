//! Conexão com a controladora: reconhece qual está ligada (pelo nome da porta MIDI), reconecta
//! sozinha e atualiza os LEDs.
//!
//! Cada controladora suportada é um `Model` no catálogo: o nome, como reconhecer a porta e como
//! criar o mapeamento dela (um `Controller`). A DDJ-REV1 tem o mapeamento escrito no código
//! (`rev1.rs`); as outras vêm de arquivos `.djosmap` (`mapa.rs`, `docs/mapeamento.md`).

pub mod aprender;
pub mod mapa;
pub mod rev1;
#[cfg(test)]
mod tests;

use crate::app::{AppState, Loader, SharedApp};
use crate::shared::{CmdTx, SharedSnap, Snapshot};
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// O mapeamento de uma controladora: transforma as mensagens MIDI dela em comandos do DJ OS e
/// diz quais LEDs acender.
pub trait Controller: Send {
    /// Uma mensagem MIDI que chegou da controladora.
    fn handle(&mut self, msg: &[u8]);
    /// LEDs a acender agora: (status, nota/CC, valor). Só o que mudou é enviado.
    fn leds(&self, app: &AppState, snap: &Snapshot, blink: bool, out: &mut Vec<(u8, u8, u8)>);
}

type Criar = Box<dyn Fn(SharedApp, CmdTx, SharedSnap, Loader) -> Box<dyn Controller> + Send + Sync>;

/// Controladora sem mapeamento: não faz nada sozinha, só com os botões ensinados (MIDI learn).
struct SemMapeamento;

impl Controller for SemMapeamento {
    fn handle(&mut self, msg: &[u8]) {
        if log_left() > 0 && msg.len() >= 3 {
            eprintln!("[midi] {:02X} {:02X} {:02X}", msg[0], msg[1], msg[2]);
        }
    }

    fn leds(&self, _: &AppState, _: &Snapshot, _: bool, _: &mut Vec<(u8, u8, u8)>) {}
}

/// Controladora que ninguém mapeou ainda (pela primeira parte do nome da porta): conecta assim
/// mesmo, para dar para ensinar os botões.
fn sem_mapeamento(nome: &str) -> Arc<Model> {
    let n = nome.to_lowercase();
    Arc::new(Model { name: format!("{nome} (sem mapeamento)"), base: nome.to_string(), matches: Box::new(move |p| p.to_lowercase().contains(&n)), create: Box::new(|_, _, _, _| Box::new(SemMapeamento)) })
}

/// Uma controladora suportada.
pub struct Model {
    /// Nome mostrado na tela.
    pub name: String,
    /// Nome sem "(em teste)" (é o do arquivo dos botões ensinados).
    pub base: String,
    /// A porta MIDI (pelo nome) é desta controladora?
    pub matches: Box<dyn Fn(&str) -> bool + Send + Sync>,
    pub create: Criar,
}

/// Controladora conectada agora (o nome sem "(em teste)"), para o MIDI learn.
pub static CONECTADA: Mutex<Option<String>> = Mutex::new(None);

/// Diagnóstico (CONFIG → CONTROLADORA): até quando (segundos desde 1970) cada mensagem que a
/// controladora manda vai para o registro da sessão.
pub static LOG_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Liga o registro dos botões por `secs` segundos.
pub fn log_for(secs: u64) {
    LOG_UNTIL.store(now_secs() + secs, std::sync::atomic::Ordering::Relaxed);
}

/// Segundos que faltam do registro dos botões (0 = desligado).
pub fn log_left() -> u64 {
    LOG_UNTIL.load(std::sync::atomic::Ordering::Relaxed).saturating_sub(now_secs())
}

/// Catálogo das controladoras suportadas (a primeira que bater com uma porta ligada é usada): a
/// REV1 e os mapeamentos por arquivo (os do programa e os da pessoa, relidos a cada conexão).
pub fn catalogo() -> Vec<Arc<Model>> {
    let mut v = vec![Arc::new(Model { name: "DDJ-REV1".into(), base: "DDJ-REV1".into(), matches: Box::new(rev1::matches), create: Box::new(rev1::create) })];
    for m in mapa::todos() {
        let (a, b) = (m.clone(), m.clone());
        v.push(Arc::new(Model {
            name: if m.situacao == "testado" { m.nome.clone() } else { format!("{} (em teste)", m.nome) },
            base: m.nome.clone(),
            matches: Box::new(move |p| a.reconhece(p)),
            create: Box::new(move |app, cmd, snap, loader| Box::new(mapa::Mapeado::new(b.clone(), app, cmd, snap, loader))),
        }));
    }
    v
}

/// Portas que não são controladoras (do próprio Linux ou do DJ OS).
fn is_system_port(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("midi through") || n.starts_with("dj-os")
}

/// Controladora ligada agora: o modelo conhecido ou o nome de uma desconhecida.
fn detect(scanner: &MidiInput, catalogo: &[Arc<Model>]) -> Option<Result<Arc<Model>, String>> {
    let names: Vec<String> = scanner.ports().iter().filter_map(|p| scanner.port_name(p).ok()).collect();
    detect_names(&names, catalogo)
}

fn detect_names(names: &[String], catalogo: &[Arc<Model>]) -> Option<Result<Arc<Model>, String>> {
    let names: Vec<&String> = names.iter().filter(|n| !is_system_port(n)).collect();
    for m in catalogo {
        if names.iter().any(|n| (m.matches)(n)) {
            return Some(Ok(m.clone()));
        }
    }
    // Primeira parte do nome da porta ("DDJ-400:DDJ-400 MIDI 1 24:0" → "DDJ-400").
    names.first().map(|n| Err(n.split(':').next().unwrap_or(n).trim().to_string()))
}

pub fn start(app: SharedApp, cmd: CmdTx, snap: SharedSnap, loader: Loader) {
    let mapping: Arc<Mutex<Option<Box<dyn Controller>>>> = Arc::new(Mutex::new(None));
    let out: Arc<Mutex<Option<MidiOutputConnection>>> = Arc::new(Mutex::new(None));
    let cache: Arc<Mutex<HashMap<(u8, u8), u8>>> = Arc::new(Mutex::new(HashMap::new()));

    // Conexão: procura uma controladora a cada segundo e reconecta se ela for desligada e ligada.
    {
        let (app, out, cache, mapping, snap) = (app.clone(), out.clone(), cache.clone(), mapping.clone(), snap.clone());
        std::thread::Builder::new()
            .name("midi".into())
            .spawn(move || {
                let mut conn: Option<(MidiInputConnection<()>, String)> = None;
                // Um só cliente MIDI para procurar (a lista de portas é relida a cada volta).
                let scanner = MidiInput::new("dj-os-busca").ok();
                let mut unknown_seen = String::new();
                let mut catalogo = catalogo();
                loop {
                    // Sem controladora conectada, relê os arquivos (mapeamento novo ou corrigido).
                    if conn.is_none() {
                        catalogo = self::catalogo();
                    }
                    let found = scanner.as_ref().and_then(|s| detect(s, &catalogo));
                    // Desconhecida: conecta "sem mapeamento" (os botões se ensinam em CONFIG).
                    let model = found.as_ref().map(|f| match f {
                        Ok(m) => m.clone(),
                        Err(nome) => sem_mapeamento(nome),
                    });
                    // A controladora conectada saiu (ou trocou por outra).
                    if let Some((_, name)) = &conn {
                        if model.as_ref().is_none_or(|m| m.name != *name) {
                            app.lock().midi_status = format!("{name} desconectada");
                            *CONECTADA.lock() = None;
                            conn = None;
                            *out.lock() = None;
                            *mapping.lock() = None;
                        }
                    }
                    match (&conn, model, &found) {
                        (None, Some(m), _) => {
                            // O mapeamento dela, com os botões ensinados (MIDI learn) por cima.
                            let base = (m.create)(app.clone(), cmd.clone(), snap.clone(), loader.clone());
                            *mapping.lock() = Some(Box::new(aprender::ComAprendidos::new(base, &m.base, app.clone(), cmd.clone(), snap.clone(), loader.clone())));
                            *CONECTADA.lock() = Some(m.base.clone());
                            match connect(&m, mapping.clone()) {
                                Ok((i, o)) => {
                                    conn = Some((i, m.name.clone()));
                                    *out.lock() = o;
                                    cache.lock().clear();
                                    app.lock().midi_status = if m.name.ends_with("(sem mapeamento)") {
                                        format!("{}: ensine os botões em CONFIG → CONTROLADORA", m.base)
                                    } else {
                                        format!("{} conectada", m.name)
                                    };
                                    eprintln!("[djos] {} conectada", m.name);
                                }
                                Err(e) => app.lock().midi_status = format!("{}: {e}", m.name),
                            }
                        }
                        (None, None, Some(Err(other))) if *other != unknown_seen => {
                            unknown_seen = other.clone();
                            app.lock().midi_status = format!("{other}: ainda não suportada");
                            eprintln!("[djos] controladora não suportada: {other}");
                        }
                        (None, None, None) if !unknown_seen.is_empty() => {
                            unknown_seen.clear();
                            app.lock().midi_status = "procurando controladora…".into();
                        }
                        _ => {}
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            })
            .expect("thread midi");
    }

    // LEDs: 30 vezes por segundo, só manda o que mudou.
    std::thread::Builder::new()
        .name("leds".into())
        .spawn(move || {
            let start = Instant::now();
            let mut list = Vec::with_capacity(256);
            loop {
                std::thread::sleep(Duration::from_millis(33));
                let mut o = out.lock();
                let Some(conn) = o.as_mut() else { continue };
                let blink = (start.elapsed().as_millis() / 400) % 2 == 0;
                list.clear();
                {
                    // Mesma ordem da entrada MIDI (mapeamento, depois app): na ordem contrária, um
                    // botão mexido bem nessa hora travava as duas threads para sempre (tela e
                    // controladora paradas, só o som seguindo).
                    let s = *snap.lock();
                    let m = mapping.lock();
                    if let Some(m) = m.as_ref() {
                        m.leds(&app.lock(), &s, blink, &mut list);
                    }
                }
                let mut c = cache.lock();
                for &(st, n, v) in &list {
                    if c.get(&(st, n)) != Some(&v) {
                        if conn.send(&[st, n, v]).is_ok() {
                            c.insert((st, n), v);
                        }
                    }
                }
            }
        })
        .expect("thread leds");
}

/// Conecta na entrada MIDI da controladora e, se ela tiver, na saída (LEDs).
fn connect(model: &Model, mapping: Arc<Mutex<Option<Box<dyn Controller>>>>) -> Result<(MidiInputConnection<()>, Option<MidiOutputConnection>), String> {
    let input = MidiInput::new("dj-os").map_err(|e| e.to_string())?;
    let in_port = input
        .ports()
        .into_iter()
        .find(|p| input.port_name(p).is_ok_and(|n| (model.matches)(&n)))
        .ok_or("entrada MIDI não encontrada")?;
    let output = MidiOutput::new("dj-os").map_err(|e| e.to_string())?;
    let out_port = output.ports().into_iter().find(|p| output.port_name(p).is_ok_and(|n| (model.matches)(&n)));
    let conn_in = input
        .connect(
            &in_port,
            "controladora-in",
            move |_, msg, _| {
                if let Some(m) = mapping.lock().as_mut() {
                    m.handle(msg);
                }
            },
            (),
        )
        .map_err(|e| e.to_string())?;
    // Sem saída MIDI (controladora sem LEDs): funciona só a entrada.
    let conn_out = out_port.and_then(|p| output.connect(&p, "controladora-out").ok());
    Ok((conn_in, conn_out))
}
