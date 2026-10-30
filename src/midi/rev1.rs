//! Mapeamento da Pioneer DDJ-REV1.
//!
//! Endereços: "DDJ-REV1 List of MIDI message" (site da Pioneer DJ; não incluída no projeto).
//! Comportamento: reproduz o mapeamento personalizado do Diomar no VirtualDJ
//! (docs/mapeamento-ddj-rev1.md lista botão por botão).
//!
//! Canais MIDI: 0-3 decks, 4 FX1 (lado esquerdo), 5 FX2 (lado direito), 6 browser/mixer,
//! 7-14 pads (deck 1, deck 1 + SHIFT, deck 2, deck 2 + SHIFT...).

use crate::app::{Loader, PadMode, SharedApp};
use crate::engine::fx::{beats_label, step_beats};
use crate::shared::{Cmd, CmdTx, SharedSnap, Snapshot, NUM_DECKS};
use std::time::{Duration, Instant};

pub const AUTOLOOP_PADS: [f64; 8] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
pub const ROLL_PADS: [f64; 8] = [1.0 / 32.0, 1.0 / 16.0, 1.0 / 8.0, 0.25, 0.5, 1.0, 2.0, 4.0];
pub const BEATJUMP_PADS: [f64; 8] = [-1.0, 1.0, -2.0, 2.0, -4.0, 4.0, -8.0, 8.0];
pub const TRANS_PADS: [f64; 8] = [1.0, 0.5, 0.25, 0.125, 1.0 / 16.0, 2.0 / 3.0, 1.0 / 3.0, 1.0 / 6.0];
/// Tracking: cada pad toca a partir do cue numa nota da escala maior.
pub const TRACKING_SEMIS: [f64; 8] = [0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 11.0, 12.0];

/// Botões de modo dos pads: (nota, nota com SHIFT) → (modo, modo com SHIFT).
const PAD_MODE_NOTES: [(u8, u8, PadMode, PadMode); 4] = [
    (0x1B, 0x69, PadMode::HotCue, PadMode::BeatJump),
    (0x1E, 0x6B, PadMode::AutoLoop, PadMode::Roll),
    (0x20, 0x6D, PadMode::Tracking, PadMode::Trans),
    (0x22, 0x6F, PadMode::Sampler, PadMode::ScratchBank),
];

const DOUBLE_PRESS: Duration = Duration::from_millis(350);

pub struct Rev1 {
    app: SharedApp,
    cmd: CmdTx,
    snap: SharedSnap,
    loader: Loader,
    msb: [[u8; 128]; 16],
    lsb: [[u8; 128]; 16],
    /// Estado do modo Tracking (variáveis `rev1trcpad` e `rev1bck` do mapeamento original).
    tracking: [bool; NUM_DECKS],
    went_back: [bool; NUM_DECKS],
    last_load: [Option<Instant>; NUM_DECKS],
    pub debug: bool,
}

/// Jog e browse mandam diferença relativa: 0x41+ horário, 0x3F- anti-horário.
fn jog_delta(v: u8) -> f64 {
    v as f64 - 64.0
}

/// Browse e seletor de efeito: 1..30 horário, 127..98 anti-horário.
fn enc_delta(v: u8) -> i32 {
    if v < 64 {
        v as i32
    } else {
        v as i32 - 128
    }
}

/// A porta MIDI é da DDJ-REV1?
pub fn matches(port: &str) -> bool {
    let n = port.to_uppercase();
    n.contains("REV1") || n.contains("DDJ-REV")
}

pub fn create(app: SharedApp, cmd: CmdTx, snap: SharedSnap, loader: Loader) -> Box<dyn super::Controller> {
    Box::new(Rev1::new(app, cmd, snap, loader))
}

impl super::Controller for Rev1 {
    fn handle(&mut self, msg: &[u8]) {
        Rev1::handle(self, msg);
    }

    fn leds(&self, app: &crate::app::AppState, snap: &Snapshot, blink: bool, out: &mut Vec<(u8, u8, u8)>) {
        leds(app, snap, blink, out);
    }
}

