//! MIDI learn: a pessoa escolhe uma função em CONFIG → CONTROLADORA e aperta o botão (ou mexe no
//! knob) na controladora; a linha vai para `~/DJOS/mapeamentos/aprendidos/<controladora>.djosmap`
//! e passa a valer por cima do mapeamento da controladora (da REV1 também).

use super::mapa::{self, Mapa, Mapeado};
use super::Controller;
use crate::app::{AppState, Loader, SharedApp};
use crate::shared::{CmdTx, SharedSnap, Snapshot};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Funções que dá para ensinar: (texto na tela, ação no arquivo, precisa de deck).
pub const FUNCOES: &[(&str, &str, bool)] = &[
    ("PLAY", "play", true),
    ("CUE", "cue", true),
    ("SYNC", "sync", true),
    ("CUE DO FONE", "pfl", true),
    ("LOAD", "load", true),
    ("HOT CUE A", "hotcue n=1", true),
    ("HOT CUE B", "hotcue n=2", true),
    ("HOT CUE C", "hotcue n=3", true),
    ("HOT CUE D", "hotcue n=4", true),
    ("HOT CUE E", "hotcue n=5", true),
    ("HOT CUE F", "hotcue n=6", true),
    ("HOT CUE G", "hotcue n=7", true),
    ("HOT CUE H", "hotcue n=8", true),
    ("LOOP 4 BATIDAS", "loop_auto b=4", true),
    ("LOOP LIGA/DESLIGA", "loop", true),
    ("LOOP ÷2", "loop_metade", true),
    ("LOOP ×2", "loop_dobro", true),
    ("LOOP IN", "loop_in", true),
    ("LOOP OUT", "loop_out", true),
    ("RELOOP / SAIR DO LOOP", "loop_sair", true),
    ("BEAT JUMP −4", "beatjump b=-4", true),
    ("BEAT JUMP +4", "beatjump b=4", true),
    ("PITCH A ZERO", "pitch_zero", true),
    ("KEYLOCK", "keylock", true),
    ("VINIL", "vinil", true),
    ("SLIP", "slip", true),
    ("REVERSE", "reverso", true),
    ("FREIO (VINIL PARANDO)", "freio", true),
    ("TOM +1", "tom_mais", true),
    ("TOM -1", "tom_menos", true),
    ("TOM ORIGINAL", "tom_zero", true),
    ("CASAR O TOM (KEY SYNC)", "tom_casar", true),
    ("FADER DO CANAL", "volume", true),
    ("FADER DE PITCH", "tempo", true),
    ("TRIM", "trim", true),
    ("EQ GRAVE", "grave", true),
    ("EQ MÉDIO", "medio", true),
    ("EQ AGUDO", "agudo", true),
    ("FILTRO", "filtro", true),
    ("JOG (SCRATCH)", "jog_scratch", true),
    ("JOG (LATERAL)", "jog_bend", true),
    ("JOG (TOQUE)", "jog_toque", true),
    ("EFEITO LIGA/DESLIGA", "fx_liga", true),
    ("EFEITO FORÇA", "fx_nivel", true),
    ("EFEITO TROCA", "fx_proximo", true),
    ("CROSSFADER", "crossfader", false),
    ("MASTER", "master", false),
    ("VOLUME DO FONE", "fone_volume", false),
    ("BROWSE GIRAR", "browse_gira", false),
    ("BROWSE APERTAR", "browse_aperta", false),
    ("SHIFT", "shift", false),
    ("QUANTIZE", "quantize", false),
    ("FILTRO DO BROWSER", "filtro_browser", false),
    ("SALVAR A FILA COMO PLAYLIST", "salvar_fila", false),
];

/// Pedido de MIDI learn em andamento: a linha (ação e opções) e a controladora.
pub struct Pedido {
    pub acao: String,
    pub controladora: String,
}

/// Esperando o botão da controladora (a tela mostra "aperte o botão…").
pub static ESPERANDO: Mutex<Option<Pedido>> = Mutex::new(None);
/// Sobe a cada botão ensinado ou apagado (o mapeamento relê o arquivo).
pub static GERACAO: AtomicU64 = AtomicU64::new(0);

/// Nome de arquivo a partir do nome da controladora.
fn slug(nome: &str) -> String {
    nome.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect::<String>().split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-")
}

pub fn arquivo(controladora: &str) -> PathBuf {
    mapa::pasta_usuario().join("aprendidos").join(format!("{}.djosmap", slug(controladora)))
}

