//! Configurações dentro do sistema (nada de configurar fora do app).
//!
//! Abre com SHIFT + MASTER CUE, com a engrenagem da barra de cima ou F10. Girar o BROWSE escolhe
//! o item, apertar muda o valor; dá para tocar nos itens. Os valores ficam em
//! `~/DJOS/config.txt` (uma linha `chave=valor` cada) e são aplicados ao abrir o sistema.

use crate::app::{data_dir, AppState, SharedApp};
use crate::bluetooth::BtMode;
use crate::shared::{Cmd, CmdTx, NUM_DECKS};

pub const PALETTES: [&str; 3] = ["AZUL/LARANJA", "RGB", "AZUL"];
pub const SCALES: [f32; 3] = [1.0, 1.15, 1.3];
/// Folga do som (amostras por bloco a 48 kHz): mais folga, menos risco de estalo.
pub const AUDIO_BUFFERS: [u32; 3] = [256, 512, 1024];
/// Girar o BROWSE na tela principal.
pub const BROWSE_GIRA: [&str; 2] = ["ABRE O BROWSER", "ZOOM DA WAVEFORM"];
/// Aviso de fim da música (segundos antes do fim; 0 = desligado).
pub const FIM_AVISOS: [u32; 4] = [0, 30, 45, 60];
/// Consertos do BPM da grade (aba FUNÇÕES).
pub const GRADE_RAZOES: [(&str, f64); 4] = [("× 1,5", 1.5), ("× 2/3", 2.0 / 3.0), ("× 2", 2.0), ("÷ 2", 0.5)];
/// Tamanho da letra do browser novo.
pub const BROWSER_FONTS: [&str; 3] = ["PEQUENA", "MÉDIA", "GRANDE"];
pub const PITCH_RANGES: [f64; 3] = [0.08, 0.16, 0.50];
pub const XF_CURVES: [&str; 3] = ["SUAVE", "CORTE (SCRATCH)", "POTÊNCIA CONSTANTE"];
pub const FADER_CURVES: [&str; 3] = ["LENTA", "NORMAL", "RÁPIDA (CORTE)"];
/// Lado de cada deck no crossfader (o índice vai para o motor: 0 A, 1 fora, 2 B).
pub const XF_SIDES: [&str; 3] = ["A (ESQUERDA)", "THRU (FORA DO CROSSFADER)", "B (DIREITA)"];
const XF_SIDE_CHARS: [char; 3] = ['A', 'T', 'B'];
pub const LIGHT_MODES: [&str; 3] = ["AUTOMÁTICO", "LIGADO", "DESLIGADO"];
pub const AMBIENT_VOLS: [f32; 4] = [0.25, 0.5, 0.75, 1.0];
const BT_DELAYS: [f32; 9] = [0.0, 50.0, 100.0, 150.0, 200.0, 250.0, 300.0, 400.0, 500.0];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub vinyl: bool,
    pub pitch_range: usize,
    pub xf_curve: usize,
    pub keylock: bool,
    pub palette: usize,
    pub scale: usize,
    pub bt_mode: BtMode,
    pub bt_delay_ms: f32,
    /// Celular no master: volume (índice em AMBIENT_VOLS) e abaixar quando um deck toca.
    pub ambient_vol: usize,
    pub ambient_duck: bool,
    /// Modo leve: 0 automático (pela placa), 1 ligado, 2 desligado.
    pub light_mode: usize,
    /// Som do celular por Bluetooth: índice em `phone::ROUTES` (0 = desligado).
    pub phone_in: usize,
    /// Ganho automático: nivela o volume das músicas.
    pub auto_gain: bool,
    /// Onde gravar o set: índice em `record::TARGETS`.
    pub rec_target: usize,
    /// Tela principal com os 4 decks (senão, os 2 da frente).
    pub four_decks: bool,
    /// EQ como isolador (corte total no mínimo) ou EQ (-26 dB).
    pub eq_iso: bool,
    pub fader_curve: usize,
    /// Lado de cada deck no crossfader: índice em XF_SIDES.
    pub xf_side: [usize; NUM_DECKS],
    /// Fone em mono dividido (cue à esquerda, master à direita).
    pub hp_split: bool,
    /// Slip: depois do scratch ou do loop, a música volta onde estaria.
    pub slip: bool,
    /// Browser em tela cheia: o novo (estilo cabine) ou o de árvore; e as colunas do novo.
    pub browser_novo: bool,
    pub browser_bpm: bool,
    pub browser_key: bool,
    /// Tamanho da letra do browser novo (índice em BROWSER_FONTS).
    pub browser_font: usize,
    /// O que girar o BROWSE faz na tela principal (índice em BROWSE_GIRA).
    pub browse_gira: usize,
    /// Folga do som (índice em AUDIO_BUFFERS).
    pub audio_buffer: usize,
    /// Saída do som escolhida (vazio = automática) e o nome dela para mostrar.
    pub saida_som: String,
    pub saida_som_nome: String,
    /// Skin da tela (nome; vazio = a de fábrica).
    pub skin: String,
    /// QUANTIZE: cue, hot cue e loop caem na batida (como nas CDJs).
    pub quantize: bool,
    /// Trava do LOAD: não carrega outra música num deck tocando no ar.
    pub trava_load: bool,
    /// Aviso de fim da música (índice em FIM_AVISOS).
    pub fim_aviso: usize,
    /// Play na batida (como o Smart Play do VirtualDJ).
    pub play_batida: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            vinyl: true,
            pitch_range: 0,
            xf_curve: 0,
            keylock: false,
            palette: 0,
            scale: 0,
            bt_mode: BtMode::Off,
            bt_delay_ms: 0.0,
            ambient_vol: 2,
            ambient_duck: true,
            light_mode: 0,
            phone_in: 0,
            auto_gain: true,
            rec_target: 0,
            four_decks: true,
            eq_iso: true,
            fader_curve: 1,
            xf_side: std::array::from_fn(|d| if d % 2 == 0 { 0 } else { 2 }),
            hp_split: false,
            slip: false,
            browser_novo: true,
            browser_bpm: true,
            browser_key: true,
            browser_font: 0,
            browse_gira: 0,
            audio_buffer: 1,
            saida_som: String::new(),
            saida_som_nome: String::new(),
            skin: String::new(),
            // O que muda o jeito de tocar vem desligado (como no VirtualDJ): liga quem quiser.
            quantize: false,
            trava_load: true,
            fim_aviso: 1,
            play_batida: false,
        }
    }
}

impl Settings {
    fn path() -> std::path::PathBuf {
        data_dir().join("config.txt")
    }

