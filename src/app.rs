//! Estado do aplicativo compartilhado entre a tela e a controladora, e o carregador de músicas.

use crate::engine::fx::FxKind;
use crate::library::Library;
use crate::shared::{Cmd, CmdTx, NUM_DECKS, SAMPLER_SLOTS};
use crate::track::{self, Track};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// Tempo sem clicar no FX 1 até o efeito mostrado virar o efeito do deck.
pub const FX_PICK_DELAY: Duration = Duration::from_millis(1000);

/// Tempo segurando o BROWSE para abrir a pesquisa.
pub const BROWSE_HOLD: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum PadMode {
    #[default]
    HotCue,
    AutoLoop,
    Tracking,
    Sampler,
    BeatJump,
    Roll,
    Trans,
    ScratchBank,
}

impl PadMode {
    /// Na ordem das notas dos pads da REV1 (0x00 hot cue … 0x70 scratch bank).
    pub const ALL: [PadMode; 8] = [
        PadMode::HotCue,
        PadMode::AutoLoop,
        PadMode::Tracking,
        PadMode::Sampler,
        PadMode::BeatJump,
        PadMode::Roll,
        PadMode::Trans,
        PadMode::ScratchBank,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PadMode::HotCue => "HOT CUE",
            PadMode::AutoLoop => "AUTO LOOP",
            PadMode::Tracking => "TRACKING",
            PadMode::Sampler => "SAMPLER",
            PadMode::BeatJump => "BEAT JUMP",
            PadMode::Roll => "ROLL",
            PadMode::Trans => "TRANS",
            PadMode::ScratchBank => "SCRATCH BANK",
        }
    }

}

#[derive(Default)]
pub struct DeckMeta {
    pub track: Option<Arc<Track>>,
    pub loading: Option<String>,
}

pub struct AppState {
    pub lib: Library,
    pub decks: Vec<DeckMeta>,
    /// Deck ativo em cada lado da controladora: esquerda 0 ou 2, direita 1 ou 3.
    pub side: [usize; 2],
    /// Efeito sendo escolhido com o botão FX 1 em cada lado (esquerda, direita): o efeito mostrado
    /// e a hora do último clique. Vira o efeito do deck depois de `FX_PICK_DELAY` sem clicar.
    pub fx_pick: [Option<(FxKind, Instant)>; 2],
    pub pad_mode: [PadMode; NUM_DECKS],
    pub shift: bool,
    pub master_cue: bool,
    pub big_browser: bool,
    /// Browser novo em tela cheia (src/navegador.rs).
    pub nav: crate::navegador::Nav,
    /// Configurações salvas e a tela de configurações (aberta ou não).
    pub settings: crate::settings::Settings,
    pub settings_ui: Option<crate::settings::SettingsUi>,
    /// Bluetooth: modo (desligado, master, retorno), texto de estado e atraso do master com fio.
    pub bt_mode: crate::bluetooth::BtMode,
    pub bt_status: String,
    pub phone_status: String,
    pub fw: crate::firmware::FwState,
    pub rec: crate::record::RecState,
    pub history: crate::history::Tracker,
    /// Ajuste da grade aberto (tocar no BPM do deck).
    pub grid_edit: Option<crate::grid::GridEdit>,
    /// Última aba das configurações (abre nela de novo).
    pub settings_tab: usize,
    /// Nome da placa de vídeo (para o modo leve automático).
    pub gpu: String,
    pub bt_delay_ms: f32,
    /// Cópia de pasta para as músicas internas (andamento).
    pub import: Option<crate::import::ImportProgress>,
    /// Apagar pedido: o que e quando (precisa confirmar em 4 s).
    pub delete_armed: Option<(PathBuf, Instant)>,
    /// Nível de zoom da waveform (índice em `ui::ZOOMS`).
    pub wave_zoom: usize,
    /// Teclado da pesquisa aberto (com o texto digitado e a tecla em destaque).
    pub search: Option<crate::search::SearchUi>,
    /// BROWSE apertado desde quando, e se já virou "segurar" (abre a pesquisa).
    pub browse_down: Option<Instant>,
    pub browse_long: bool,
    pub toast: Option<(String, Instant)>,
    pub samples: Vec<Option<Arc<Track>>>,
    pub scratch: Vec<Option<Arc<Track>>>,
    /// Música original guardada enquanto um sample do scratch bank está no deck.
    pub scratch_saved: Vec<Option<(Arc<Track>, f64)>>,
    pub midi_status: String,
    pub audio_status: String,
}

