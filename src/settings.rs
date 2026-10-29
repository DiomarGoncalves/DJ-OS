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
pub const PITCH_RANGES: [f64; 3] = [0.08, 0.16, 0.50];
pub const XF_CURVES: [&str; 3] = ["SUAVE", "CORTE (SCRATCH)", "POTÊNCIA CONSTANTE"];
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
             volume_ambiente={}\nabaixar_ambiente={}\nmodo_leve={}\ncelular_bt={}\nganho_automatico={}\ngravar_em={}\nquatro_decks={}\n",
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
            yn(self.four_decks)
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
        }
        cmd.send(Cmd::XfCurve(self.xf_curve as u8));
        cmd.send(Cmd::AmbientVolume(AMBIENT_VOLS[self.ambient_vol]));
        cmd.send(Cmd::AmbientDuck(self.ambient_duck));
        cmd.send(Cmd::LineInRoute(crate::phone::route_target(self.phone_in)));
        cmd.send(Cmd::AutoGain(self.auto_gain));
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
    Keylock,
    Palette,
    Scale,
    AmbientVol,
    AmbientDuck,
    LightMode,
    FourDecks,
}

pub enum Row {
    Header(&'static str),
    Item { label: String, value: String, action: Action },
    Info(String),
}

/// Estado da tela de configurações aberta.
#[derive(Default)]
pub struct SettingsUi {
    /// Aba aberta (índice em Tab::ALL).
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
}

/// Abas da tela de configurações (estilo XDJ: abas à esquerda, itens à direita).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Audio,
    Bluetooth,
    Phone,
    Wifi,
    Controller,
    Screen,
    System,
}

impl Tab {
    pub const ALL: [Tab; 7] = [Tab::Audio, Tab::Bluetooth, Tab::Phone, Tab::Wifi, Tab::Controller, Tab::Screen, Tab::System];

    pub fn name(self) -> &'static str {
        match self {
            Tab::Audio => "ÁUDIO",
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
        Tab::Audio => if app.audio_status.contains("DDJ-REV1") { "DDJ-REV1".into() } else if app.audio_status.starts_with(crate::pw_out::NO_OUTPUT) { "sem saída".into() } else { "sem a REV1".into() },
        Tab::Bluetooth => app.bt_mode.name().to_lowercase(),
        Tab::Phone => crate::phone::ROUTES[s.phone_in].to_lowercase(),
        Tab::Wifi => app.settings_ui.as_ref().and_then(|u| u.wifi.iter().find(|n| n.active)).map_or("—".into(), |n| n.ssid.clone()),
        Tab::Controller => format!("{} · ±{:.0}%", if s.vinyl { "vinil" } else { "cdj" }, PITCH_RANGES[s.pitch_range] * 100.0),
        Tab::Screen => PALETTES[s.palette].to_lowercase(),
        Tab::System => app.fw.info.split(" · ").next().unwrap_or("").replace("Versão ", "v"),
    }
}