    pub fn parse(text: &str) -> Self {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let num = |max: usize| v.parse::<usize>().ok().filter(|n| *n < max);
            match k {
                "vinil" => s.vinyl = v == "sim",
                "faixa_pitch" => s.pitch_range = num(PITCH_RANGES.len()).unwrap_or(0),
                "crossfader" => s.xf_curve = num(XF_CURVES.len()).unwrap_or(0),
                "keylock" => s.keylock = v == "sim",
                "cores" => s.palette = num(PALETTES.len()).unwrap_or(0),
                "tamanho" => s.scale = num(SCALES.len()).unwrap_or(0),
                "bluetooth" => {
                    s.bt_mode = match v {
                        "master" => BtMode::Master,
                        "retorno" => BtMode::Retorno,
                        _ => BtMode::Off,
                    }
                }
                "atraso_bt_ms" => s.bt_delay_ms = v.parse().unwrap_or(0.0f32).clamp(0.0, 500.0),
                "volume_ambiente" => s.ambient_vol = num(AMBIENT_VOLS.len()).unwrap_or(2),
                "abaixar_ambiente" => s.ambient_duck = v == "sim",
                "modo_leve" => s.light_mode = num(LIGHT_MODES.len()).unwrap_or(0),
                "celular_bt" => s.phone_in = num(crate::phone::ROUTES.len()).unwrap_or(0),
                "ganho_automatico" => s.auto_gain = v == "sim",
                "gravar_em" => s.rec_target = num(crate::record::TARGETS.len()).unwrap_or(0),
                "quatro_decks" => s.four_decks = v == "sim",
                "eq_isolador" => s.eq_iso = v != "nao",
                "curva_fader" => s.fader_curve = num(FADER_CURVES.len()).unwrap_or(1),
                "lado_crossfader" => {
                    for (d, c) in v.chars().take(NUM_DECKS).enumerate() {
                        if let Some(i) = XF_SIDE_CHARS.iter().position(|x| *x == c) {
                            s.xf_side[d] = i;
                        }
                    }
                }
                "fone_dividido" => s.hp_split = v == "sim",
                "slip" => s.slip = v == "sim",
                "browser_novo" => s.browser_novo = v == "sim",
                "browser_bpm" => s.browser_bpm = v != "nao",
                "browser_tom" => s.browser_key = v != "nao",
                "browser_letra" => s.browser_font = num(BROWSER_FONTS.len()).unwrap_or(0),
                "browse_gira" => s.browse_gira = num(BROWSE_GIRA.len()).unwrap_or(0),
                "buffer_som" => s.audio_buffer = num(AUDIO_BUFFERS.len()).unwrap_or(1),
                "saida_som" => s.saida_som = v.to_string(),
                "saida_som_nome" => s.saida_som_nome = v.to_string(),
                "skin" => s.skin = v.to_string(),
                "quantize" => s.quantize = v == "sim",
                "trava_load" => s.trava_load = v != "nao",
                "aviso_fim" => s.fim_aviso = num(FIM_AVISOS.len()).unwrap_or(1),
                "play_na_batida" => s.play_batida = v == "sim",
                _ => {}
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        let yn = |b: bool| if b { "sim" } else { "nao" };
        let bt = match self.bt_mode {
            BtMode::Off => "desligado",
            BtMode::Master => "master",
            BtMode::Retorno => "retorno",
        };
        format!(
            "vinil={}\nfaixa_pitch={}\ncrossfader={}\nkeylock={}\ncores={}\ntamanho={}\nbluetooth={bt}\natraso_bt_ms={}\n\
             volume_ambiente={}\nabaixar_ambiente={}\nmodo_leve={}\ncelular_bt={}\nganho_automatico={}\ngravar_em={}\nquatro_decks={}\n\
             eq_isolador={}\ncurva_fader={}\nlado_crossfader={}\nfone_dividido={}\nslip={}\n\
             browser_novo={}\nbrowser_bpm={}\nbrowser_tom={}\nbrowser_letra={}\nbrowse_gira={}\nbuffer_som={}\nsaida_som={}\nsaida_som_nome={}\nskin={}\n\
             quantize={}\ntrava_load={}\naviso_fim={}\nplay_na_batida={}\n",
            yn(self.vinyl),
            self.pitch_range,
            self.xf_curve,
            yn(self.keylock),
            self.palette,
            self.scale,
            self.bt_delay_ms,
            self.ambient_vol,
            yn(self.ambient_duck),
            self.light_mode,
            self.phone_in,
            yn(self.auto_gain),
            self.rec_target,
            yn(self.four_decks),
            yn(self.eq_iso),
            self.fader_curve,
            self.xf_side.iter().map(|&i| XF_SIDE_CHARS[i]).collect::<String>(),
            yn(self.hp_split),
            yn(self.slip),
            yn(self.browser_novo),
            yn(self.browser_bpm),
            yn(self.browser_key),
            self.browser_font,
            self.browse_gira,
            self.audio_buffer,
            self.saida_som,
            self.saida_som_nome,
            self.skin,
            yn(self.quantize),
            yn(self.trava_load),
            self.fim_aviso,
            yn(self.play_batida)
        )
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).map(|t| Self::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) {
        if cfg!(test) {
            return; // os testes não mexem no config.txt de verdade
        }
        let _ = std::fs::create_dir_all(data_dir());
        if let Err(e) = std::fs::write(Self::path(), self.to_text()) {
            eprintln!("[djos] não salvou as configurações: {e}");
        }
    }

    /// Manda para o motor o que é do motor (vale para os 4 decks).
    pub fn apply(&self, cmd: &CmdTx) {
        for deck in 0..NUM_DECKS {
            cmd.send(Cmd::Vinyl { deck, on: self.vinyl });
            cmd.send(Cmd::SetPitchRange { deck, range: PITCH_RANGES[self.pitch_range] });
            cmd.send(Cmd::Keylock { deck, on: self.keylock });
            cmd.send(Cmd::Slip { deck, on: self.slip });
            cmd.send(Cmd::XfAssign { deck, side: self.xf_side[deck] as u8 });
        }
        cmd.send(Cmd::XfCurve(self.xf_curve as u8));
        cmd.send(Cmd::Quantize(self.quantize));
        cmd.send(Cmd::SmartPlay(self.play_batida));
        cmd.send(Cmd::EqIso(self.eq_iso));
        cmd.send(Cmd::FaderCurve(self.fader_curve as u8));
        cmd.send(Cmd::HpSplit(self.hp_split));
        cmd.send(Cmd::AmbientVolume(AMBIENT_VOLS[self.ambient_vol]));
        cmd.send(Cmd::AmbientDuck(self.ambient_duck));
        cmd.send(Cmd::LineInRoute(crate::phone::route_target(self.phone_in)));
        cmd.send(Cmd::AutoGain(self.auto_gain));
        *crate::pw_out::PREFERIDA.lock() = self.saida_som.clone();
        crate::pw_out::BUFFER.store(AUDIO_BUFFERS[self.audio_buffer.min(AUDIO_BUFFERS.len() - 1)], std::sync::atomic::Ordering::Relaxed);
    }
}

/// Rede Wi-Fi vista na última busca.
#[derive(Clone, Debug)]
pub struct WifiNet {
    pub ssid: String,
    pub signal: u8,
    pub secure: bool,
    pub active: bool,
}

/// O que acontece ao apertar um item.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    WifiScan,
    WifiPick(String, bool),
    BtMode,
    BtDelay,
    BtScan,
    BtPair(String, String),
    PhoneIn,
    AutoGain,
    RecTarget,
    FwSearch,
    FwInstall(std::path::PathBuf),
    Restart,
    PowerOff,
    Vinyl,
    PitchRange,
    XfCurve,
    XfSide(usize),
    EqMode,
    FaderCurve,
    HpSplit,
    Slip,
    Automix,
    Gravar,
    TomDeck(usize, crate::app::Tom),
    /// Conserto da grade do deck: índice em GRADE_RAZOES, ou (o seguinte) "batida aqui".
    Grade(usize, usize),
    Reverso(usize),
    Quantize,
    PlayBatida,
    TravaLoad,
    FimAviso,
    BrowserNovo,
    FiltroBrowser,
    SalvarFila,
    BrowserBpm,
    BrowserKey,
    BrowserFont,
    BrowseGira,
    AudioBuffer,
    SaidaSom,
    Skin,
    AprenderFuncao,
    AprenderDeck,
    AprenderAgora,
    AprenderApagar,
    Keylock,
    Palette,
    Scale,
    AmbientVol,
    AmbientDuck,
    LightMode,
    FourDecks,
    AnalyzeAll,
    MidiLog,
}