impl AppState {
    pub fn new() -> Self {
        let settings = crate::settings::Settings::load();
        Self {
            lib: Library::new(),
            decks: (0..NUM_DECKS).map(|_| DeckMeta::default()).collect(),
            side: [0, 1],
            fx_pick: [None, None],
            pad_mode: [PadMode::HotCue; NUM_DECKS],
            shift: false,
            master_cue: false,
            big_browser: false,
            nav: Default::default(),
            settings_ui: None,
            bt_mode: settings.bt_mode,
            bt_status: "BT desligado".into(),
            phone_status: "desligado".into(),
            fw: Default::default(),
            rec: Default::default(),
            history: Default::default(),
            grid_edit: None,
            settings_tab: 0,
            gpu: String::new(),
            bt_delay_ms: std::env::var("DJOS_BT_ATRASO_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(settings.bt_delay_ms),
            settings,
            import: None,
            delete_armed: None,
            wave_zoom: 2,
            search: None,
            browse_down: None,
            browse_long: false,
            toast: None,
            samples: vec![None; SAMPLER_SLOTS],
            scratch: vec![None; SAMPLER_SLOTS],
            scratch_saved: vec![None; NUM_DECKS],
            midi_status: "procurando controladora…".into(),
            audio_status: String::new(),
        }
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    /// Segurar o BROWSE por `BROWSE_HOLD` abre a pesquisa (chamado a cada quadro da tela).
    pub fn tick_browse_hold(&mut self) {
        if let Some(t) = self.browse_down {
            if !self.browse_long && t.elapsed() >= BROWSE_HOLD && self.settings_ui.is_none() {
                self.browse_long = true;
                crate::search::open(self);
            }
        }
    }

    /// Soltou o BROWSE: toque curto alterna pastas/músicas (ou digita a tecla, com o teclado aberto).
    pub fn browse_released(&mut self) {
        let was_long = self.browse_long;
        self.browse_down = None;
        self.browse_long = false;
        if was_long {
            return;
        }
        if self.search.is_some() {
            crate::search::press(self);
        } else if crate::navegador::is_on(self) {
            crate::navegador::press(self);
        } else {
            self.lib.toggle_focus();
        }
    }

    /// Apagar (das músicas internas) em dois toques: o primeiro avisa, o segundo apaga.
    pub fn delete_selected(&mut self) {
        let path = if crate::navegador::is_on(self) { self.nav.selected_path() } else { self.lib.selected_path() };
        let Some(path) = path else { return };
        let allowed = [crate::import::internal_dir(), crate::record::dir()].iter().any(|r| path.starts_with(r) && path != *r);
        if !allowed {
            self.toast("Só dá para apagar o que está nas músicas internas ou nas gravações");
            return;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let confirmed = self.delete_armed.as_ref().is_some_and(|(p, t)| *p == path && t.elapsed() < Duration::from_secs(4));
        if !confirmed {
            self.delete_armed = Some((path, Instant::now()));
            self.toast(format!("Apagar \"{name}\"? Aperte de novo para confirmar"));
            return;
        }
        self.delete_armed = None;
        match crate::import::delete_internal(&path) {
            Ok(()) => {
                self.lib.force_reload();
                self.toast(format!("\"{name}\" apagado"));
            }
            Err(e) => self.toast(format!("Não apagou: {e}")),
        }
    }

    /// Confirma as escolhas de efeito que ficaram paradas por `FX_PICK_DELAY`.
    pub fn commit_fx_picks(&mut self, cmd: &CmdTx) {
        for side in 0..2 {
            if let Some((kind, t)) = self.fx_pick[side] {
                if t.elapsed() >= FX_PICK_DELAY {
                    self.fx_pick[side] = None;
                    cmd.send(Cmd::FxKind { deck: self.side[side], slot: 0, kind });
                }
            }
        }
    }
}

pub type SharedApp = Arc<Mutex<AppState>>;

/// Pasta do sistema na casa do usuário (samples, scratch bank, configurações).
pub use dj_os_analise::data_dir;

/// O sistema se chamava "Diomar XDJ": a pasta de dados antiga (~/DiomarXDJ) vira ~/DJOS.
pub fn migrate_old_data_dir() {
    let new = data_dir();
    let old = new.with_file_name("DiomarXDJ");
    if old.is_dir() && !new.exists() {
        match std::fs::rename(&old, &new) {
            Ok(()) => eprintln!("[djos] dados movidos de {} para {}", old.display(), new.display()),
            Err(e) => eprintln!("[djos] não moveu {}: {e}", old.display()),
        }
    }
}

enum Job {
    /// `resume`: posição (frames) para voltar depois de um travamento.
    Deck { deck: usize, path: PathBuf, resume: Option<f64> },
    Banks,
}

/// Decodifica e analisa músicas em segundo plano.
#[derive(Clone)]
pub struct Loader {
    tx: mpsc::Sender<Job>,
}

impl Loader {
    pub fn start(app: SharedApp, cmd: CmdTx) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("carregador".into())
            .spawn(move || {
                for job in rx {
                    match job {
                        Job::Deck { deck, path, resume } => load_deck(&app, &cmd, deck, &path, resume),
                        Job::Banks => load_banks(&app),
                    }
                }
            })
            .expect("thread do carregador");
        let l = Self { tx };
        let _ = l.tx.send(Job::Banks);
        l
    }

    pub fn load_deck(&self, app: &mut AppState, deck: usize, path: PathBuf) {
        if deck >= NUM_DECKS {
            return;
        }
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        app.decks[deck].loading = Some(name);
        let _ = self.tx.send(Job::Deck { deck, path, resume: None });
    }

    /// Recarrega uma música e volta para `pos` (retomar depois de um travamento).
    pub fn resume_deck(&self, app: &mut AppState, deck: usize, path: PathBuf, pos: f64) {
        if deck >= NUM_DECKS {
            return;
        }
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        app.decks[deck].loading = Some(name);
        let _ = self.tx.send(Job::Deck { deck, path, resume: Some(pos) });
    }

}

fn load_deck(app: &SharedApp, cmd: &CmdTx, deck: usize, path: &std::path::Path, resume: Option<f64>) {
    let started = Instant::now();
    let mut saved = crate::store::load(path);
    // Primeira vez de uma música de pendrive do rekordbox: grade, tom e cues dele.
    let loaded = match saved.is_none().then(|| crate::rekordbox::load_track(path)).flatten() {
        Some(r) => r.map(|(t, s)| {
            crate::store::save(path, &s);
            saved = Some(s);
            t
        }),
        None => {
            let cached = saved.as_ref().map(|s| (track::Waveform { low: s.low.clone(), mid: s.mid.clone(), high: s.high.clone() }, s.bpm, s.first_beat, s.key, s.loudness));
            track::load_cached(path, true, cached)
        }
    };
    match loaded {
        Ok(t) => {
            let t = Arc::new(t);
            match &saved {
                // Primeira vez: guarda a análise para abrir rápido nas próximas.
                None => crate::store::save(path, &crate::store::Saved::from_track(&t)),
                // Análise de antes do tom/volume existirem: guarda o que foi calculado agora
                // (mantém os cues).
                Some(s) if s.key != t.key || s.loudness.is_nan() != t.loudness.is_nan() => {
                    crate::store::save(path, &crate::store::Saved { key: t.key, loudness: t.loudness, ..s.clone() })
                }
                _ => {}
            }
            eprintln!(
                "[djos] deck {}: {} ({:.1} BPM, {:.0} ms)",
                deck + 1,
                t.display_name(),
                t.bpm(),
                started.elapsed().as_secs_f64() * 1000.0
            );
            let mut a = app.lock();
            a.decks[deck].track = Some(t.clone());
            a.decks[deck].loading = None;
            a.scratch_saved[deck] = None;
            cmd.send(Cmd::Load { deck, track: t });
            if let Some(s) = saved {
                cmd.send(Cmd::RestoreCues { deck, cue: s.cue, hotcues: s.hotcues, loop_: s.loop_ });
            }
            if let Some(pos) = resume {
                cmd.send(Cmd::SetPos { deck, pos });
            }
        }
        Err(e) => {
            let mut a = app.lock();
            a.decks[deck].loading = None;
            a.toast(format!("Não deu para carregar: {e}"));
        }
    }
}

/// Carrega os samples (pasta `samples`) e o scratch bank (pasta `scratch`), 8 de cada.
fn load_banks(app: &SharedApp) {
    for (dir, is_scratch) in [("samples", false), ("scratch", true)] {
        let base = data_dir().join(dir);
        let _ = std::fs::create_dir_all(&base);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&base)
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect())
            .unwrap_or_default();
        files.sort();
        let loaded: Vec<Option<Arc<Track>>> = (0..SAMPLER_SLOTS)
            .map(|i| files.get(i).and_then(|p| track::load(p, is_scratch).ok()).map(Arc::new))
            .collect();
        let mut a = app.lock();
        if is_scratch {
            a.scratch = loaded;
        } else {
            a.samples = loaded;
        }
    }
}