impl Rev1 {
    pub fn new(app: SharedApp, cmd: CmdTx, snap: SharedSnap, loader: Loader) -> Self {
        Self {
            app,
            cmd,
            snap,
            loader,
            msb: [[0; 128]; 16],
            lsb: [[0; 128]; 16],
            tracking: [false; NUM_DECKS],
            went_back: [false; NUM_DECKS],
            last_load: [None; NUM_DECKS],
            debug: std::env::var_os("DJOS_MIDI_DEBUG").is_some(),
        }
    }

    fn snap(&self) -> Snapshot {
        *self.snap.lock()
    }

    fn send(&self, c: Cmd) {
        self.cmd.send(c);
    }

    /// Valor de 14 bits (MSB + LSB), atualizado a cada uma das duas mensagens.
    fn v14(&mut self, ch: usize, msb_cc: u8, is_lsb: bool, v: u8) -> f32 {
        let i = msb_cc as usize;
        if is_lsb {
            self.lsb[ch][i] = v;
        } else {
            self.msb[ch][i] = v;
        }
        (((self.msb[ch][i] as u32) << 7) | self.lsb[ch][i] as u32) as f32 / 16383.0
    }

    pub fn handle(&mut self, msg: &[u8]) {
        if msg.len() < 3 {
            return;
        }
        if self.debug || super::log_left() > 0 {
            eprintln!("[midi] {:02X} {:02X} {:02X}", msg[0], msg[1], msg[2]);
        }
        let (st, d1, d2) = (msg[0], msg[1], msg[2]);
        let ch = (st & 0x0F) as usize;
        match st & 0xF0 {
            0x90 | 0x80 => {
                let down = st & 0xF0 == 0x90 && d2 > 0;
                match ch {
                    0..=3 => self.deck_note(ch, d1, down),
                    4 | 5 => self.fx_note(ch - 4, d1, down),
                    6 => self.global_note(d1, down),
                    7..=14 => self.pad(ch - 7, d1, down),
                    _ => {}
                }
            }
            0xB0 => match ch {
                0..=3 => self.deck_cc(ch, d1, d2),
                4 | 5 => self.fx_cc(ch - 4, d1, d2),
                6 => self.global_cc(d1, d2),
                _ => {}
            },
            _ => {}
        }
    }

    // ------------------------------------------------------------------ decks

