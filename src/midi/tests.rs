//! Testes do mapeamento da DDJ-REV1: cada mensagem da lista MIDI oficial gera o comando certo.

use super::rev1::{leds, Rev1};
use crate::app::{AppState, Loader, PadMode};
use crate::shared::{Cmd, CmdTx, Snapshot};
use parking_lot::Mutex;
use std::sync::Arc;

struct Rig {
    rev1: Rev1,
    rx: rtrb::Consumer<Cmd>,
    app: Arc<Mutex<AppState>>,
    snap: Arc<Mutex<Snapshot>>,
}

fn rig() -> Rig {
    let (tx, rx) = rtrb::RingBuffer::new(1024);
    let cmd = CmdTx::new(tx);
    let app = Arc::new(Mutex::new(AppState::new()));
    let snap = Arc::new(Mutex::new(Snapshot::default()));
    let loader = Loader::start(app.clone(), cmd.clone());
    let rev1 = Rev1::new(app.clone(), cmd, snap.clone(), loader);
    Rig { rev1, rx, app, snap }
}

impl Rig {
    /// Manda uma mensagem MIDI e devolve os comandos gerados, como texto.
    fn send(&mut self, msg: [u8; 3]) -> Vec<String> {
        self.rev1.handle(&msg);
        let mut out = Vec::new();
        while let Ok(c) = self.rx.pop() {
            out.push(format!("{c:?}"));
        }
        out
    }

    fn one(&mut self, msg: [u8; 3]) -> String {
        let v = self.send(msg);
        assert_eq!(v.len(), 1, "{msg:02X?} deveria gerar 1 comando, gerou {v:?}");
        v[0].clone()
    }
}

// ------------------------------------------------------------------- decks

#[test]
fn transporte_dos_quatro_decks() {
    let mut r = rig();
    for d in 0..4u8 {
        let st = 0x90 | d;
        assert_eq!(r.one([st, 0x0B, 0x7F]), format!("PlayToggle({d})"));
        assert!(r.send([st, 0x0B, 0x00]).is_empty(), "soltar o PLAY não faz nada");
        assert_eq!(r.one([st, 0x47, 0x7F]), format!("Stutter({d})"));
        assert_eq!(r.one([st, 0x0C, 0x7F]), format!("Cue {{ deck: {d}, down: true }}"));
        assert_eq!(r.one([st, 0x0C, 0x00]), format!("Cue {{ deck: {d}, down: false }}"));
        assert_eq!(r.one([st, 0x48, 0x7F]), format!("GotoStart({d})"));
        assert_eq!(r.one([st, 0x58, 0x7F]), format!("Sync({d})"));
        assert_eq!(r.one([st, 0x5C, 0x7F]), format!("PitchReset({d})"));
    }
    // Note Off (0x8n) também solta o CUE.
    assert_eq!(r.one([0x80, 0x0C, 0x00]), "Cue { deck: 0, down: false }");
}

#[test]
fn loops_e_deck_select() {
    let mut r = rig();
    assert_eq!(r.one([0x91, 0x14, 0x7F]), "LoopToggle(1)");
    assert_eq!(r.one([0x91, 0x50, 0x7F]), "ReloopExit(1)");
    assert_eq!(r.one([0x91, 0x12, 0x7F]), "LoopHalve(1)");
    assert_eq!(r.one([0x91, 0x13, 0x7F]), "LoopDouble(1)");
    assert_eq!(r.one([0x91, 0x10, 0x7F]), "LoopIn(1)");
    assert_eq!(r.one([0x91, 0x11, 0x7F]), "LoopOut(1)");
    assert_eq!(r.one([0x90, 0x60, 0x7F]), "PitchRangeCycle(0)");
    assert_eq!(r.one([0x90, 0x17, 0x7F]), "Vinyl { deck: 0, on: true }");
    // Fader start (SHIFT + fader).
    assert_eq!(r.one([0x92, 0x66, 0x7F]), "Play(2)");
    assert_eq!(r.one([0x92, 0x51, 0x7F]), "Sync(2)");
    assert_eq!(r.one([0x92, 0x52, 0x7F]), "CueStop(2)");
    // A REV1 avisa a troca de camada: deck 3 na esquerda, deck 4 na direita.
    r.send([0x92, 0x3C, 0x7F]);
    r.send([0x93, 0x3C, 0x7F]);
    assert_eq!(r.app.lock().side, [2, 3]);
    r.send([0x90, 0x3C, 0x7F]);
    assert_eq!(r.app.lock().side, [0, 3]);
}