pub enum Row {
    Header(&'static str),
    Item { label: String, value: String, action: Action },
    Info(String),
}

/// Estado da tela de configurações aberta.
#[derive(Default)]
pub struct SettingsUi {
    /// Aba aberta (índice em Tab::all()).
    pub tab: usize,
    /// O BROWSE está nas abas (true) ou nos itens da aba (false).
    pub on_tabs: bool,
    pub sel: usize,
    /// Ação perigosa esperando o segundo toque (instalar firmware, reiniciar, desligar).
    pub armed: Option<(Action, std::time::Instant)>,
    pub wifi: Vec<WifiNet>,
    pub wifi_msg: String,
    pub bt_devices: Vec<(String, String)>,
    pub bt_msg: String,
    /// MIDI learn: função escolhida (índice em midi::aprender::FUNCOES) e deck.
    pub aprender_funcao: usize,
    pub aprender_deck: usize,
}

/// Abas da tela de configurações (estilo XDJ: abas à esquerda, itens à direita).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    /// O que se usa no meio do set, tudo pelo BROWSE (o aparelho não tem tela de toque).
    Funcoes,
    Audio,
    Mixer,
    Bluetooth,
    Phone,
    Wifi,
    Controller,
    Screen,
    System,
}

impl Tab {
    /// Abas que aparecem: no PC ficam de fora as do sistema do aparelho (Bluetooth, celular, Wi-Fi).
    pub fn all() -> &'static [Tab] {
        const APARELHO: [Tab; 9] = [Tab::Funcoes, Tab::Audio, Tab::Mixer, Tab::Bluetooth, Tab::Phone, Tab::Wifi, Tab::Controller, Tab::Screen, Tab::System];
        const PC: [Tab; 6] = [Tab::Funcoes, Tab::Audio, Tab::Mixer, Tab::Controller, Tab::Screen, Tab::System];
        if crate::app::aparelho() {
            &APARELHO
        } else {
            &PC
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tab::Funcoes => "FUNÇÕES",
            Tab::Audio => "ÁUDIO",
            Tab::Mixer => "MIXER",
            Tab::Bluetooth => "CAIXA BLUETOOTH",
            Tab::Phone => "CELULAR",
            Tab::Wifi => "WI-FI",
            Tab::Controller => "CONTROLADORA",
            Tab::Screen => "TELA",
            Tab::System => "SISTEMA",
        }
    }
}

/// Resumo que aparece embaixo do nome da aba.
pub fn tab_summary(app: &AppState, tab: Tab) -> String {
    let s = &app.settings;
    match tab {
        Tab::Funcoes => match (app.automix.is_some(), app.rec.active.is_some()) {
            (true, true) => "automix · gravando".into(),
            (true, false) => "automix ligado".into(),
            (false, true) => "gravando".into(),
            _ => "automix · gravar · tom · bpm".into(),
        },
        Tab::Audio => if app.audio_status.contains("DDJ-REV1") { "DDJ-REV1".into() } else if app.audio_status.starts_with(crate::pw_out::NO_OUTPUT) { "sem saída".into() } else { "sem a REV1".into() },
        Tab::Mixer => format!("{} · fone {}", if s.eq_iso { "isolador" } else { "eq" }, if s.hp_split { "dividido" } else { "estéreo" }),
        Tab::Bluetooth => app.bt_mode.name().to_lowercase(),
        Tab::Phone => crate::phone::ROUTES[s.phone_in].to_lowercase(),
        Tab::Wifi => app.settings_ui.as_ref().and_then(|u| u.wifi.iter().find(|n| n.active)).map_or("—".into(), |n| n.ssid.clone()),
        Tab::Controller => format!("{} · ±{:.0}%{}", if s.vinyl { "vinil" } else { "cdj" }, PITCH_RANGES[s.pitch_range] * 100.0, if s.slip { " · slip" } else { "" }),
        Tab::Screen => PALETTES[s.palette].to_lowercase(),
        Tab::System => match app.fw.info.split(" · ").find_map(|p| p.strip_prefix("firmware de ")) {
            Some(date) => format!("firmware {date}"),
            None => app.fw.info.split(" · ").next().unwrap_or("").replace("Versão ", "v"),
        },
    }
}

/// Aba aberta agora.
pub fn current_tab(app: &AppState) -> Tab {
    Tab::all()[app.settings_ui.as_ref().map_or(0, |u| u.tab).min(Tab::all().len() - 1)]
}

/// Itens da aba aberta, com os valores atuais.
pub fn rows(app: &AppState) -> Vec<Row> {
    rows_for(app, current_tab(app))
}