    fn deck_note(&mut self, d: usize, note: u8, down: bool) {
        let side = d % 2;
        match note {
            // PLAY/PAUSE; com SHIFT: stutter (volta ao cue tocando).
            0x0B if down => self.send(Cmd::PlayToggle(d)),
            0x47 if down => self.send(Cmd::Stutter(d)),
            // CUE; com SHIFT: volta ao início.
            0x0C => self.send(Cmd::Cue { deck: d, down }),
            0x48 if down => self.send(Cmd::GotoStart(d)),
            // Toque no jog. No modo Tracking, soltar o jog volta ao cue.
            0x36 | 0x67 => {
                self.send(Cmd::JogTouch { deck: d, down });
                if !down && self.tracking[d] {
                    self.went_back[d] = false;
                    self.send(Cmd::GotoCue(d));
                }
            }
            // DECK SELECT: o controlador avisa qual deck ficou ativo (0x3C liga/desliga).
            0x3C if down => self.app.lock().side[side] = d,
            0x72 if down && (d == 2 || d == 3) => {
                let mut a = self.app.lock();
                a.side[side] = if a.side[side] == side { side + 2 } else { side };
            }
            // SHIFT + DECK SELECT: modo vinil (jog faz scratch) liga/desliga.
            0x17 if down => {
                let on = !self.snap().decks[d].vinyl;
                self.send(Cmd::Vinyl { deck: d, on });
                self.app.lock().toast(format!("Deck {}: {}", d + 1, if on { "VINIL" } else { "CDJ" }));
            }
            // DECK SELECT segurado: faixa de pitch 8 → 16 → 50%.
            0x60 if down => self.send(Cmd::PitchRangeCycle(d)),
            // AUTO LOOP; com SHIFT: reloop/exit.
            0x14 if down => self.send(Cmd::LoopToggle(d)),
            0x50 if down => self.send(Cmd::ReloopExit(d)),
            // 1/2X e 2X; com SHIFT: loop in e loop out.
            0x12 if down => self.send(Cmd::LoopHalve(d)),
            0x13 if down => self.send(Cmd::LoopDouble(d)),
            0x10 if down => self.send(Cmd::LoopIn(d)),
            0x11 if down => self.send(Cmd::LoopOut(d)),
            // SYNC; com SHIFT: pitch volta a 0.
            0x58 if down => self.send(Cmd::Sync(d)),
            0x5C if down => self.send(Cmd::PitchReset(d)),
            // CUE do fone (PFL).
            0x54 if down => self.pfl(d),
            // Fader start (SHIFT + fader).
            0x66 if down => self.send(Cmd::Play(d)),
            0x51 if down => self.send(Cmd::Sync(d)),
            0x52 if down => self.send(Cmd::CueStop(d)),
            _ => {
                if down {
                    for (n, ns, mode, mode_s) in PAD_MODE_NOTES {
                        if note == n || note == ns {
                            let m = if note == n { mode } else { mode_s };
                            eprintln!("[djos] deck {}: pads em {}", d + 1, m.name());
                            self.app.lock().pad_mode[d] = m;
                            self.tracking[d] = m == PadMode::Tracking;
                            self.went_back[d] = false;
                        }
                    }
                }
            }
        }
    }

    fn deck_cc(&mut self, d: usize, cc: u8, v: u8) {
        const LSBS: [u8; 7] = [0x20, 0x24, 0x25, 0x27, 0x2B, 0x2F, 0x33];
        let (base, is_lsb) = if LSBS.contains(&cc) { (cc - 0x20, true) } else { (cc, false) };
        match base {
            0x00 | 0x05 => {
                let x = self.v14(d, base, is_lsb, v) as f64;
                self.send(Cmd::Tempo { deck: d, v: x });
            }
            0x04 => {
                let x = self.v14(d, base, is_lsb, v);
                self.send(Cmd::Trim { deck: d, v: x });
            }
            0x07 | 0x0B | 0x0F => {
                let x = self.v14(d, base, is_lsb, v);
                let band = match base {
                    0x0F => 0,
                    0x0B => 1,
                    _ => 2,
                };
                self.send(Cmd::Eq { deck: d, band, v: x });
            }
            0x13 => {
                let x = self.v14(d, base, is_lsb, v);
                self.send(Cmd::Fader { deck: d, v: x });
            }
            // Prato do jog (vinil ligado): scratch. Lógica do Tracking igual ao mapeamento original.
            0x22 => {
                let t = jog_delta(v);
                if self.tracking[d] {
                    if t < 0.0 {
                        self.went_back[d] = true;
                    } else if self.went_back[d] {
                        self.went_back[d] = false;
                        self.send(Cmd::GotoCue(d));
                    }
                }
                self.send(Cmd::JogScratch { deck: d, ticks: t });
            }
            // Prato com vinil desligado e lateral do jog: pitch bend.
            0x23 | 0x21 => self.send(Cmd::JogBend { deck: d, ticks: jog_delta(v) }),
            // SHIFT + jog: busca rápida.
            0x1F | 0x26 => self.send(Cmd::JogSearch { deck: d, ticks: jog_delta(v) }),
            _ => {}
        }
    }

    /// PFL com a lógica do botão MASTER CUE do mapeamento original.
    fn pfl(&mut self, d: usize) {
        let snap = self.snap();
        let on = !snap.decks[d].pfl;
        self.send(Cmd::Pfl { deck: d, on });
        if self.app.lock().master_cue {
            let any = (0..NUM_DECKS).any(|i| if i == d { on } else { snap.decks[i].pfl });
            self.send(Cmd::HeadphoneMix(if any { 0.5 } else { 1.0 }));
        }
    }