#[test]
fn jog_scratch_bend_e_busca() {
    let mut r = rig();
    assert_eq!(r.one([0x90, 0x36, 0x7F]), "JogTouch { deck: 0, down: true }");
    assert_eq!(r.one([0xB0, 0x22, 0x43]), "JogScratch { deck: 0, ticks: 3.0 }");
    assert_eq!(r.one([0xB0, 0x22, 0x3E]), "JogScratch { deck: 0, ticks: -2.0 }");
    assert_eq!(r.one([0xB1, 0x23, 0x41]), "JogBend { deck: 1, ticks: 1.0 }");
    assert_eq!(r.one([0xB1, 0x21, 0x3F]), "JogBend { deck: 1, ticks: -1.0 }");
    assert_eq!(r.one([0xB2, 0x1F, 0x45]), "JogSearch { deck: 2, ticks: 5.0 }");
    assert_eq!(r.one([0xB3, 0x26, 0x41]), "JogSearch { deck: 3, ticks: 1.0 }");
}

#[test]
fn modo_tracking_volta_ao_cue() {
    let mut r = rig();
    r.send([0x90, 0x20, 0x7F]); // botão TRACKING
    assert_eq!(r.app.lock().pad_mode[0], PadMode::Tracking);
    // Para trás e depois para frente: volta ao cue (lógica do mapeamento original).
    assert_eq!(r.send([0xB0, 0x22, 0x3E]), ["JogScratch { deck: 0, ticks: -2.0 }"]);
    assert_eq!(r.send([0xB0, 0x22, 0x42]), ["GotoCue(0)", "JogScratch { deck: 0, ticks: 2.0 }"]);
    // Soltar o jog também volta ao cue.
    assert_eq!(r.send([0x90, 0x36, 0x00]), ["JogTouch { deck: 0, down: false }", "GotoCue(0)"]);
    // Fora do modo Tracking, soltar o jog só solta.
    r.send([0x90, 0x1B, 0x7F]);
    assert_eq!(r.send([0x90, 0x36, 0x00]), ["JogTouch { deck: 0, down: false }"]);
}

#[test]
fn faders_e_knobs_de_14_bits() {
    let mut r = rig();
    // MSB e LSB: o valor final usa os dois.
    r.send([0xB0, 0x00, 0x7F]);
    assert_eq!(r.one([0xB0, 0x20, 0x7F]), "Tempo { deck: 0, v: 1.0 }");
    // MSB 0 com o LSB anterior (127): 127 / 16383.
    assert!(r.one([0xB0, 0x00, 0x00]).starts_with("Tempo { deck: 0, v: 0.00775193"));
    r.send([0xB1, 0x13, 0x40]);
    // 0x40 << 7 = 8192 / 16383.
    assert!(r.one([0xB1, 0x33, 0x00]).starts_with("Fader { deck: 1, v: 0.50003"));
    assert_eq!(r.one([0xB0, 0x0F, 0x00]), "Eq { deck: 0, band: 0, v: 0.0 }", "EQ LOW = banda 0");
    assert_eq!(r.one([0xB0, 0x0B, 0x00]), "Eq { deck: 0, band: 1, v: 0.0 }", "EQ MID = banda 1");
    assert_eq!(r.one([0xB0, 0x07, 0x00]), "Eq { deck: 0, band: 2, v: 0.0 }", "EQ HI = banda 2");
    assert_eq!(r.one([0xB2, 0x04, 0x00]), "Trim { deck: 2, v: 0.0 }");
    // Mixer global (canal 7).
    assert!(r.one([0xB6, 0x1F, 0x7F]).starts_with("Crossfader("));
    assert!(r.one([0xB6, 0x08, 0x7F]).starts_with("MasterLevel("));
    assert!(r.one([0xB6, 0x0D, 0x7F]).starts_with("HeadphoneLevel("));
    assert!(r.one([0xB6, 0x18, 0x7F]).starts_with("Filter { deck: 1,"), "FILTER CH2 = deck 2");
    assert!(r.one([0xB6, 0x3A, 0x7F]).starts_with("Filter { deck: 3,"), "LSB do FILTER CH4");
}

#[test]
fn pfl_e_master_cue() {
    let mut r = rig();
    assert_eq!(r.one([0x90, 0x54, 0x7F]), "Pfl { deck: 0, on: true }");
    // MASTER CUE sem nenhum PFL ligado: fone 100% master.
    assert_eq!(r.one([0x96, 0x63, 0x7F]), "HeadphoneMix(1.0)");
    assert!(r.app.lock().master_cue);
    // Com MASTER CUE ligado, ligar um PFL deixa o fone meio a meio.
    assert_eq!(r.send([0x91, 0x54, 0x7F]), ["Pfl { deck: 1, on: true }", "HeadphoneMix(0.5)"]);
    // MASTER CUE de novo: fone só cue.
    assert_eq!(r.one([0x96, 0x63, 0x7F]), "HeadphoneMix(0.0)");
    assert!(!r.app.lock().master_cue);
}