/// Uma mensagem chegou com o MIDI learn esperando: vira linha do arquivo. Devolve `true` se usou
/// a mensagem (ela não vai para o mapeamento). Soltar botão (nota desligada / valor 0) é ignorado.
pub fn talvez_aprender(msg: &[u8]) -> Option<String> {
    if msg.len() < 3 {
        return None;
    }
    let mut esperando = ESPERANDO.lock();
    let pedido = esperando.as_ref()?;
    let (st, num, v) = (msg[0], msg[1], msg[2]);
    let canal = (st & 0x0F) + 1;
    let linha = match st & 0xF0 {
        0x90 if v > 0 => format!("nota {canal} 0x{num:02X} {}", pedido.acao),
        0xB0 => format!("cc {canal} 0x{num:02X} {}", pedido.acao),
        0xE0 => format!("pb {canal} {}", pedido.acao),
        _ => return Some(String::new()),
    };
    let pedido = esperando.take()?;
    let f = arquivo(&pedido.controladora);
    let _ = std::fs::create_dir_all(f.parent().unwrap_or(&mapa::pasta_usuario()));
    let mut texto = std::fs::read_to_string(&f).unwrap_or_else(|_| format!("# Botões ensinados pelo MIDI learn (valem por cima do mapeamento).\nnome = aprendidos {}\nporta = (aprendidos)\n", pedido.controladora));
    // A mesma mensagem ensinada de novo: a linha antiga sai.
    let chave: Vec<&str> = linha.split_whitespace().take(if st & 0xF0 == 0xE0 { 2 } else { 3 }).collect();
    texto = texto.lines().filter(|l| l.split_whitespace().take(chave.len()).collect::<Vec<_>>() != chave).map(|l| format!("{l}\n")).collect();
    texto.push_str(&linha);
    texto.push('\n');
    if let Err(e) = std::fs::write(&f, texto) {
        return Some(format!("Não gravou: {e}"));
    }
    GERACAO.fetch_add(1, Ordering::Relaxed);
    Some(format!("Aprendido: {linha}"))
}

/// Esquece o que foi ensinado para esta controladora.
pub fn apagar(controladora: &str) {
    let _ = std::fs::remove_file(arquivo(controladora));
    GERACAO.fetch_add(1, Ordering::Relaxed);
}

/// Quantos botões ensinados esta controladora tem.
pub fn quantos(controladora: &str) -> usize {
    std::fs::read_to_string(arquivo(controladora)).map_or(0, |t| t.lines().filter(|l| l.starts_with("nota ") || l.starts_with("cc ") || l.starts_with("pb ")).count())
}

/// Mapeamento da controladora com os botões ensinados por cima.
pub struct ComAprendidos {
    base: Box<dyn Controller>,
    nome: String,
    aprendidos: Option<(Arc<Mapa>, Mapeado)>,
    geracao: u64,
    app: SharedApp,
    cmd: CmdTx,
    snap: SharedSnap,
    loader: Loader,
}

impl ComAprendidos {
    pub fn new(base: Box<dyn Controller>, nome: &str, app: SharedApp, cmd: CmdTx, snap: SharedSnap, loader: Loader) -> Self {
        let mut c = Self { base, nome: nome.to_string(), aprendidos: None, geracao: u64::MAX, app, cmd, snap, loader };
        c.reler();
        c
    }

    fn reler(&mut self) {
        let g = GERACAO.load(Ordering::Relaxed);
        if g == self.geracao {
            return;
        }
        self.geracao = g;
        self.aprendidos = std::fs::read_to_string(arquivo(&self.nome)).ok().and_then(|t| match mapa::ler(&t) {
            Ok(m) => {
                let m = Arc::new(m);
                Some((m.clone(), Mapeado::new(m, self.app.clone(), self.cmd.clone(), self.snap.clone(), self.loader.clone())))
            }
            Err(e) => {
                eprintln!("[djos] botões ensinados de {}: {e}", self.nome);
                None
            }
        });
    }
}

impl Controller for ComAprendidos {
    fn handle(&mut self, msg: &[u8]) {
        if let Some(m) = talvez_aprender(msg) {
            if !m.is_empty() {
                self.app.lock().toast(m);
            }
            return;
        }
        self.reler();
        if let Some((_, m)) = self.aprendidos.as_mut() {
            if m.trata(msg) {
                return;
            }
        }
        self.base.handle(msg);
    }

    fn leds(&self, app: &AppState, snap: &Snapshot, blink: bool, out: &mut Vec<(u8, u8, u8)>) {
        self.base.leds(app, snap, blink, out);
        if let Some((m, _)) = &self.aprendidos {
            mapa::leds(m, app, snap, out);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn nome_do_arquivo() {
        assert_eq!(super::slug("Pioneer DDJ-400 (em teste)"), "pioneer-ddj-400-em-teste");
        assert!(super::arquivo("DDJ-REV1").ends_with("aprendidos/ddj-rev1.djosmap"));
    }
}