    fn master_cue(&mut self) {
        let snap = self.snap();
        let mut a = self.app.lock();
        if !a.master_cue {
            let any = snap.decks.iter().any(|d| d.pfl);
            self.cmd.send(Cmd::HeadphoneMix(if any { 0.5 } else { 1.0 }));
            a.master_cue = true;
        } else {
            self.cmd.send(Cmd::HeadphoneMix(0.0));
            a.master_cue = false;
        }
    }

    // ------------------------------------------------------------- browser/mixer

    fn global_note(&mut self, note: u8, down: bool) {
        match note {
            0x3F => self.app.lock().shift = down,
            // BROWSE: toque curto (ao soltar) entra/volta da pasta, ou digita a tecla com o teclado
            // aberto; segurar 0,5 s abre a pesquisa (conferido a cada quadro da tela).
            0x41 => {
                let mut a = self.app.lock();
                if down {
                    a.browse_down = Some(Instant::now());
                    a.browse_long = false;
                } else if a.browse_down.is_some() {
                    if a.settings_ui.is_some() && a.search.is_none() {
                        // Configurações abertas: apertar o BROWSE aciona o item.
                        a.browse_down = None;
                        drop(a);
                        crate::settings::activate(&self.app, &self.cmd);
                    } else {
                        a.browse_released();
                    }
                }
            }
            // SHIFT + BROWSE: abre/fecha a lista em tela cheia.
            0x42 if down => {
                let mut a = self.app.lock();
                a.big_browser = !a.big_browser;
                if !a.big_browser {
                    crate::search::close(&mut a);
                }
            }
            0x46..=0x49 if down => self.load(match note {
                0x46 => 0,
                0x47 => 1,
                0x48 => 2,
                _ => 3,
            }),
            // SHIFT + LOAD: numa música, adiciona na lista "Preparar"; numa pasta do pendrive,
            // copia a pasta para as músicas internas.
            0x58 | 0x59 | 0x60 | 0x61 if down => {
                let mut a = self.app.lock();
                if a.lib.focus == crate::library::Focus::Folders {
                    if let Some(p) = a.lib.selected_path() {
                        drop(a);
                        let msg = crate::import::start(&self.app, p);
                        self.app.lock().toast(msg);
                    }
                } else if let Some(name) = a.lib.add_selected_to_prepare() {
                    a.toast(format!("Em Preparar: {name}"));
                }
            }
            0x63 if down => self.master_cue(),
            // SHIFT + MASTER CUE: abre/fecha as configurações.
            0x62 if down => crate::settings::toggle(&mut self.app.lock()),
            _ => {}
        }
    }

    /// LOAD: dois toques clonam o outro deck; na lista de pastas, abre a pasta.
    fn load(&mut self, d: usize) {
        let now = Instant::now();
        let double = self.last_load[d].is_some_and(|t| now.duration_since(t) < DOUBLE_PRESS);
        self.last_load[d] = Some(now);
        let mut a = self.app.lock();
        if a.search.is_some() {
            return; // com o teclado aberto o LOAD não faz nada (confirma-se com ENTER)
        }
        if double {
            let from = a.side[1 - d % 2];
            if let Some(t) = a.decks[from].track.clone() {
                a.decks[d].track = Some(t);
                self.cmd.send(Cmd::CloneDeck { from, to: d });
                a.toast(format!("Deck {} clonado do deck {}", d + 1, from + 1));
            }
            return;
        }
        if a.lib.focus == crate::library::Focus::Folders {
            a.lib.open_selected();
            return;
        }
        if let Some(t) = a.lib.selected_track() {
            self.loader.load_deck(&mut a, d, t.path);
            // Carregou: fecha a tela cheia e volta para as waveforms.
            a.big_browser = false;
        }
    }