// ------------------------------------------------------------------ efeitos

#[test]
fn efeitos_um_por_deck() {
    let mut r = rig();
    r.snap.lock().decks[0].fx[0].beats = 0.5;
    // Alavanca (sem e com botão FX aceso) liga/desliga o efeito do deck do lado.
    assert_eq!(r.one([0x94, 0x50, 0x7F]), "FxActive { deck: 0, slot: 0, on: true }");
    assert_eq!(r.one([0x94, 0x50, 0x00]), "FxActive { deck: 0, slot: 0, on: false }");
    assert_eq!(r.one([0x95, 0x47, 0x7F]), "FxActive { deck: 1, slot: 0, on: true }");
    assert_eq!(r.one([0x95, 0x66, 0x00]), "FxActive { deck: 1, slot: 0, on: false }");
    // FX 2 / FX 3: batida menor / maior.
    assert_eq!(r.one([0x94, 0x71, 0x7F]), "FxBeats { deck: 0, slot: 0, beats: 0.25 }");
    assert_eq!(r.one([0x94, 0x72, 0x7F]), "FxBeats { deck: 0, slot: 0, beats: 0.75 }");
    assert_eq!(r.one([0x94, 0x06, 0x7F]), "FxBeats { deck: 0, slot: 0, beats: 0.25 }", "com SHIFT também");
    // FX 1: só mostra a escolha; o efeito entra depois de 1 s sem clicar.
    assert!(r.send([0x94, 0x70, 0x7F]).is_empty());
    assert_eq!(r.app.lock().fx_pick[0].map(|(k, _)| k.name()), Some("PING PONG"));
    r.send([0x94, 0x63, 0x7F]);
    assert_eq!(r.app.lock().fx_pick[0].map(|(k, _)| k.name()), Some("REVERB"));
    // Knob central: força, pelo canal do lado.
    r.send([0xB4, 0x02, 0x7F]);
    assert_eq!(r.one([0xB4, 0x22, 0x7F]), "FxLevel { deck: 0, slot: 0, v: 1.0 }");
    assert_eq!(r.one([0xB5, 0x04, 0x00]), "FxLevel { deck: 1, slot: 0, v: 0.0 }");
    // Com o deck 3 na esquerda, a seção FX1 controla o deck 3.
    r.send([0x92, 0x3C, 0x7F]);
    assert_eq!(r.one([0x94, 0x50, 0x7F]), "FxActive { deck: 2, slot: 0, on: true }");
}

// --------------------------------------------------------------------- pads

#[test]
fn botoes_de_modo_dos_pads() {
    let mut r = rig();
    for (note, mode) in [
        (0x1B, PadMode::HotCue),
        (0x69, PadMode::BeatJump),
        (0x1E, PadMode::AutoLoop),
        (0x6B, PadMode::Roll),
        (0x20, PadMode::Tracking),
        (0x6D, PadMode::Trans),
        (0x22, PadMode::Sampler),
        (0x6F, PadMode::ScratchBank),
    ] {
        r.send([0x91, note, 0x7F]);
        assert_eq!(r.app.lock().pad_mode[1], mode, "nota {note:02X}");
    }
}

#[test]
fn pads_de_cada_modo() {
    let mut r = rig();
    // Canais dos pads: 0x97 deck 1, 0x98 deck 1 + SHIFT, 0x99 deck 2 ... 0x9D deck 4.
    assert_eq!(r.one([0x97, 0x00, 0x7F]), "Hotcue { deck: 0, idx: 0, down: true }");
    assert_eq!(r.one([0x97, 0x00, 0x00]), "Hotcue { deck: 0, idx: 0, down: false }");
    assert_eq!(r.one([0x98, 0x03, 0x7F]), "HotcueDelete { deck: 0, idx: 3 }");
    assert_eq!(r.one([0x99, 0x13, 0x7F]), "LoopAuto { deck: 1, beats: 2.0 }");
    assert_eq!(r.one([0x9B, 0x22, 0x7F]), "TrackingPad { deck: 2, semis: 4.0 }");
    assert_eq!(r.one([0x9C, 0x22, 0x7F]), "TrackingPad { deck: 2, semis: -8.0 }", "SHIFT = oitava abaixo");
    assert_eq!(r.one([0x97, 0x45, 0x7F]), "BeatJump { deck: 0, beats: 4.0 }");
    assert_eq!(r.one([0x98, 0x40, 0x7F]), "BeatJump { deck: 0, beats: -4.0 }", "SHIFT = 4x");
    assert_eq!(r.one([0x9D, 0x52, 0x7F]), "Roll { deck: 3, beats: 0.125, down: true }");
    assert_eq!(r.one([0x9D, 0x52, 0x00]), "Roll { deck: 3, beats: 0.125, down: false }");
    assert_eq!(r.one([0x97, 0x61, 0x7F]), "Trans { deck: 0, beats: 0.5, down: true }");
    assert_eq!(r.one([0x98, 0x30, 0x7F]), "SamplerStop(0)");
    // Sampler sem sample carregado não faz nada.
    assert!(r.send([0x97, 0x30, 0x7F]).is_empty());
}

