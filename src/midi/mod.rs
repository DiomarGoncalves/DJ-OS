//! Conexão com a controladora: reconhece qual está ligada (pelo nome da porta MIDI), reconecta
//! sozinha e atualiza os LEDs.
//!
//! Cada controladora suportada é um `Model` no catálogo `MODELS`: o nome, como reconhecer a porta
//! e como criar o mapeamento dela (um `Controller`). Para acrescentar uma controladora nova, veja
//! `docs/controladoras.md`.

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

/// Uma controladora suportada.
pub struct Model {
    /// Nome mostrado na tela.
    pub name: &'static str,
    /// A porta MIDI (pelo nome) é desta controladora?
    pub matches: fn(&str) -> bool,
    pub create: fn(SharedApp, CmdTx, SharedSnap, Loader) -> Box<dyn Controller>,
}

/// Catálogo das controladoras suportadas (a primeira que bater com uma porta ligada é usada).
pub const MODELS: &[Model] = &[Model { name: "DDJ-REV1", matches: rev1::matches, create: rev1::create }];

/// Portas que não são controladoras (do próprio Linux ou do DJ OS).
fn is_system_port(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("midi through") || n.starts_with("dj-os")
}

/// Controladora ligada agora: o modelo conhecido ou o nome de uma desconhecida.
fn detect(scanner: &MidiInput) -> Option<Result<&'static Model, String>> {
    let names: Vec<String> = scanner.ports().iter().filter_map(|p| scanner.port_name(p).ok()).collect();
    detect_names(&names)
}

fn detect_names(names: &[String]) -> Option<Result<&'static Model, String>> {
    let names: Vec<&String> = names.iter().filter(|n| !is_system_port(n)).collect();
    for m in MODELS {
        if names.iter().any(|n| (m.matches)(n)) {
            return Some(Ok(m));
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
                let mut conn: Option<(MidiInputConnection<()>, &'static str)> = None;
                // Um só cliente MIDI para procurar (a lista de portas é relida a cada volta).
                let scanner = MidiInput::new("dj-os-busca").ok();
                let mut unknown_seen = String::new();
                loop {
                    let found = scanner.as_ref().and_then(detect);
                    let model = found.as_ref().and_then(|f| f.as_ref().ok()).copied();
                    // A controladora conectada saiu (ou trocou por outra).
                    if let Some((_, name)) = &conn {
                        if model.is_none_or(|m| m.name != *name) {
                            app.lock().midi_status = format!("{name} desconectada");
                            conn = None;
                            *out.lock() = None;
                            *mapping.lock() = None;
                        }
                    }
                    match (&conn, model, &found) {
                        (None, Some(m), _) => {
                            *mapping.lock() = Some((m.create)(app.clone(), cmd.clone(), snap.clone(), loader.clone()));
                            match connect(m, mapping.clone()) {
                                Ok((i, o)) => {
                                    conn = Some((i, m.name));
                                    *out.lock() = Some(o);
                                    cache.lock().clear();
                                    app.lock().midi_status = format!("{} conectada", m.name);
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

fn connect(model: &Model, mapping: Arc<Mutex<Option<Box<dyn Controller>>>>) -> Result<(MidiInputConnection<()>, MidiOutputConnection), String> {
    let input = MidiInput::new("dj-os").map_err(|e| e.to_string())?;
    let in_port = input
        .ports()
        .into_iter()
        .find(|p| input.port_name(p).is_ok_and(|n| (model.matches)(&n)))
        .ok_or("entrada MIDI não encontrada")?;
    let output = MidiOutput::new("dj-os").map_err(|e| e.to_string())?;
    let out_port = output
        .ports()
        .into_iter()
        .find(|p| output.port_name(p).is_ok_and(|n| (model.matches)(&n)))
        .ok_or("saída MIDI não encontrada")?;
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
    let conn_out = output.connect(&out_port, "controladora-out").map_err(|e| e.to_string())?;
    Ok((conn_in, conn_out))
}