    fn global_cc(&mut self, cc: u8, v: u8) {
        match cc {
            0x40 => {
                let mut a = self.app.lock();
                if a.search.is_none() && a.settings_ui.is_some() {
                    crate::settings::scroll(&mut a, enc_delta(v));
                    return;
                }
                match a.search.as_mut() {
                    Some(ui) => ui.step(enc_delta(v)),
                    None => {
                        // Girar o BROWSE na tela principal abre a lista em tela cheia.
                        a.big_browser = true;
                        a.lib.scroll(enc_delta(v));
                    }
                }
            }
            0x64 => {
                let mut a = self.app.lock();
                a.lib.focus_prepare();
                a.lib.scroll(enc_delta(v));
            }
            0x08 | 0x28 => {
                let x = self.v14(6, 0x08, cc == 0x28, v);
                self.send(Cmd::MasterLevel(x));
            }
            0x0D | 0x2D => {
                let x = self.v14(6, 0x0D, cc == 0x2D, v);
                self.send(Cmd::HeadphoneLevel(x));
            }
            0x1F | 0x3F => {
                let x = self.v14(6, 0x1F, cc == 0x3F, v);
                self.send(Cmd::Crossfader(x));
            }
            0x17..=0x1A | 0x37..=0x3A => {
                let base = if cc >= 0x37 { cc - 0x20 } else { cc };
                let x = self.v14(6, base, cc >= 0x37, v);
                self.send(Cmd::Filter { deck: (base - 0x17) as usize, v: x });
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ efeitos

    /// Deck controlado pela seção de efeitos do lado `u` (FX1 esquerda, FX2 direita).
    fn fx_deck(&self, u: usize) -> usize {
        self.app.lock().side[u]
    }

    /// Botões de efeito de um lado (FX1 = canal MIDI 5, esquerda; FX2 = canal 6, direita).
    /// Um efeito por deck: FX 1 troca o efeito, FX 2/3 mudam a batida, a alavanca liga.
    fn fx_note(&mut self, u: usize, note: u8, down: bool) {
        let d = self.fx_deck(u);
        match note {
            // Alavanca. O controlador manda 0x47-0x49 quando um botão FX está aceso e 0x50/0x66
            // quando nenhum está; tudo liga e desliga o efeito único do deck.
            0x50 | 0x66 | 0x47..=0x49 => self.send(Cmd::FxActive { deck: d, slot: 0, on: down }),
            // FX 1 (com ou sem SHIFT): mostra o próximo efeito; fica escolhido ao parar de clicar.
            0x70 | 0x63 if down => self.pick_next_fx(u, 1),
            // FX 2 / FX 3 (com ou sem SHIFT): divisão de batida menor / maior.
            0x71 | 0x06 | 0x72 | 0x07 if down => {
                let dir = if note == 0x71 || note == 0x06 { -1 } else { 1 };
                let cur = self.snap().decks[d].fx[0];
                let beats = step_beats(cur.beats, dir);
                self.send(Cmd::FxBeats { deck: d, slot: 0, beats });
                self.app.lock().toast(format!("Deck {} · {} {}", d + 1, cur.kind.name(), beats_label(beats)));
            }
            _ => {}
        }
    }

    /// Avança a lista de efeitos mostrada na tela; o efeito só muda no deck depois de uma pausa.
    fn pick_next_fx(&mut self, u: usize, dir: i32) {
        let d = self.fx_deck(u);
        let current = self.snap().decks[d].fx[0].kind;
        let mut a = self.app.lock();
        let shown = a.fx_pick[u].map_or(current, |(k, _)| k);
        a.fx_pick[u] = Some((shown.next(dir), Instant::now()));
    }

    fn fx_cc(&mut self, u: usize, cc: u8, v: u8) {
        let d = self.fx_deck(u);
        match cc {
            // Knob central LEVEL/DEPTH: força do efeito. O CC muda conforme o botão FX aceso.
            0x02 | 0x04 | 0x06 | 0x22 | 0x24 | 0x26 => {
                let base = if cc >= 0x20 { cc - 0x20 } else { cc };
                let x = self.v14(4 + u, base, cc >= 0x20, v);
                self.send(Cmd::FxLevel { deck: d, slot: 0, v: x });
            }
            // BROWSE girado segurando um botão FX: anda pela lista de efeitos nos dois sentidos.
            0x50..=0x55 => {
                let dir = enc_delta(v).signum();
                if dir != 0 {
                    self.pick_next_fx(u, dir);
                }
            }
            _ => {}
        }
    }

    // --------------------------------------------------------------------- pads

    fn pad(&mut self, pch: usize, note: u8, down: bool) {
        let d = pch / 2;
        let shift = pch % 2 == 1;
        let idx = (note & 0x0F) as usize;
        if d >= NUM_DECKS || idx > 7 {
            return;
        }
        // Teclado da pesquisa aberto: os pads viram setas/confirmar.
        {
            let mut a = self.app.lock();
            if a.search.is_some() {
                if down {
                    crate::search::pad(&mut a, idx);
                }
                return;
            }
        }
        pad_action(&self.app, &self.cmd, &self.snap(), d, PadMode::ALL[(note >> 4) as usize & 7], idx, shift, down);
    }

}

/// O que um pad faz: compartilhado entre os pads da REV1 e os pads da tela (toque).
#[allow(clippy::too_many_arguments)]
pub fn pad_action(app: &SharedApp, cmd: &CmdTx, snap: &Snapshot, d: usize, mode: PadMode, idx: usize, shift: bool, down: bool) {
    if d >= NUM_DECKS || idx > 7 {
        return;
    }
    match mode {
        PadMode::HotCue if shift => {
            if down {
                cmd.send(Cmd::HotcueDelete { deck: d, idx });
            }
        }
        PadMode::HotCue => cmd.send(Cmd::Hotcue { deck: d, idx, down }),
        PadMode::AutoLoop if down => cmd.send(Cmd::LoopAuto { deck: d, beats: AUTOLOOP_PADS[idx] }),
        PadMode::Tracking if down => {
            let semis = TRACKING_SEMIS[idx] - if shift { 12.0 } else { 0.0 };
            cmd.send(Cmd::TrackingPad { deck: d, semis });
        }
        PadMode::Sampler if down => {
            if shift {
                cmd.send(Cmd::SamplerStop(idx));
            } else if let Some(t) = app.lock().samples[idx].clone() {
                cmd.send(Cmd::SamplerPlay { slot: idx, track: t });
            }
        }
        PadMode::BeatJump if down => {
            let mult = if shift { 4.0 } else { 1.0 };
            cmd.send(Cmd::BeatJump { deck: d, beats: BEATJUMP_PADS[idx] * mult });
        }
        PadMode::Roll => cmd.send(Cmd::Roll { deck: d, beats: ROLL_PADS[idx], down }),
        PadMode::Trans => cmd.send(Cmd::Trans { deck: d, beats: TRANS_PADS[idx], down }),
        PadMode::ScratchBank if down => scratch_bank(app, cmd, snap.decks[d].pos, d, idx, shift),
        _ => {}
    }
}

/// Scratch bank: o pad põe o sample no deck; SHIFT + pad devolve a música original.
fn scratch_bank(app: &SharedApp, cmd: &CmdTx, pos: f64, d: usize, idx: usize, restore: bool) {
    let mut a = app.lock();
    if restore {
        if let Some((t, p)) = a.scratch_saved[d].take() {
            a.decks[d].track = Some(t.clone());
            cmd.send(Cmd::Load { deck: d, track: t });
            cmd.send(Cmd::SetPos { deck: d, pos: p });
        }
        return;
    }
    let Some(sample) = a.scratch[idx].clone() else {
        a.toast(format!("Scratch bank {}: vazio (coloque arquivos em ~/DJOS/scratch)", idx + 1));
        return;
    };
    if a.scratch_saved[d].is_none() {
        if let Some(cur) = a.decks[d].track.clone() {
            a.scratch_saved[d] = Some((cur, pos));
        }
    }
    a.decks[d].track = Some(sample.clone());
    cmd.send(Cmd::Load { deck: d, track: sample });
}

/// Estado dos LEDs: lista de (status, nota, valor).
/// Pico do canal (0 a 1) no valor do medidor da controladora: -40 dB apagado, 0 dB tudo aceso.
pub fn meter(peak: f32) -> u8 {
    if peak <= 0.0 {
        return 0;
    }
    let db = 20.0 * peak.log10();
    ((db + 40.0) / 40.0 * 127.0).clamp(0.0, 127.0) as u8
}

pub fn leds(app: &crate::app::AppState, snap: &Snapshot, blink: bool, out: &mut Vec<(u8, u8, u8)>) {
    let b = |x: bool| if x { 0x7F } else { 0x00 };
    for d in 0..NUM_DECKS {
        let s = &snap.decks[d];
        let st = 0x90 | d as u8;
        let play = b((s.playing && !s.cue_preview) || (s.loaded && !s.playing && blink));
        let cue = b(s.loaded && (s.cue_preview || (!s.playing && (s.at_cue || blink))));
        for (n, v) in [
            (0x0B, play),
            (0x47, play),
            (0x0C, cue),
            (0x48, cue),
            (0x58, b(s.sync_ok)),
            (0x5C, b(s.sync_ok)),
            (0x14, b(s.loop_on)),
            (0x50, b(s.loop_on)),
            (0x54, b(s.pfl)),
            (0x68, b(s.pfl)),
            (0x17, b(s.vinyl)),
        ] {
            out.push((st, n, v));
        }
        let mode = app.pad_mode[d];
        for (n, ns, m, ms) in PAD_MODE_NOTES {
            out.push((st, n, b(mode == m)));
            out.push((st, ns, b(mode == ms)));
        }
        let pst = 0x97 + 2 * d as u8;
        for i in 0..8 {
            let (base, on) = match mode {
                PadMode::HotCue => (0x00, s.hotcues[i].is_some()),
                PadMode::AutoLoop => (0x10, s.loop_on && (s.loop_beats - AUTOLOOP_PADS[i]).abs() < 1e-9),
                PadMode::Sampler => (0x30, app.samples[i].is_some() && (!snap.sampler_playing[i] || blink)),
                PadMode::BeatJump => (0x40, true),
                PadMode::Roll => (0x50, (s.roll_beats - ROLL_PADS[i]).abs() < 1e-9),
                PadMode::ScratchBank => (0x70, app.scratch[i].is_some()),
                PadMode::Tracking | PadMode::Trans => continue,
            };
            out.push((pst, base + i as u8, b(on)));
            out.push((pst + 1, base + i as u8, b(on)));
        }
    }
    // Camada de cada lado (deck 3/4 aceso).
    out.push((0x92, 0x72, b(app.side[0] == 2)));
    out.push((0x93, 0x72, b(app.side[1] == 3)));
    // Medidor de cada canal (CC 02, 0 a 127): o deck que está daquele lado, de -40 dB a 0 dB.
    // Vai nos dois canais MIDI do lado (1 e 3 / 2 e 4), valha qual camada estiver acesa.
    for u in 0..2 {
        let v = meter(snap.decks[app.side[u]].peak);
        out.push((0xB0 + u as u8, 0x02, v));
        out.push((0xB2 + u as u8, 0x02, v));
    }
    // Botão FX 1 fica aceso (assim o knob central e a alavanca mandam os códigos do "slot 1");
    // pisca enquanto o efeito está ligado. FX 2 e FX 3 ficam apagados.
    for u in 0..2 {
        let st = 0x94 + u as u8;
        let f = &snap.decks[app.side[u]].fx[0];
        let fx1 = b(!f.active || blink);
        out.push((st, 0x70, fx1));
        out.push((st, 0x63, fx1));
        for n in [0x71, 0x72, 0x06, 0x07] {
            out.push((st, n, 0x00));
        }
    }
}