#[test]
fn com_o_teclado_aberto_os_pads_navegam() {
    let mut r = rig();
    crate::search::open(&mut r.app.lock());
    r.app.lock().search.as_mut().unwrap().go_to("A");
    assert!(r.send([0x97, 0x00, 0x7F]).is_empty(), "pad 1 vira seta, não hotcue");
    assert_eq!(r.app.lock().search.as_ref().unwrap().key(), "Q");
    r.send([0x97, 0x04, 0x7F]); // pad 5 confirma
    assert_eq!(r.app.lock().search.as_ref().unwrap().query, "Q");
    r.send([0x97, 0x07, 0x7F]); // pad 8 fecha
    assert!(r.app.lock().search.is_none());
    assert!(!r.send([0x97, 0x00, 0x7F]).is_empty(), "teclado fechado: hotcue de novo");
}

// ------------------------------------------------------------------ browser

#[test]
fn browse_load_e_preparar() {
    let mut r = rig();
    // Girar o BROWSE abre a lista em tela cheia.
    r.send([0xB6, 0x40, 0x01]);
    assert!(r.app.lock().big_browser);
    // SHIFT + BROWSE fecha.
    r.send([0x96, 0x42, 0x7F]);
    assert!(!r.app.lock().big_browser);
    // Toque curto no BROWSE (apertar e soltar): alterna pastas/músicas.
    let before = r.app.lock().lib.focus;
    r.send([0x96, 0x41, 0x7F]);
    r.send([0x96, 0x41, 0x00]);
    assert_ne!(r.app.lock().lib.focus, before);
    // Segurar o BROWSE: a tela abre a pesquisa depois de 0,5 s.
    r.send([0x96, 0x41, 0x7F]);
    r.app.lock().browse_down = Some(std::time::Instant::now() - std::time::Duration::from_millis(600));
    r.app.lock().tick_browse_hold();
    assert!(r.app.lock().search.is_some());
    r.send([0x96, 0x41, 0x00]);
    assert!(r.app.lock().search.as_ref().unwrap().query.is_empty(), "soltar depois de segurar não digita");
}

// --------------------------------------------------------------------- LEDs

#[test]
fn leds_refletem_o_estado() {
    let r = rig();
    let mut snap = Snapshot::default();
    snap.decks[0].loaded = true;
    snap.decks[0].playing = true;
    snap.decks[0].hotcues[2] = Some(1000.0);
    snap.decks[1].loop_on = true;
    let mut app = r.app.lock();
    app.side = [2, 1];
    let mut out = Vec::new();
    leds(&app, &snap, false, &mut out);
    let get = |st: u8, n: u8| out.iter().rev().find(|(s, m, _)| *s == st && *m == n).map(|x| x.2);
    assert_eq!(get(0x90, 0x0B), Some(0x7F), "PLAY aceso tocando");
    assert_eq!(get(0x97, 0x02), Some(0x7F), "pad 3 aceso: hotcue marcado");
    assert_eq!(get(0x97, 0x03), Some(0x00));
    assert_eq!(get(0x91, 0x14), Some(0x7F), "AUTO LOOP aceso com loop ligado");
    assert_eq!(get(0x92, 0x72), Some(0x7F), "deck 3 na esquerda");
    assert_eq!(get(0x93, 0x72), Some(0x00));
    assert_eq!(get(0x94, 0x70), Some(0x7F), "FX 1 aceso para o knob central funcionar");
    assert_eq!(get(0x90, 0x1B), Some(0x7F), "modo HOT CUE aceso");
}

#[test]
fn reconhece_a_controladora_pelo_nome_da_porta() {
    let n = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let rev1 = super::detect_names(&n(&["Midi Through:Midi Through Port-0 14:0", "DDJ-REV1:DDJ-REV1 MIDI 1 28:0"]));
    assert_eq!(rev1.and_then(|r| r.ok()).map(|m| m.name), Some("DDJ-REV1"));
    let outra = super::detect_names(&n(&["DDJ-400:DDJ-400 MIDI 1 24:0"]));
    assert_eq!(outra.and_then(|r| r.err()).as_deref(), Some("DDJ-400"), "desconhecida: mostra o nome");
    assert!(super::detect_names(&n(&["Midi Through:Midi Through Port-0 14:0"])).is_none(), "só portas do sistema");
}
