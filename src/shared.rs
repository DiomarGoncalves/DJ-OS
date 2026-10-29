//! Comunicação entre as partes: comandos para o motor de áudio e o retrato do estado dele.

use crate::engine::fx::FxKind;
use crate::track::Track;
use parking_lot::Mutex;
use std::sync::Arc;

pub const NUM_DECKS: usize = 4;
/// Um efeito por deck (como na DDJ-REV1).
pub const FX_SLOTS: usize = 1;
pub const HOTCUES: usize = 8;
pub const SAMPLER_SLOTS: usize = 8;

/// Comandos para o motor de áudio. Valores de knob/fader vão de 0.0 a 1.0.
#[derive(Debug)]
pub enum Cmd {
    Load { deck: usize, track: Arc<Track> },
    CloneDeck { from: usize, to: usize },
    SetPos { deck: usize, pos: f64 },
    /// Devolve cue, hotcues e loop salvos da música (logo depois do `Load`).
    RestoreCues { deck: usize, cue: Option<f64>, hotcues: [Option<f64>; HOTCUES], loop_: Option<(f64, f64, f64)> },
    PlayToggle(usize),
    Play(usize),
    Cue { deck: usize, down: bool },
    /// Volta ao cue mantendo o estado de reprodução.
    GotoCue(usize),
    /// Volta ao cue e para (CUE do fader start).
    CueStop(usize),
    GotoStart(usize),
    Stutter(usize),
    Tempo { deck: usize, v: f64 },
    PitchRangeCycle(usize),
    /// Faixa de pitch (0,08 / 0,16 / 0,50).
    SetPitchRange { deck: usize, range: f64 },
    /// Curva do crossfader: 0 = suave (os dois cheios no meio), 1 = corte (scratch), 2 = potência constante.
    XfCurve(u8),
    PitchReset(usize),
    Sync(usize),
    Vinyl { deck: usize, on: bool },
    Keylock { deck: usize, on: bool },
    JogTouch { deck: usize, down: bool },
    JogScratch { deck: usize, ticks: f64 },
    JogBend { deck: usize, ticks: f64 },
    JogSearch { deck: usize, ticks: f64 },
    Hotcue { deck: usize, idx: usize, down: bool },
    HotcueDelete { deck: usize, idx: usize },
    LoopAuto { deck: usize, beats: f64 },
    LoopToggle(usize),
    LoopHalve(usize),
    LoopDouble(usize),
    LoopIn(usize),
    LoopOut(usize),
    ReloopExit(usize),
    BeatJump { deck: usize, beats: f64 },
    Roll { deck: usize, beats: f64, down: bool },
    Trans { deck: usize, beats: f64, down: bool },
    TrackingPad { deck: usize, semis: f64 },
    Trim { deck: usize, v: f32 },
    /// Ganho automático (nivela o volume das músicas) ligado/desligado em todos os decks.
    AutoGain(bool),
    Eq { deck: usize, band: usize, v: f32 },
    Filter { deck: usize, v: f32 },
    Fader { deck: usize, v: f32 },
    Pfl { deck: usize, on: bool },
    Crossfader(f32),
    MasterLevel(f32),
    HeadphoneLevel(f32),
    /// 0 = só cue, 1 = só master.
    HeadphoneMix(f32),
    FxKind { deck: usize, slot: usize, kind: FxKind },
    FxActive { deck: usize, slot: usize, on: bool },
    FxLevel { deck: usize, slot: usize, v: f32 },
    FxBeats { deck: usize, slot: usize, beats: f32 },
    SamplerPlay { slot: usize, track: Arc<Track> },
    SamplerStop(usize),
    /// Liga a fila que leva o áudio para a caixa Bluetooth (a outra ponta fica no fluxo do Bluetooth).
    BtAttach(rtrb::Producer<[f32; 2]>),
    BtDetach,
    /// Gravação do set: o motor manda o master (depois do limitador) para esta fila.
    RecAttach(rtrb::Producer<[f32; 2]>),
    RecDetach,
    /// O que vai para o Bluetooth: 1 = master, 2 = fone (retorno).
    BtSource(u8),
    /// Atraso do master com fio (saídas 1-2), em ms, para alinhar com a caixa Bluetooth.
    MasterDelay(f32),
    /// Som ambiente no master (celular por Bluetooth): volume e "abaixar quando um deck toca".
    AmbientVolume(f32),
    AmbientDuck(bool),
    /// Som recebido do celular por Bluetooth (o sistema como caixa de som).
    LineInAttach(rtrb::Consumer<[f32; 2]>),
    LineInDetach,
    /// Para onde vai o som do celular: um deck (no lugar da música) ou `None` = master ambiente.
    LineInRoute(Option<usize>),
}

/// Envio de comandos (várias origens: interface, controladora, carregador).
#[derive(Clone)]
pub struct CmdTx(Arc<Mutex<rtrb::Producer<Cmd>>>);

impl CmdTx {
    pub fn new(p: rtrb::Producer<Cmd>) -> Self {
        Self(Arc::new(Mutex::new(p)))
    }

    pub fn send(&self, c: Cmd) {
        let mut p = self.0.lock();
        if p.is_abandoned() {
            return;
        }
        if p.push(c).is_err() {
            eprintln!("[djos] fila de comandos cheia, comando descartado");
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct FxSnap {
    pub kind: FxKind,
    pub active: bool,
    pub sounding: bool,
    pub level: f32,
    pub beats: f32,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Default)]
pub struct DeckSnap {
    pub loaded: bool,
    pub track_gen: u64,
    pub pos: f64,
    pub frames: f64,
    pub sr: f64,
    pub playing: bool,
    pub cue_preview: bool,
    pub cue: f64,
    pub at_cue: bool,
    pub hotcues: [Option<f64>; HOTCUES],
    pub loop_on: bool,
    pub loop_in: Option<f64>,
    pub loop_out: Option<f64>,
    pub loop_beats: f64,
    pub roll_beats: f64,
    pub bpm_track: f64,
    pub bpm: f64,
    pub rate: f64,
    pub pitch_range: f64,
    pub key_semis: f64,
    pub sync_ok: bool,
    pub vinyl: bool,
    pub keylock: bool,
    pub pfl: bool,
    pub fader: f32,
    pub peak: f32,
    /// Correção do ganho automático aplicada à música carregada (dB).
    pub auto_gain_db: f32,
    /// Ganho atual do EQ (grave, médio, agudo) e do TRIM, para a waveform reagir aos knobs.
    pub eq: [f32; 3],
    pub trim: f32,
    /// O deck está tocando o som do celular (Bluetooth) em vez de uma música.
    pub live: bool,
    pub fx: [FxSnap; FX_SLOTS],
}

#[derive(Clone, Copy, Default)]
pub struct Snapshot {
    pub decks: [DeckSnap; NUM_DECKS],
    pub master_peak: [f32; 2],
    /// Menor ganho do limitador do master desde a última leitura (1 = não está limitando).
    pub master_limit: f32,
    pub sampler_playing: [bool; SAMPLER_SLOTS],
    pub sample_rate: f64,
}

pub type SharedSnap = Arc<Mutex<Snapshot>>;