pub fn rows_for(app: &AppState, tab: Tab) -> Vec<Row> {
    let s = &app.settings;
    let ui = app.settings_ui.as_ref();
    // Ação perigosa: o primeiro toque só pede confirmação.
    let armed = |a: &Action| ui.and_then(|u| u.armed.as_ref()).is_some_and(|(x, t)| x == a && t.elapsed().as_secs() < 4);
    let confirm = |a: &Action, normal: String| if armed(a) { "toque de novo para confirmar".to_string() } else { normal };
    let mut v = Vec::new();
    match tab {
        Tab::Funcoes => {
            let liga = |b: bool| if b { "LIGADO" } else { "DESLIGADO" }.to_string();
            let mixando = crate::automix::estado(app).flatten().is_some();
            v.push(Row::Item { label: "Automix (toca e mistura sozinho)".into(), value: if mixando { "MIXANDO…".into() } else { liga(app.automix.is_some()) }, action: Action::Automix });
            let rec = app.rec.active.as_ref().map(|(_, t, _)| t.elapsed().as_secs());
            v.push(Row::Item {
                label: "Gravar o set".into(),
                value: rec.map_or("PARADO".into(), |s| format!("GRAVANDO {:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)),
                action: Action::Gravar,
            });
            v.push(Row::Item { label: "Filtro da lista de músicas".into(), value: crate::library::FILTROS[app.lib.filtro].into(), action: Action::FiltroBrowser });
            v.push(Row::Item { label: "Salvar a fila como playlist".into(), value: format!("{} MÚSICA(S) NA FILA", app.lib.prepare.len()), action: Action::SalvarFila });
            v.push(Row::Info("Automix: toca a fila (SHIFT + LOAD) ou a lista aberta a partir da música selecionada".into()));
            const NOMES: [&str; NUM_DECKS] = ["DECK 1", "DECK 2", "DECK 3", "DECK 4"];
            for &d in app.side.iter().filter(|d| **d < NUM_DECKS) {
                // Os itens são sempre os mesmos e na mesma ordem, com ou sem música: o BROWSE escolhe
                // pela posição, e uma música terminando de carregar não pode mudar o item debaixo dele.
                v.push(Row::Header(NOMES[d]));
                let t = app.decks[d].track.as_ref();
                let ds = &app.snap_tela.decks[d];
                let shift = ds.key_shift as i32;
                let tom = match t {
                    Some(t) => crate::key::from_raw(t.key).map_or("sem tom".to_string(), |k| crate::key::label(crate::key::transpor(k, shift))),
                    None => "sem música".to_string(),
                };
                let tom = if shift != 0 { format!("{tom} ({shift:+})") } else { tom };
                let casar = if shift != 0 { ("Voltar ao tom original", crate::app::Tom::Zero) } else { ("Casar o tom com o outro deck", crate::app::Tom::Casar) };
                v.push(Row::Item { label: casar.0.into(), value: tom.clone(), action: Action::TomDeck(d, casar.1) });
                v.push(Row::Item { label: "Tom +1".into(), value: tom.clone(), action: Action::TomDeck(d, crate::app::Tom::Mais) });
                v.push(Row::Item { label: "Tom -1".into(), value: tom, action: Action::TomDeck(d, crate::app::Tom::Menos) });
                let bpm = t.map_or(0.0, |t| t.bpm());
                for (i, (nome, r)) in GRADE_RAZOES.iter().enumerate() {
                    let value = if bpm > 0.0 { format!("{bpm:.2} → {:.2}", bpm * r) } else { "—".into() };
                    v.push(Row::Item { label: format!("BPM {nome}"), value, action: Action::Grade(d, i) });
                }
                v.push(Row::Item { label: "Batida aqui (acerta a grade na posição de agora)".into(), value: String::new(), action: Action::Grade(d, GRADE_RAZOES.len()) });
                v.push(Row::Item { label: "Reverse (toca de trás para frente)".into(), value: liga(ds.reverse), action: Action::Reverso(d) });
            }
            v.push(Row::Info("BPM errado (ex.: 86,66 num funk de 130): use ×1,5. Batida fora do lugar: pare em cima do bumbo e use \"Batida aqui\"".into()));
        }
        Tab::Audio => {
            v.push(Row::Info(format!("Saída: {}", app.audio_status)));
            v.push(Row::Info("Com a DDJ-REV1 ligada: master nas saídas 1-2, fone nas 3-4.".into()));
            v.push(Row::Info("Sem a REV1: o master vai para a placa de som do computador.".into()));
            v.push(Row::Item {
                label: "Ganho automático (nivela o volume das músicas)".into(),
                value: if s.auto_gain { "LIGADO" } else { "DESLIGADO" }.into(),
                action: Action::AutoGain,
            });
            v.push(Row::Info(format!("Leva cada música a {:.0} LUFS (de {:+.0} a {:+.0} dB); o TRIM ajusta por cima.", crate::loudness::TARGET_LUFS, crate::loudness::MIN_GAIN_DB, crate::loudness::MAX_GAIN_DB)));
            let b = AUDIO_BUFFERS[s.audio_buffer.min(AUDIO_BUFFERS.len() - 1)];
            v.push(Row::Item { label: "Folga do som (contra estalos)".into(), value: format!("{b} ({:.0} ms)", b as f64 / 48.0), action: Action::AudioBuffer });
            v.push(Row::Info("Estalos ou estática com o computador ocupado: aumente a folga (512 é o normal; 1024 para computador fraco)".into()));
            let saida = if s.saida_som.is_empty() { "AUTOMÁTICA (controladora ou placa)".to_string() } else { s.saida_som_nome.clone() };
            v.push(Row::Item { label: "Saída do som".into(), value: saida, action: Action::SaidaSom });
            v.push(Row::Info("Automática: a controladora (master 1-2, fone 3-4) ou, sem ela, a placa do computador.".into()));
            v.push(Row::Header("GRAVAR O SET (botão GRAVAR na barra de cima ou Ctrl+R)"));
            v.push(Row::Item { label: "Gravar em".into(), value: crate::record::TARGETS[s.rec_target].into(), action: Action::RecTarget });
            v.push(Row::Info("Arquivo WAV na pasta \"records\" (≈11 MB por minuto).".into()));
        }
        Tab::Mixer => {
            v.push(Row::Item {
                label: "Modo do EQ".into(),
                value: if s.eq_iso { "ISOLADOR (corta tudo)" } else { "EQ (até -26 dB)" }.into(),
                action: Action::EqMode,
            });
            v.push(Row::Info("Isolador: girar o grave todo para a esquerda some com o grave, como nos mixers de clube.".into()));
            v.push(Row::Item { label: "Curva do fader de canal".into(), value: FADER_CURVES[s.fader_curve].into(), action: Action::FaderCurve });
            v.push(Row::Item { label: "Curva do crossfader".into(), value: XF_CURVES[s.xf_curve].into(), action: Action::XfCurve });
            v.push(Row::Header("LADO DE CADA DECK NO CROSSFADER"));
            let decks = if s.four_decks { NUM_DECKS } else { 2 };
            for d in 0..decks {
                v.push(Row::Item { label: format!("Deck {}", d + 1), value: XF_SIDES[s.xf_side[d]].into(), action: Action::XfSide(d) });
            }
            v.push(Row::Info("THRU: o deck não passa pelo crossfader (só o fader de canal manda).".into()));
            v.push(Row::Header("FONE DE OUVIDO"));
            v.push(Row::Item {
                label: "Fone".into(),
                value: if s.hp_split { "MONO DIVIDIDO" } else { "ESTÉREO" }.into(),
                action: Action::HpSplit,
            });
            v.push(Row::Info("Mono dividido: o CUE no ouvido esquerdo e o master no direito.".into()));
        }
        Tab::Bluetooth => {
            v.push(Row::Item { label: "Caixa Bluetooth toca".into(), value: app.bt_mode.name().into(), action: Action::BtMode });
            v.push(Row::Item { label: "Atraso do master com fio".into(), value: format!("{:.0} ms", s.bt_delay_ms), action: Action::BtDelay });
            v.push(Row::Info(app.bt_status.clone()));
            v.push(Row::Header("PAREAR UMA CAIXA NOVA"));
            v.push(Row::Item { label: "Procurar caixas".into(), value: ui.map_or(String::new(), |u| u.bt_msg.clone()), action: Action::BtScan });
            for (mac, name) in ui.map(|u| u.bt_devices.as_slice()).unwrap_or(&[]) {
                v.push(Row::Item { label: format!("   {name}"), value: "parear e conectar".into(), action: Action::BtPair(mac.clone(), name.clone()) });
            }
        }
        Tab::Phone => {
            v.push(Row::Info("O DJ OS vira uma caixa de som: no celular, conecte no Bluetooth \"DJ OS\".".into()));
            v.push(Row::Item { label: "Receber som do celular em".into(), value: crate::phone::ROUTES[s.phone_in].into(), action: Action::PhoneIn });
            if crate::phone::route_target(s.phone_in).is_none() && s.phone_in != 0 {
                v.push(Row::Item { label: "Volume do celular no master".into(), value: format!("{:.0}%", AMBIENT_VOLS[s.ambient_vol] * 100.0), action: Action::AmbientVol });
                v.push(Row::Item { label: "Abaixar quando um deck toca".into(), value: if s.ambient_duck { "SIM" } else { "NÃO" }.into(), action: Action::AmbientDuck });
            }
            v.push(Row::Info(format!("Estado: {}", app.phone_status)));
        }
        Tab::Wifi => {
            v.push(Row::Item { label: "Procurar redes".into(), value: ui.map_or(String::new(), |u| u.wifi_msg.clone()), action: Action::WifiScan });
            for n in ui.map(|u| u.wifi.as_slice()).unwrap_or(&[]) {
                let value = format!("{}{}% {}", if n.active { "CONECTADA · " } else { "" }, n.signal, if n.secure { "🔒" } else { "" });
                v.push(Row::Item { label: format!("   {}", n.ssid), value, action: Action::WifiPick(n.ssid.clone(), n.secure) });
            }
        }
        Tab::Controller => {
            v.push(Row::Item { label: "Jog ao ligar".into(), value: if s.vinyl { "VINIL (scratch)" } else { "CDJ (bend)" }.into(), action: Action::Vinyl });
            v.push(Row::Item { label: "Faixa de pitch".into(), value: format!("±{:.0}%", PITCH_RANGES[s.pitch_range] * 100.0), action: Action::PitchRange });
            v.push(Row::Item { label: "Keylock ao carregar".into(), value: if s.keylock { "LIGADO" } else { "DESLIGADO" }.into(), action: Action::Keylock });
            v.push(Row::Item { label: "Slip".into(), value: if s.slip { "LIGADO" } else { "DESLIGADO" }.into(), action: Action::Slip });
            v.push(Row::Item { label: "Quantize".into(), value: if s.quantize { "LIGADO" } else { "DESLIGADO" }.into(), action: Action::Quantize });
            v.push(Row::Item { label: "Play na batida".into(), value: if s.play_batida { "LIGADO" } else { "DESLIGADO" }.into(), action: Action::PlayBatida });
            v.push(Row::Item { label: "Trava do LOAD (deck no ar)".into(), value: if s.trava_load { "LIGADA" } else { "DESLIGADA" }.into(), action: Action::TravaLoad });
            let fim = FIM_AVISOS[s.fim_aviso.min(FIM_AVISOS.len() - 1)];
            v.push(Row::Item { label: "Aviso de fim da música".into(), value: if fim == 0 { "DESLIGADO".into() } else { format!("{fim} s ANTES") }, action: Action::FimAviso });
            v.push(Row::Item { label: "Girar o BROWSE na tela principal".into(), value: BROWSE_GIRA[s.browse_gira].into(), action: Action::BrowseGira });
            v.push(Row::Info("Segurar o BROWSE e girar: zoom da waveform. No modo zoom, o browser abre apertando o BROWSE ou SHIFT + BROWSE".into()));
            v.push(Row::Info("Slip: scratch, backspin e loop por cima da música; ao soltar, ela volta onde estaria".into()));
            v.push(Row::Info("Quantize: cue, hot cue e loop caem na batida; pular para um hot cue tocando não perde a fase".into()));
            v.push(Row::Info("Play na batida: dar PLAY com o outro deck tocando já entra na batida dele (anda até meia batida)".into()));
            v.push(Row::Info("Trava do LOAD: com o deck tocando no ar, o LOAD não troca a música (pare o deck, ou feche o fader ou o crossfader dele)".into()));
            let left = crate::midi::log_left();
            let value = if left > 0 { format!("GRAVANDO ({}:{:02})", left / 60, left % 60) } else { "DESLIGADO".into() };
            v.push(Row::Item { label: "Registrar os botões (diagnóstico, 5 min)".into(), value, action: Action::MidiLog });
            v.push(Row::Info(format!("Controladora: {}", app.midi_status)));
            // MIDI learn: escolhe a função e o deck, aperta "ensinar" e depois o botão.
            v.push(Row::Header("ENSINAR BOTÕES (MIDI LEARN)"));
            match crate::midi::CONECTADA.lock().clone() {
                None => v.push(Row::Info("Ligue a controladora para ensinar botões.".into())),
                Some(nome) => {
                    use crate::midi::aprender::{quantos, ESPERANDO, FUNCOES};
                    let (fi, deck) = ui.map_or((0, 0), |u| (u.aprender_funcao.min(FUNCOES.len() - 1), u.aprender_deck));
                    let (texto, _, de_deck) = FUNCOES[fi];
                    v.push(Row::Item { label: "Função".into(), value: texto.into(), action: Action::AprenderFuncao });
                    if de_deck {
                        v.push(Row::Item { label: "Deck".into(), value: format!("{}", deck + 1), action: Action::AprenderDeck });
                    }
                    let esperando = ESPERANDO.lock().is_some();
                    v.push(Row::Item {
                        label: "Ensinar agora".into(),
                        value: if esperando { "APERTE O BOTÃO NA CONTROLADORA…".into() } else { String::new() },
                        action: Action::AprenderAgora,
                    });
                    let n = quantos(&nome);
                    if n > 0 {
                        v.push(Row::Item { label: format!("Apagar os {n} botões ensinados"), value: confirm(&Action::AprenderApagar, String::new()), action: Action::AprenderApagar });
                    }
                    v.push(Row::Info("Os botões ensinados valem por cima do mapeamento. Mapear por arquivo: docs/mapeamento.md".into()));
                }
            }
        }
        Tab::Screen => {
            v.push(Row::Item { label: "Decks embaixo da tela".into(), value: if s.four_decks { "OS 4" } else { "SÓ OS 2 DA FRENTE" }.into(), action: Action::FourDecks });
            v.push(Row::Item { label: "Skin (cores e layout)".into(), value: if s.skin.is_empty() { "DE FÁBRICA (PRETA)".into() } else { s.skin.to_uppercase() }, action: Action::Skin });
            v.push(Row::Info("Skins prontas e as suas em ~/DJOS/skins (como fazer: docs/skins.md)".into()));
            v.push(Row::Item { label: "Cores da waveform".into(), value: PALETTES[s.palette].into(), action: Action::Palette });
            v.push(Row::Item { label: "Tamanho da tela".into(), value: format!("{:.0}%", SCALES[s.scale] * 100.0), action: Action::Scale });
            let auto_on = crate::ui::weak_gpu(&app.gpu);
            let light_value = match s.light_mode {
                0 => format!("AUTOMÁTICO ({})", if auto_on { "ligado" } else { "desligado" }),
                m => LIGHT_MODES[m].to_string(),
            };
            v.push(Row::Item { label: "Modo leve (placa de vídeo antiga)".into(), value: light_value, action: Action::LightMode });
            v.push(Row::Info(format!("Placa de vídeo: {}", if app.gpu.is_empty() { "?" } else { &app.gpu })));
            v.push(Row::Header("BROWSER (SHIFT + BROWSE)"));
            let show = |b: bool| if b { "MOSTRAR" } else { "ESCONDER" }.to_string();
            v.push(Row::Item { label: "Browser em tela cheia".into(), value: if s.browser_novo { "NOVO (PRETO)" } else { "CLÁSSICO" }.into(), action: Action::BrowserNovo });
            v.push(Row::Info("Playlists salvas e as .m3u do pendrive (VirtualDJ, rekordbox) aparecem nas pastas. Filtro e salvar a fila: aba FUNÇÕES".into()));
            v.push(Row::Item { label: "Coluna do BPM".into(), value: show(s.browser_bpm), action: Action::BrowserBpm });
            v.push(Row::Item { label: "Coluna do tom".into(), value: show(s.browser_key), action: Action::BrowserKey });
            v.push(Row::Item { label: "Tamanho da letra".into(), value: BROWSER_FONTS[s.browser_font].into(), action: Action::BrowserFont });
        }
        Tab::System => {
            v.push(Row::Info(app.fw.info.clone()));
            // Firmware, reiniciar e desligar só no aparelho.
            let aparelho = crate::app::aparelho();
            if aparelho {
                v.push(Row::Item { label: "Atualizar firmware (pendrive)".into(), value: app.fw.msg.clone(), action: Action::FwSearch });
                for f in &app.fw.files {
                    let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let act = Action::FwInstall(f.clone());
                    v.push(Row::Item { label: format!("   {name}"), value: confirm(&act, "instalar".into()), action: act });
                }
            }
            let analyzing = crate::import::ANALYZE_ALL.load(std::sync::atomic::Ordering::Relaxed);
            let value = if analyzing { "ANALISANDO… (para enquanto um deck toca)" } else { "ANALISAR" };
            v.push(Row::Item { label: "Analisar todas as músicas (BPM, tom, volume)".into(), value: value.into(), action: Action::AnalyzeAll });
            if aparelho {
                v.push(Row::Item { label: "Reiniciar".into(), value: confirm(&Action::Restart, String::new()), action: Action::Restart });
                v.push(Row::Item { label: "Desligar".into(), value: confirm(&Action::PowerOff, String::new()), action: Action::PowerOff });
            }
        }
    }
    v
}

/// Índices das linhas que dá para selecionar.
pub fn selectable(rows: &[Row]) -> Vec<usize> {
    rows.iter().enumerate().filter(|(_, r)| matches!(r, Row::Item { .. })).map(|(i, _)| i).collect()
}

pub fn open(app: &mut AppState) {
    // Abre na aba usada por último, com o BROWSE nas abas.
    let tab = app.settings_tab;
    app.settings_ui = Some(SettingsUi { tab, on_tabs: true, ..Default::default() });
    app.big_browser = false;
}

/// Toque/clique numa aba: abre a aba e passa para os itens dela.
pub fn select_tab(app: &mut AppState, tab: usize) {
    if let Some(ui) = app.settings_ui.as_mut() {
        ui.tab = tab.min(Tab::all().len() - 1);
        ui.sel = 0;
        ui.on_tabs = false;
        app.settings_tab = ui.tab;
    }
}

/// Teclado: ← volta para as abas, → entra nos itens.
pub fn focus_tabs(app: &mut AppState, on: bool) {
    let has_items = !selectable(&rows(app)).is_empty();
    if let Some(ui) = app.settings_ui.as_mut() {
        ui.on_tabs = on || !has_items;
        if !ui.on_tabs {
            ui.sel = 0;
        }
    }
}

pub fn close(app: &mut AppState) {
    app.settings_ui = None;
}

pub fn toggle(app: &mut AppState) {
    if app.settings_ui.is_some() {
        close(app)
    } else {
        open(app)
    }
}

/// BROWSE girado: nas abas, troca de aba; nos itens, anda pelos itens (girar para cima no
/// primeiro item volta para as abas).
pub fn scroll(app: &mut AppState, delta: i32) {
    let n = selectable(&rows(app)).len();
    let Some(ui) = app.settings_ui.as_mut() else { return };
    if ui.on_tabs {
        ui.tab = (ui.tab as i64 + delta as i64).clamp(0, Tab::all().len() as i64 - 1) as usize;
        ui.sel = 0;
        app.settings_tab = ui.tab;
    } else if delta < 0 && ui.sel == 0 {
        ui.on_tabs = true;
    } else if n > 0 {
        ui.sel = (ui.sel as i64 + delta as i64).clamp(0, n as i64 - 1) as usize;
    }
}

/// BROWSE apertado (ou toque): aciona o item selecionado.
pub fn activate(shared: &SharedApp, cmd: &CmdTx) {
    // BROWSE apertado nas abas: entra na aba (se ela tiver itens).
    {
        let mut a = shared.lock();
        if a.settings_ui.as_ref().is_some_and(|u| u.on_tabs) {
            focus_tabs(&mut a, false);
            return;
        }
    }
    let action = {
        let a = shared.lock();
        let rows = rows(&a);
        let sel = a.settings_ui.as_ref().map_or(0, |u| u.sel);
        selectable(&rows).get(sel).and_then(|&i| match &rows[i] {
            Row::Item { action, .. } => Some(action.clone()),
            _ => None,
        })
    };
    let Some(action) = action else { return };
    // Funções do set (aba FUNÇÕES): agem na hora, sem mexer nas configurações salvas.
    match action {
        Action::Gravar => {
            let rate = shared.lock().snap_tela.sample_rate;
            crate::record::toggle(shared, cmd, rate);
            return;
        }
        Action::Automix => {
            let mut a = shared.lock();
            let msg = crate::automix::alternar(&mut a, cmd);
            a.toast(msg);
            return;
        }
        Action::TomDeck(d, qual) => {
            let mut a = shared.lock();
            let snap = a.snap_tela;
            crate::app::comando_tom(&mut a, &snap, cmd, d, qual);
            return;
        }
        Action::Reverso(d) => {
            let on = !shared.lock().snap_tela.decks[d].reverse;
            cmd.send(Cmd::Reverse { deck: d, on });
            return;
        }
        Action::Grade(d, i) => {
            let mut a = shared.lock();
            let Some(t) = a.decks.get(d).and_then(|x| x.track.clone()).filter(|t| t.bpm() > 0.0) else {
                a.toast(format!("Deck {}: sem música com batida para acertar", d + 1));
                return;
            };
            match GRADE_RAZOES.get(i) {
                Some((_, r)) => crate::grid::set_bpm_ratio(&t, *r),
                None => crate::grid::beat_here(&t, a.snap_tela.decks[d].pos),
            }
            crate::grid::mudou(&mut a, &t);
            a.toast(format!("Deck {}: grade em {:.2} BPM", d + 1, t.bpm()));
            return;
        }
        _ => {}
    }
    let mut a = shared.lock();
    // Ações perigosas: só no segundo toque (em até 4 s).
    if matches!(action, Action::FwInstall(_) | Action::Restart | Action::PowerOff | Action::AprenderApagar) {
        let Some(u) = a.settings_ui.as_mut() else { return };
        let again = u.armed.as_ref().is_some_and(|(x, t)| *x == action && t.elapsed().as_secs() < 4);
        if !again {
            u.armed = Some((action, std::time::Instant::now()));
            return;
        }
        u.armed = None;
        drop(a);
        match action {
            Action::FwInstall(f) => crate::firmware::install(shared, f),
            Action::AprenderApagar => {
                if let Some(nome) = crate::midi::CONECTADA.lock().clone() {
                    crate::midi::aprender::apagar(&nome);
                    shared.lock().toast("Botões ensinados apagados");
                }
            }
            Action::Restart => crate::firmware::power(shared, "reiniciar"),
            _ => crate::firmware::power(shared, "desligar"),
        }
        return;
    }
    let next = |i: usize, n: usize| (i + 1) % n;
    match action {
        Action::BtMode => {
            a.bt_mode = a.bt_mode.next();
            a.settings.bt_mode = a.bt_mode;
            a.bt_status = format!("BT {} · procurando…", a.bt_mode.name());
        }
        Action::BtDelay => {
            let i = BT_DELAYS.iter().position(|d| *d == a.settings.bt_delay_ms).unwrap_or(0);
            a.settings.bt_delay_ms = BT_DELAYS[next(i, BT_DELAYS.len())];
            a.bt_delay_ms = a.settings.bt_delay_ms;
        }
        Action::Vinyl => a.settings.vinyl = !a.settings.vinyl,
        Action::PitchRange => a.settings.pitch_range = next(a.settings.pitch_range, PITCH_RANGES.len()),
        Action::XfCurve => a.settings.xf_curve = next(a.settings.xf_curve, XF_CURVES.len()),
        Action::XfSide(d) => a.settings.xf_side[d] = next(a.settings.xf_side[d], XF_SIDES.len()),
        Action::EqMode => a.settings.eq_iso = !a.settings.eq_iso,
        Action::FaderCurve => a.settings.fader_curve = next(a.settings.fader_curve, FADER_CURVES.len()),
        Action::HpSplit => a.settings.hp_split = !a.settings.hp_split,
        Action::Keylock => a.settings.keylock = !a.settings.keylock,
        Action::Slip => a.settings.slip = !a.settings.slip,
        Action::Quantize => a.settings.quantize = !a.settings.quantize,
        Action::PlayBatida => a.settings.play_batida = !a.settings.play_batida,
        Action::TravaLoad => a.settings.trava_load = !a.settings.trava_load,
        Action::FimAviso => a.settings.fim_aviso = next(a.settings.fim_aviso, FIM_AVISOS.len()),
        Action::BrowserNovo => a.settings.browser_novo = !a.settings.browser_novo,
        Action::FiltroBrowser => {
            a.lib.proximo_filtro();
            return;
        }
        Action::SalvarFila => {
            crate::midi::mapa::salvar_fila(&mut a);
            return;
        }
        Action::BrowserBpm => a.settings.browser_bpm = !a.settings.browser_bpm,
        Action::BrowserKey => a.settings.browser_key = !a.settings.browser_key,
        Action::BrowserFont => a.settings.browser_font = next(a.settings.browser_font, BROWSER_FONTS.len()),
        Action::BrowseGira => a.settings.browse_gira = next(a.settings.browse_gira, BROWSE_GIRA.len()),
        Action::AudioBuffer => a.settings.audio_buffer = next(a.settings.audio_buffer, AUDIO_BUFFERS.len()),
        Action::Skin => {
            // Relê os arquivos (a pessoa pode ter editado) e passa para a próxima skin.
            crate::ui::skin::recarregar();
            let nomes: Vec<String> = std::iter::once(String::new()).chain(crate::ui::skin::todas().into_iter().map(|(n, _)| n)).collect();
            let i = nomes.iter().position(|n| *n == a.settings.skin).unwrap_or(0);
            a.settings.skin = nomes[next(i, nomes.len())].clone();
        }
        Action::SaidaSom => {
            // Automática → cada saída ligada → automática.
            let lista: Vec<(String, String)> = std::iter::once((String::new(), String::new())).chain(crate::audio_out::saidas()).collect();
            let i = lista.iter().position(|(n, _)| *n == a.settings.saida_som).unwrap_or(0);
            let (n, nome) = lista[next(i, lista.len())].clone();
            a.settings.saida_som = n;
            a.settings.saida_som_nome = nome;
        }
        Action::AprenderFuncao => {
            if let Some(u) = a.settings_ui.as_mut() {
                u.aprender_funcao = next(u.aprender_funcao, crate::midi::aprender::FUNCOES.len());
            }
            return;
        }
        Action::AprenderDeck => {
            if let Some(u) = a.settings_ui.as_mut() {
                u.aprender_deck = next(u.aprender_deck, crate::shared::NUM_DECKS);
            }
            return;
        }
        Action::AprenderAgora => {
            use crate::midi::aprender::{Pedido, ESPERANDO, FUNCOES};
            let (Some(nome), Some(u)) = (crate::midi::CONECTADA.lock().clone(), a.settings_ui.as_ref()) else { return };
            let (_, acao, de_deck) = FUNCOES[u.aprender_funcao.min(FUNCOES.len() - 1)];
            let acao = if de_deck { format!("{acao} deck={}", u.aprender_deck + 1) } else { acao.to_string() };
            *ESPERANDO.lock() = Some(Pedido { acao, controladora: nome });
            a.toast("Aperte o botão (ou mexa no knob) na controladora");
            return;
        }
        Action::Palette => a.settings.palette = next(a.settings.palette, PALETTES.len()),
        Action::Scale => a.settings.scale = next(a.settings.scale, SCALES.len()),
        Action::AmbientVol => a.settings.ambient_vol = next(a.settings.ambient_vol, AMBIENT_VOLS.len()),
        Action::AmbientDuck => a.settings.ambient_duck = !a.settings.ambient_duck,
        Action::LightMode => a.settings.light_mode = next(a.settings.light_mode, LIGHT_MODES.len()),
        Action::FourDecks => a.settings.four_decks = !a.settings.four_decks,
        Action::MidiLog => {
            let on = crate::midi::log_left() == 0;
            crate::midi::log_for(if on { 300 } else { 0 });
            a.toast(if on { "Registrando os botões por 5 min: aperte o que não funciona" } else { "Registro dos botões desligado" }.to_string());
            return;
        }
        Action::AnalyzeAll => {
            crate::import::ANALYZE_ALL.store(true, std::sync::atomic::Ordering::Relaxed);
            a.toast("Analisando todas as músicas: pode demorar; para sozinho enquanto um deck toca".to_string());
            return;
        }
        Action::AutoGain => a.settings.auto_gain = !a.settings.auto_gain,
        Action::RecTarget => a.settings.rec_target = next(a.settings.rec_target, crate::record::TARGETS.len()),
        Action::PhoneIn => {
            a.settings.phone_in = next(a.settings.phone_in, crate::phone::ROUTES.len());
            a.phone_status = if a.settings.phone_in == 0 { "desligando…".into() } else { "ligando o Bluetooth…".into() };
        }
        Action::WifiScan => {
            drop(a);
            wifi_scan(shared);
            return;
        }
        Action::WifiPick(ssid, secure) => {
            if secure {
                crate::search::open_password(&mut a, &ssid);
            } else {
                wifi_connect(&mut a, &ssid, "");
            }
            return;
        }
        Action::FwSearch => {
            drop(a);
            crate::firmware::search(shared);
            return;
        }
        Action::FwInstall(_) | Action::Restart | Action::PowerOff | Action::AprenderApagar => return,
        Action::Gravar | Action::Automix | Action::TomDeck(..) | Action::Grade(..) | Action::Reverso(_) => return,
        Action::BtScan => {
            drop(a);
            bt_scan(shared);
            return;
        }
        Action::BtPair(mac, name) => {
            drop(a);
            bt_pair(shared, mac, name);
            return;
        }
    }
    a.settings.save();
    a.settings.apply(cmd);
}

// ------------------------------------------------------------------ Wi-Fi

/// Lista as redes (só leitura: `nmcli dev wifi list`).
fn wifi_scan(shared: &SharedApp) {
    if let Some(u) = shared.lock().settings_ui.as_mut() {
        u.wifi_msg = "procurando…".into();
    }
    let shared = shared.clone();
    std::thread::spawn(move || {
        let out = std::process::Command::new("nmcli").args(["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "dev", "wifi", "list"]).output();
        let mut nets: Vec<WifiNet> = match &out {
            Ok(o) => String::from_utf8_lossy(&o.stdout).lines().filter_map(parse_nmcli_line).collect(),
            Err(_) => Vec::new(),
        };
        nets.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.cmp(&a.signal)));
        nets.dedup_by(|a, b| a.ssid == b.ssid);
        let mut a = shared.lock();
        if let Some(u) = a.settings_ui.as_mut() {
            u.wifi_msg = match &out {
                Err(_) => "sem Wi-Fi neste aparelho".into(),
                Ok(_) if nets.is_empty() => "nenhuma rede encontrada".into(),
                Ok(_) => format!("{} redes", nets.len()),
            };
            u.wifi = nets;
        }
    });
}

/// Uma linha do `nmcli -t`: "*:MinhaRede:72:WPA2" (":" dentro do nome vem como "\:").
pub fn parse_nmcli_line(line: &str) -> Option<WifiNet> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => cur.extend(chars.next()),
            ':' => fields.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    fields.push(cur);
    if fields.len() < 4 || fields[1].is_empty() {
        return None;
    }
    Some(WifiNet {
        active: fields[0] == "*",
        ssid: fields[1].clone(),
        signal: fields[2].parse().unwrap_or(0),
        secure: !fields[3].is_empty() && fields[3] != "--",
    })
}

/// Conecta na rede (`nmcli dev wifi connect`), em segundo plano.
pub fn wifi_connect(app: &mut AppState, ssid: &str, password: &str) {
    app.toast(format!("Conectando em {ssid}…"));
    let (ssid, password) = (ssid.to_string(), password.to_string());
    let mut args = vec!["dev".to_string(), "wifi".into(), "connect".into(), ssid.clone()];
    if !password.is_empty() {
        args.push("password".into());
        args.push(password);
    }
    std::thread::spawn(move || {
        let ok = std::process::Command::new("nmcli").args(&args).output().is_ok_and(|o| o.status.success());
        eprintln!("[djos] Wi-Fi {ssid}: {}", if ok { "conectado" } else { "falhou" });
    });
}

// -------------------------------------------------------------- Bluetooth

/// Procura caixas por 8 s (`bluetoothctl --timeout 8 scan on`) e lista os aparelhos.
fn bt_scan(shared: &SharedApp) {
    if let Some(u) = shared.lock().settings_ui.as_mut() {
        u.bt_msg = "procurando (8 s)…".into();
    }
    let shared = shared.clone();
    std::thread::spawn(move || {
        if let Err(e) = crate::bluetooth::hold_on(30) {
            if let Some(u) = shared.lock().settings_ui.as_mut() {
                u.bt_msg = format!("Bluetooth: {e}");
            }
            return;
        }
        let _ = std::process::Command::new("bluetoothctl").args(["--timeout", "8", "scan", "on"]).output();
        let out = std::process::Command::new("bluetoothctl").arg("devices").output();
        let devices: Vec<(String, String)> = out
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|l| {
                        let rest = l.strip_prefix("Device ")?;
                        let (mac, name) = rest.split_once(' ')?;
                        Some((mac.to_string(), name.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut a = shared.lock();
        if let Some(u) = a.settings_ui.as_mut() {
            u.bt_msg = if devices.is_empty() { "nada encontrado".into() } else { format!("{} aparelhos", devices.len()) };
            u.bt_devices = devices;
        }
    });
}

/// Pareia, confia e conecta (`bluetoothctl pair/trust/connect`), em segundo plano.
fn bt_pair(shared: &SharedApp, mac: String, name: String) {
    shared.lock().toast(format!("Pareando {name}…"));
    let shared = shared.clone();
    std::thread::spawn(move || {
        if let Err(e) = crate::bluetooth::hold_on(60) {
            shared.lock().toast(format!("Bluetooth: {e}"));
            return;
        }
        let run = |cmd: &str| {
            std::process::Command::new("bluetoothctl").args(["--timeout", "15", cmd, &mac]).output().is_ok_and(|o| o.status.success())
        };
        let _ = run("pair"); // já pareada também serve
        let ok = run("trust") && run("connect");
        shared.lock().toast(if ok { format!("{name} conectada") } else { format!("Não conectou em {name}") });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuracoes_gravam_e_leem_igual() {
        let s = Settings {
            vinyl: false,
            pitch_range: 2,
            xf_curve: 1,
            keylock: true,
            palette: 1,
            scale: 2,
            bt_mode: BtMode::Retorno,
            bt_delay_ms: 150.0,
            ambient_vol: 0,
            ambient_duck: false,
            light_mode: 1,
            phone_in: 3,
            auto_gain: false,
            rec_target: 2,
            four_decks: true,
            eq_iso: false,
            fader_curve: 2,
            xf_side: [1, 2, 0, 1],
            hp_split: true,
            slip: true,
            browser_novo: false,
            browser_bpm: false,
            browser_key: false,
            browser_font: 2,
            browse_gira: 1,
            audio_buffer: 2,
            saida_som: "alsa_output.usb-Pioneer_DDJ-400".into(),
            saida_som_nome: "DDJ-400".into(),
            skin: "Clara".into(),
            quantize: true,
            trava_load: false,
            fim_aviso: 3,
            play_batida: true,
        };
        assert_eq!(Settings::parse(&s.to_text()), s);
        assert_eq!(Settings::parse("lixo\nfaixa_pitch=99\n"), Settings::default(), "valores inválidos viram o padrão");
    }

    #[test]
    fn le_a_lista_do_nmcli() {
        let n = parse_nmcli_line(r"*:Casa\:5G:81:WPA2").unwrap();
        assert!(n.active && n.secure);
        assert_eq!((n.ssid.as_str(), n.signal), ("Casa:5G", 81));
        let n = parse_nmcli_line(":Aberta:40:").unwrap();
        assert!(!n.secure && !n.active);
        assert!(parse_nmcli_line("::10:WPA2").is_none(), "rede sem nome some");
    }

    #[test]
    fn navegar_e_acionar_itens() {
        let (tx, mut rx) = rtrb::RingBuffer::new(256);
        let cmd = CmdTx::new(tx);
        let app = std::sync::Arc::new(parking_lot::Mutex::new(AppState::new()));
        open(&mut app.lock());
        // Aba MIXER: acha o item "Curva do crossfader" e aciona.
        select_tab(&mut app.lock(), Tab::all().iter().position(|t| *t == Tab::Mixer).unwrap());
        let idx = {
            let a = app.lock();
            let r = rows(&a);
            selectable(&r).iter().position(|&i| matches!(&r[i], Row::Item { action: Action::XfCurve, .. })).unwrap()
        };
        scroll(&mut app.lock(), idx as i32);
        let before = app.lock().settings.xf_curve;
        activate(&app, &cmd);
        assert_eq!(app.lock().settings.xf_curve, (before + 1) % XF_CURVES.len());
        let mut cmds = Vec::new();
        while let Ok(c) = rx.pop() {
            cmds.push(format!("{c:?}"));
        }
        assert!(cmds.iter().any(|c| c.starts_with("XfCurve(")));
        assert!(cmds.iter().filter(|c| c.starts_with("Vinyl")).count() == NUM_DECKS);
    }

    #[test]
    fn funcoes_pela_controladora() {
        let (tx, mut rx) = rtrb::RingBuffer::new(256);
        let cmd = CmdTx::new(tx);
        let app = std::sync::Arc::new(parking_lot::Mutex::new(AppState::new()));
        {
            let mut a = app.lock();
            a.side = [0, 1];
            a.decks[0].track = Some(std::sync::Arc::new(crate::track::synthetic(10.0, 48000.0, 86.66, 0.1)));
            open(&mut a);
            assert_eq!(current_tab(&a), Tab::Funcoes, "a primeira aba é a das funções do set");
            focus_tabs(&mut a, false);
        }
        // Acha um item pela ação, anda até ele com o BROWSE e aperta.
        let aciona = |quer: Action| {
            let idx = {
                let a = app.lock();
                let r = rows(&a);
                selectable(&r).iter().position(|&i| matches!(&r[i], Row::Item { action, .. } if *action == quer)).unwrap_or_else(|| panic!("sem o item {quer:?}"))
            };
            {
                let mut a = app.lock();
                a.settings_ui.as_mut().unwrap().sel = 0;
                scroll(&mut a, idx as i32);
            }
            activate(&app, &cmd);
        };
        // Funk analisado em 86,66: × 1,5 = 130, pelo BROWSE.
        aciona(Action::Grade(0, 0));
        assert_eq!(app.lock().decks[0].track.as_ref().unwrap().bpm(), 130.0);
        aciona(Action::TomDeck(0, crate::app::Tom::Mais));
        aciona(Action::Reverso(0));
        aciona(Action::Automix);
        assert!(app.lock().automix.is_some());
        let mut cmds = Vec::new();
        while let Ok(c) = rx.pop() {
            cmds.push(format!("{c:?}"));
        }
        assert!(cmds.contains(&"KeyShift { deck: 0, semis: 1.0 }".to_string()), "{cmds:?}");
        assert!(cmds.contains(&"Reverse { deck: 0, on: true }".to_string()));
        assert!(!cmds.iter().any(|c| c.starts_with("Vinyl")), "as funções não reaplicam as configurações");
        // Os itens ficam no mesmo lugar com ou sem música (uma carga no meio não troca o item
        // debaixo do BROWSE).
        let antes = selectable(&rows(&app.lock())).len();
        app.lock().decks[0].track = None;
        assert_eq!(selectable(&rows(&app.lock())).len(), antes);
    }

    #[test]
    fn no_pc_nao_tem_as_abas_do_sistema() {
        if crate::app::aparelho() {
            return;
        }
        assert!(!Tab::all().iter().any(|t| matches!(t, Tab::Wifi | Tab::Bluetooth | Tab::Phone)), "Wi-Fi e Bluetooth são do computador");
        let mut app = AppState::new();
        open(&mut app);
        select_tab(&mut app, Tab::all().iter().position(|t| *t == Tab::System).unwrap());
        assert!(!rows(&app).iter().any(|r| matches!(r, Row::Item { action: Action::PowerOff | Action::Restart | Action::FwSearch, .. })), "nem desligar nem firmware");
    }

    #[test]
    fn browse_anda_pelas_abas_entra_e_volta() {
        let mut app = AppState::new();
        open(&mut app);
        assert!(app.settings_ui.as_ref().unwrap().on_tabs, "abre com o BROWSE nas abas");
        scroll(&mut app, 3);
        let quarta = Tab::all()[3]; // CELULAR no aparelho; TELA no PC (sem as abas do sistema)
        assert_eq!(current_tab(&app), quarta);
        // Apertar o BROWSE entra na aba.
        focus_tabs(&mut app, false);
        let ui = app.settings_ui.as_ref().unwrap();
        assert!(!ui.on_tabs && ui.sel == 0);
        // Girar para cima no primeiro item volta para as abas, na mesma aba.
        scroll(&mut app, -1);
        assert!(app.settings_ui.as_ref().unwrap().on_tabs);
        assert_eq!(current_tab(&app), quarta);
        // Fecha e abre de novo: volta na última aba.
        close(&mut app);
        open(&mut app);
        assert_eq!(current_tab(&app), quarta);
        // Aba ÁUDIO: apertar entra no item do ganho automático.
        select_tab(&mut app, Tab::all().iter().position(|t| *t == Tab::Audio).unwrap());
        let r = rows(&app);
        assert!(matches!(&r[selectable(&r)[0]], Row::Item { action: Action::AutoGain, .. }));
    }
}