/// Aba aberta agora.
pub fn current_tab(app: &AppState) -> Tab {
    Tab::ALL[app.settings_ui.as_ref().map_or(0, |u| u.tab).min(Tab::ALL.len() - 1)]
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
            v.push(Row::Header("GRAVAR O SET (botão GRAVAR na barra de cima ou Ctrl+R)"));
            v.push(Row::Item { label: "Gravar em".into(), value: crate::record::TARGETS[s.rec_target].into(), action: Action::RecTarget });
            v.push(Row::Info("Arquivo WAV na pasta \"records\" (≈11 MB por minuto).".into()));
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
            v.push(Row::Item { label: "Curva do crossfader".into(), value: XF_CURVES[s.xf_curve].into(), action: Action::XfCurve });
            v.push(Row::Item { label: "Keylock ao carregar".into(), value: if s.keylock { "LIGADO" } else { "DESLIGADO" }.into(), action: Action::Keylock });
            v.push(Row::Info(format!("Controladora: {}", app.midi_status)));
        }
        Tab::Screen => {
            v.push(Row::Item { label: "Decks embaixo da tela".into(), value: if s.four_decks { "OS 4" } else { "SÓ OS 2 DA FRENTE" }.into(), action: Action::FourDecks });
            v.push(Row::Item { label: "Cores da waveform".into(), value: PALETTES[s.palette].into(), action: Action::Palette });
            v.push(Row::Item { label: "Tamanho da tela".into(), value: format!("{:.0}%", SCALES[s.scale] * 100.0), action: Action::Scale });
            let auto_on = crate::ui::weak_gpu(&app.gpu);
            let light_value = match s.light_mode {
                0 => format!("AUTOMÁTICO ({})", if auto_on { "ligado" } else { "desligado" }),
                m => LIGHT_MODES[m].to_string(),
            };
            v.push(Row::Item { label: "Modo leve (placa de vídeo antiga)".into(), value: light_value, action: Action::LightMode });
            v.push(Row::Info(format!("Placa de vídeo: {}", if app.gpu.is_empty() { "?" } else { &app.gpu })));
        }
        Tab::System => {
            v.push(Row::Info(app.fw.info.clone()));
            v.push(Row::Item { label: "Atualizar firmware (pendrive)".into(), value: app.fw.msg.clone(), action: Action::FwSearch });
            for f in &app.fw.files {
                let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let act = Action::FwInstall(f.clone());
                v.push(Row::Item { label: format!("   {name}"), value: confirm(&act, "instalar".into()), action: act });
            }
            v.push(Row::Item { label: "Reiniciar".into(), value: confirm(&Action::Restart, String::new()), action: Action::Restart });
            v.push(Row::Item { label: "Desligar".into(), value: confirm(&Action::PowerOff, String::new()), action: Action::PowerOff });
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
        ui.tab = tab.min(Tab::ALL.len() - 1);
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
        ui.tab = (ui.tab as i64 + delta as i64).clamp(0, Tab::ALL.len() as i64 - 1) as usize;
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
    let mut a = shared.lock();
    // Ações perigosas: só no segundo toque (em até 4 s).
    if matches!(action, Action::FwInstall(_) | Action::Restart | Action::PowerOff) {
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
        Action::Keylock => a.settings.keylock = !a.settings.keylock,
        Action::Palette => a.settings.palette = next(a.settings.palette, PALETTES.len()),
        Action::Scale => a.settings.scale = next(a.settings.scale, SCALES.len()),
        Action::AmbientVol => a.settings.ambient_vol = next(a.settings.ambient_vol, AMBIENT_VOLS.len()),
        Action::AmbientDuck => a.settings.ambient_duck = !a.settings.ambient_duck,
        Action::LightMode => a.settings.light_mode = next(a.settings.light_mode, LIGHT_MODES.len()),
        Action::FourDecks => a.settings.four_decks = !a.settings.four_decks,
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
        Action::FwInstall(_) | Action::Restart | Action::PowerOff => return,
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
        // Aba CONTROLADORA: acha o item "Curva do crossfader" e aciona.
        select_tab(&mut app.lock(), Tab::ALL.iter().position(|t| *t == Tab::Controller).unwrap());
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
    fn browse_anda_pelas_abas_entra_e_volta() {
        let mut app = AppState::new();
        open(&mut app);
        assert!(app.settings_ui.as_ref().unwrap().on_tabs, "abre com o BROWSE nas abas");
        scroll(&mut app, 2);
        assert_eq!(current_tab(&app), Tab::Phone);
        // Apertar o BROWSE entra na aba.
        focus_tabs(&mut app, false);
        let ui = app.settings_ui.as_ref().unwrap();
        assert!(!ui.on_tabs && ui.sel == 0);
        // Girar para cima no primeiro item volta para as abas, na mesma aba.
        scroll(&mut app, -1);
        assert!(app.settings_ui.as_ref().unwrap().on_tabs);
        assert_eq!(current_tab(&app), Tab::Phone);
        // Fecha e abre de novo: volta na última aba.
        close(&mut app);
        open(&mut app);
        assert_eq!(current_tab(&app), Tab::Phone);
        // Aba ÁUDIO: apertar entra no item do ganho automático.
        select_tab(&mut app, 0);
        let r = rows(&app);
        assert!(matches!(&r[selectable(&r)[0]], Row::Item { action: Action::AutoGain, .. }));
    }
}
