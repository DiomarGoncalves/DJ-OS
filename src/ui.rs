//! Tela do sistema no estilo XDJ: waveforms empilhadas, informações dos decks e biblioteca.
//! Pensada para 1366x768 e operada pela controladora; o teclado serve para testes.

mod principal;

use crate::app::{Loader, SharedApp};
use crate::engine::fx::{beats_label, step_beats, FxKind};
use crate::library::Focus;
use crate::search;
use crate::shared::{Cmd, CmdTx, DeckSnap, SharedSnap, Snapshot};
use crate::track::{Track, BINS_PER_SEC};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Key, Mesh, Painter, Rect, Shape, Stroke};
use std::sync::Arc;
use std::time::Duration;

const BG: Color32 = Color32::from_rgb(9, 11, 14);
const PANEL: Color32 = Color32::from_rgb(20, 23, 28);
const PANEL2: Color32 = Color32::from_rgb(28, 32, 39);
const LINE: Color32 = Color32::from_rgb(44, 50, 60);
const TEXT: Color32 = Color32::from_rgb(232, 235, 240);
const DIM: Color32 = Color32::from_rgb(130, 140, 155);
const ACCENT: Color32 = Color32::from_rgb(198, 240, 0);
const RED: Color32 = Color32::from_rgb(255, 70, 70);
const ORANGE: Color32 = Color32::from_rgb(255, 150, 40);
/// Modo leve (placa de vídeo antiga): menos desenho por quadro e 30 quadros por segundo.
static LIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn light() -> bool {
    LIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Placas fracas ou sem aceleração: liga o modo leve sozinho (no "automático").
pub fn weak_gpu(renderer: &str) -> bool {
    let r = renderer.to_lowercase();
    ["hd graphics 2000", "hd graphics 3000", "sandybridge", "snb gt", "ironlake", "llvmpipe", "softpipe", "software"]
        .iter()
        .any(|k| r.contains(k))
}

/// Paleta da waveform escolhida nas configurações (graves, médios, agudos).
pub(crate) static PALETTE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn wave_colors() -> [Color32; 3] {
    match PALETTE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => [Color32::from_rgb(230, 45, 45), Color32::from_rgb(45, 210, 90), Color32::from_rgb(70, 150, 255)],
        2 => [Color32::from_rgb(25, 75, 210), Color32::from_rgb(70, 155, 255), Color32::from_rgb(195, 230, 255)],
        _ => [Color32::from_rgb(40, 110, 255), Color32::from_rgb(255, 165, 50), Color32::from_rgb(245, 245, 255)],
    }
}
const HOTCUE_COLORS: [Color32; 8] = [
    Color32::from_rgb(255, 60, 60),
    Color32::from_rgb(255, 150, 30),
    Color32::from_rgb(255, 230, 40),
    Color32::from_rgb(60, 220, 90),
    Color32::from_rgb(40, 200, 230),
    Color32::from_rgb(60, 110, 255),
    Color32::from_rgb(180, 80, 255),
    Color32::from_rgb(255, 80, 200),
];

fn deck_color(d: usize) -> Color32 {
    if d % 2 == 0 {
        Color32::from_rgb(50, 195, 255)
    } else {
        Color32::from_rgb(255, 120, 50)
    }
}

/// Zoom da waveform: segundos visíveis (5 níveis, como na XDJ). Tocar na waveform troca.
pub const ZOOMS: [f64; 5] = [2.0, 4.0, 6.0, 10.0, 16.0];

struct Overview {
    gen: u64,
    width: usize,
    cols: Vec<[u8; 3]>,
}

pub struct DjOsApp {
    app: SharedApp,
    cmd: CmdTx,
    snap: SharedSnap,
    loader: Loader,
    trash: rtrb::Consumer<Arc<Track>>,
    overview: [Option<Overview>; 4],
    xf: f32,
    frame: u64,
    /// Contagem ao vivo da pesquisa: (texto, quantas músicas).
    search_count: (String, usize),
    /// Pads da tela apertados (por deck), para mandar o "soltar".
    pad_touch: [[bool; 8]; 4],
    last_frame: std::time::Instant,
    /// Estado dos decks guardado por último (para retomar depois de um travamento).
    resume_saved: String,
    resume_at: std::time::Instant,
    /// Último quadro contado no histórico (tempo no ar de cada música).
    history_at: std::time::Instant,
    /// Até quando mostrar "LIM" (o limitador do master trabalhou há pouco).
    lim_until: std::time::Instant,
    /// Roda do mouse nas configurações: pixels acumulados até virar um passo.
    wheel: std::cell::Cell<f32>,
    /// Memória por música: último estado de cue/hotcues/loop visto em cada deck.
    persist: [Persist; 4],
}

/// Cue, hotcues e loop de um deck, para saber quando mudaram e precisam ser salvos.
type CueState = (f64, [Option<f64>; 8], Option<(f64, f64, f64)>);

#[derive(Default)]
struct Persist {
    gen: u64,
    state: Option<CueState>,
    changed_at: Option<std::time::Instant>,
}

fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0);
    format!("{:02}:{:02}.{}", (s / 60.0) as u32, (s % 60.0) as u32, ((s * 10.0) % 10.0) as u32)
}

fn text(p: &Painter, pos: egui::Pos2, align: Align2, s: impl ToString, size: f32, color: Color32) -> Rect {
    p.text(pos, align, s, FontId::proportional(size), color)
}

fn mono(p: &Painter, pos: egui::Pos2, align: Align2, s: impl ToString, size: f32, color: Color32) -> Rect {
    p.text(pos, align, s, FontId::monospace(size), color)
}

/// Etiqueta do tom (Camelot, ex.: "8A") na cor da roda, encostada à direita em `right_center`.
fn key_badge(p: &Painter, right_center: egui::Pos2, key: u8, size: f32) -> Rect {
    let [r, g, b] = crate::key::color(key);
    let col = Color32::from_rgb(r, g, b);
    let w = size * 2.6;
    let rect = Rect::from_min_size(pos2(right_center.x - w, right_center.y - size * 0.75), vec2(w, size * 1.5));
    p.rect_filled(rect, 4.0, col.gamma_multiply(0.22));
    p.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, col));
    mono(p, rect.center(), Align2::CENTER_CENTER, crate::key::label(key), size, col);
    rect
}

fn badge(p: &Painter, x: f32, cy: f32, label: &str, on: bool, color: Color32) -> f32 {
    let font = FontId::proportional(12.0);
    let g = p.layout_no_wrap(label.to_string(), font, if on { BG } else { DIM });
    let r = Rect::from_min_size(pos2(x, cy - 10.0), vec2(g.size().x + 14.0, 20.0));
    if on {
        p.rect_filled(r, 4.0, color);
    } else {
        p.rect_stroke(r, 4.0, Stroke::new(1.0_f32, LINE));
    }
    p.galley(pos2(r.left() + 7.0, cy - g.size().y / 2.0), g, TEXT);
    r.right() + 6.0
}

impl DjOsApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        app: SharedApp,
        cmd: CmdTx,
        snap: SharedSnap,
        loader: Loader,
        trash: rtrb::Consumer<Arc<Track>>,
    ) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        principal::install_fonts(&cc.egui_ctx);
        if let Some(gl) = cc.gl.as_ref() {
            use eframe::glow::HasContext;
            let renderer = unsafe { gl.get_parameter_string(eframe::glow::RENDERER) };
            eprintln!("[djos] placa de vídeo: {renderer}{}", if weak_gpu(&renderer) { " (modo leve automático)" } else { "" });
            app.lock().gpu = renderer;
        }
        Self { app, cmd, snap, loader, trash, overview: Default::default(), xf: 0.5, frame: 0, search_count: (String::new(), 0), pad_touch: [[false; 8]; 4], last_frame: std::time::Instant::now(), lim_until: std::time::Instant::now(), history_at: std::time::Instant::now(), resume_saved: String::new(), resume_at: std::time::Instant::now(), wheel: Default::default(), persist: Default::default() }
    }

    /// Atalhos para testar sem a controladora.
    fn keyboard(&mut self, ctx: &egui::Context, snap: &Snapshot) {
        // Ctrl+R: grava / para de gravar o set.
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::R)) {
            crate::record::toggle(&self.app, &self.cmd, snap.sample_rate);
        }
        let (side_l, side_r) = {
            let a = self.app.lock();
            (a.side[0], a.side[1])
        };
        // Pesquisa aberta: o teclado do computador digita direto e as setas andam pelas teclas.
        {
            let mut a = self.app.lock();
            if a.search.is_some() {
                let pass = a.search.as_ref().is_some_and(|u| u.purpose != search::Purpose::Search);
                ctx.input(|i| {
                    for e in &i.events {
                        if let egui::Event::Text(t) = e {
                            for c in t.chars().filter(|c| c.is_alphanumeric() || *c == ' ') {
                                let k = if c == ' ' {
                                    "ESPAÇO".to_string()
                                } else if pass {
                                    c.to_string()
                                } else {
                                    c.to_uppercase().to_string()
                                };
                                if let Some(ui) = a.search.as_mut() {
                                    if ui.query.chars().count() < 40 {
                                        if k == "ESPAÇO" {
                                            search::press_key(&mut a, "ESPAÇO");
                                        } else {
                                            ui.query.push_str(&k);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let moves = [(Key::ArrowUp, -1, 0), (Key::ArrowDown, 1, 0), (Key::ArrowLeft, 0, -1), (Key::ArrowRight, 0, 1)];
                    for (k, dr, dc) in moves {
                        if i.key_pressed(k) {
                            if let Some(ui) = a.search.as_mut() {
                                ui.move_by(dr, dc);
                            }
                        }
                    }
                    if i.key_pressed(Key::Backspace) {
                        search::press_key(&mut a, "APAGAR");
                    }
                    if i.key_pressed(Key::Enter) {
                        search::submit(&mut a);
                    }
                    if i.key_pressed(Key::Escape) {
                        search::close(&mut a);
                    }
                });
                return;
            }
        }
        // F10 abre/fecha as configurações; com elas abertas, ↑↓ andam, ← abas, → itens, Enter
        // aciona, Esc fecha.
        if ctx.input(|i| i.key_pressed(Key::F10)) {
            crate::settings::toggle(&mut self.app.lock());
        }
        if self.app.lock().settings_ui.is_some() {
            let (up, down, enter, esc, left, right) = ctx.input(|i| {
                (
                    i.key_pressed(Key::ArrowUp),
                    i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::Enter),
                    i.key_pressed(Key::Escape),
                    i.key_pressed(Key::ArrowLeft),
                    i.key_pressed(Key::ArrowRight),
                )
            });
            let mut a = self.app.lock();
            if left || right {
                crate::settings::focus_tabs(&mut a, left);
            }
            if up {
                crate::settings::scroll(&mut a, -1);
            }
            if down {
                crate::settings::scroll(&mut a, 1);
            }
            if esc {
                crate::settings::close(&mut a);
            }
            drop(a);
            if enter {
                crate::settings::activate(&self.app, &self.cmd);
            }
            return;
        }
        ctx.input(|i| {
            let mut a = self.app.lock();
            if i.key_pressed(Key::Delete) {
                a.delete_selected();
            }
            let nav_on = crate::navegador::is_on(&a);
            if i.modifiers.ctrl && i.key_pressed(Key::S) {
                let path = if nav_on { a.nav.selected_path() } else { a.lib.selected_path() };
                if let Some(path) = path {
                    drop(a);
                    let msg = crate::import::start(&self.app, path);
                    self.app.lock().toast(msg);
                    return;
                }
            }
            if i.modifiers.ctrl && i.key_pressed(Key::F) {
                search::open(&mut a);
                return;
            }
            // Browser novo aberto: setas andam nele, Enter abre, Backspace volta, Tab troca entre
            // as categorias e a lista, Esc ou B fecha, 1-4 carregam.
            if nav_on {
                use crate::navegador as nav;
                for (k, d) in [(Key::ArrowUp, -1), (Key::ArrowDown, 1), (Key::PageUp, -12), (Key::PageDown, 12)] {
                    if i.key_pressed(k) {
                        nav::scroll(&mut a, d);
                    }
                }
                for (k, d) in [(Key::ArrowLeft, -1), (Key::ArrowRight, 1)] {
                    if i.key_pressed(k) {
                        nav::side(&mut a, d);
                    }
                }
                if i.key_pressed(Key::Enter) {
                    nav::press(&mut a);
                }
                if i.key_pressed(Key::Backspace) {
                    a.nav.back();
                }
                if i.key_pressed(Key::Tab) {
                    a.nav.area = if a.nav.area == nav::Area::List { nav::Area::Cats } else { nav::Area::List };
                }
                if i.key_pressed(Key::Escape) || i.key_pressed(Key::B) {
                    a.big_browser = false;
                }
                for (k, d) in [(Key::Num1, 0), (Key::Num2, 1), (Key::Num3, 2), (Key::Num4, 3)] {
                    if let (true, Some(t)) = (i.key_pressed(k), a.nav.selected_track()) {
                        self.loader.load_deck(&mut a, d, t.path);
                        a.big_browser = false;
                    }
                }
                return;
            }
            if i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowUp) {
                // Mexer na lista abre o browser em tela cheia (como girar o BROWSE); o novo abre
                // onde parou, sem andar.
                let novo = crate::navegador::enabled(&a);
                if !(novo && !a.big_browser) {
                    a.lib.scroll(if i.key_pressed(Key::ArrowDown) { 1 } else { -1 });
                }
                a.big_browser = true;
            }
            if i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals) {
                a.wave_zoom = a.wave_zoom.saturating_sub(1);
            }
            if i.key_pressed(Key::Minus) {
                a.wave_zoom = (a.wave_zoom + 1).min(ZOOMS.len() - 1);
            }
            if i.key_pressed(Key::PageDown) {
                a.lib.scroll(12);
            }
            if i.key_pressed(Key::PageUp) {
                a.lib.scroll(-12);
            }
            if i.key_pressed(Key::Tab) {
                a.lib.toggle_focus();
            }
            if i.key_pressed(Key::Enter) && a.lib.focus == Focus::Folders {
                a.lib.open_selected();
            }
            if i.key_pressed(Key::B) {
                a.big_browser = !a.big_browser;
            }
            for (k, d) in [(Key::Num1, 0), (Key::Num2, 1), (Key::Num3, 2), (Key::Num4, 3)] {
                if i.key_pressed(k) {
                    if let Some(t) = a.lib.selected_track() {
                        self.loader.load_deck(&mut a, d, t.path);
                        a.big_browser = false;
                    }
                }
            }
        });
        let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));
        let released = |k: Key| ctx.input(|i| i.key_released(k));
        for (d, k) in [(side_l, Key::Q), (side_r, Key::P)] {
            if pressed(k) {
                self.cmd.send(Cmd::Keylock { deck: d, on: !snap.decks[d].keylock });
            }
        }
        for (d, play, cue, sync) in [(side_l, Key::Z, Key::X, Key::S), (side_r, Key::M, Key::N, Key::K)] {
            if pressed(play) {
                self.cmd.send(Cmd::PlayToggle(d));
            }
            if pressed(cue) {
                self.cmd.send(Cmd::Cue { deck: d, down: true });
            }
            if released(cue) {
                self.cmd.send(Cmd::Cue { deck: d, down: false });
            }
            if pressed(sync) {
                self.cmd.send(Cmd::Sync(d));
            }
        }
        // Efeitos: F1/F5 alavanca, F2/F6 batida menor, F3/F7 batida maior, F4/F8 troca o efeito.
        for (side, d, [lever, less, more, pick]) in
            [(0, side_l, [Key::F1, Key::F2, Key::F3, Key::F4]), (1, side_r, [Key::F5, Key::F6, Key::F7, Key::F8])]
        {
            let f = snap.decks[d].fx[0];
            if pressed(lever) {
                if f.level < 0.01 {
                    self.cmd.send(Cmd::FxLevel { deck: d, slot: 0, v: 0.6 });
                }
                self.cmd.send(Cmd::FxActive { deck: d, slot: 0, on: !f.active });
            }
            for (k, dir) in [(less, -1), (more, 1)] {
                if pressed(k) {
                    self.cmd.send(Cmd::FxBeats { deck: d, slot: 0, beats: step_beats(f.beats, dir) });
                }
            }
            if pressed(pick) {
                let mut a = self.app.lock();
                let shown = a.fx_pick[side].map_or(f.kind, |(k, _)| k);
                a.fx_pick[side] = Some((shown.next(1), std::time::Instant::now()));
            }
        }
        let dx = ctx.input(|i| {
            (if i.key_down(Key::ArrowRight) { 1.0 } else { 0.0 }) - (if i.key_down(Key::ArrowLeft) { 1.0 } else { 0.0 })
        });
        // (No browser novo as setas andam na lista.)
        if dx != 0.0 && !crate::navegador::is_on(&self.app.lock()) {
            self.xf = (self.xf + dx * 0.02).clamp(0.0, 1.0);
            self.cmd.send(Cmd::Crossfader(self.xf));
        }
    }

    /// Diagnóstico: DJOS_DEMO carrega as duas primeiras músicas de ~/DJOS/musicas e toca;
    /// DJOS_SCREENSHOT=<arquivo> salva a tela (RGBA cru com cabeçalho "largura altura\n").
    fn diagnostics(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        if std::env::var_os("DJOS_DEMO").is_some() {
            let dir = crate::app::data_dir().join("musicas");
            if self.frame == 5 {
                let mut files: Vec<_> = std::fs::read_dir(&dir).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
                files.sort();
                let mut a = self.app.lock();
                for (d, f) in files.into_iter().take(2).enumerate() {
                    self.loader.load_deck(&mut a, d, f);
                }
            }
            if self.frame == 60 {
                self.cmd.send(Cmd::Hotcue { deck: 0, idx: 0, down: true });
                self.cmd.send(Cmd::Hotcue { deck: 0, idx: 0, down: false });
                self.cmd.send(Cmd::PlayToggle(0));
                self.cmd.send(Cmd::PlayToggle(1));
                self.cmd.send(Cmd::Sync(1));
                self.cmd.send(Cmd::FxLevel { deck: 0, slot: 0, v: 0.6 });
                self.cmd.send(Cmd::FxActive { deck: 0, slot: 0, on: true });
                self.cmd.send(Cmd::LoopAuto { deck: 1, beats: 4.0 });
            }
        }
        if let (Ok(q), 30) = (std::env::var("DJOS_DEMO_BUSCA"), self.frame) {
            let mut a = self.app.lock();
            search::open(&mut a);
            if let Some(u) = a.search.as_mut() {
                u.query = q.to_uppercase();
                u.go_to("ENTER");
            }
        }
        // DJOS_DEMO_BROWSER=<pasta>: abre o browser em tela cheia nessa pasta (teste da tela).
        if let (Some(dir), 70) = (std::env::var_os("DJOS_DEMO_BROWSER"), self.frame) {
            let mut a = self.app.lock();
            a.big_browser = true;
            a.lib.reveal(std::path::Path::new(&dir));
            a.lib.focus = crate::library::Focus::Tracks;
            a.lib.track_sel = 3.min(a.lib.tracks.len().saturating_sub(1));
            let st: &mut crate::app::AppState = &mut a;
            st.nav.tick(&st.lib);
            st.nav.reveal(&st.lib, std::path::Path::new(&dir));
            let sel = st.nav.level().map_or(0, |l| l.sel + 3);
            st.nav.select(sel);
            // DJOS_DEMO_CATEGORIA=ARTISTA (etc.): abre essa categoria com a barra de cima em foco.
            if let Some(c) = std::env::var("DJOS_DEMO_CATEGORIA").ok().and_then(|n| crate::navegador::Cat::ALL.into_iter().find(|c| c.name() == n)) {
                st.nav.set_cat(&st.lib, c);
                st.nav.area = crate::navegador::Area::Top;
                st.nav.top = crate::navegador::TopButton::Preview;
            }
        }
        // DJOS_DEMO_GRADE: abre o ajuste da grade do deck 1 (teste da tela).
        if self.frame == 100 && std::env::var_os("DJOS_DEMO_GRADE").is_some() {
            self.app.lock().grid_edit = Some(crate::grid::GridEdit::new(0));
        }
        // DJOS_DEMO_GRAVAR=<segundos>: grava o set por esse tempo (teste da gravação).
        if let Some(secs) = std::env::var("DJOS_DEMO_GRAVAR").ok().and_then(|v| v.parse::<u64>().ok()) {
            if self.frame == 60 {
                crate::record::start(&self.app, &self.cmd, self.snap.lock().sample_rate);
            }
            let rec_secs = self.app.lock().rec.active.as_ref().map(|(_, t, _)| t.elapsed().as_secs());
            if rec_secs.is_some_and(|s| s >= secs) {
                crate::record::stop(&self.app, &self.cmd);
            }
        }
        if self.frame == 30 && std::env::var_os("DJOS_DEMO_CONFIG").is_some() {
            {
                let mut a = self.app.lock();
                crate::settings::open(&mut a);
                // DJOS_DEMO_CONFIG=<número da aba> abre nela; sem número, Wi-Fi procurando redes.
                let wifi = crate::settings::Tab::ALL.iter().position(|t| *t == crate::settings::Tab::Wifi).unwrap_or(0);
                let tab = std::env::var("DJOS_DEMO_CONFIG").ok().and_then(|v| v.parse().ok()).unwrap_or(wifi);
                crate::settings::select_tab(&mut a, tab);
            }
            // Só na aba Wi-Fi aperta o 1º item (procurar redes: só lista, não muda nada).
            if crate::settings::current_tab(&self.app.lock()) == crate::settings::Tab::Wifi {
                crate::settings::activate(&self.app, &self.cmd);
            }
        }
        if self.frame == 90 && std::env::var_os("DJOS_DEMO_ENTER").is_some() {
            search::submit(&mut self.app.lock());
        }
        if std::env::var_os("DJOS_DEBUG_SNAP").is_some() && self.frame % 60 == 0 {
            let s = *self.snap.lock();
            eprintln!("[snap] quadro {} sr={} deck1 pos={:.0} tocando={} bpm={:.1} rate={:.3} hotcues={}", self.frame, s.sample_rate, s.decks[0].pos, s.decks[0].playing, s.decks[0].bpm, s.decks[0].rate, s.decks[0].hotcues.iter().flatten().count());
        }
        if let Some(path) = std::env::var_os("DJOS_SCREENSHOT") {
            if self.frame == 240 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            let shot = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot {
                let mut bytes = format!("{} {}\n", img.size[0], img.size[1]).into_bytes();
                for c in &img.pixels {
                    bytes.extend_from_slice(&c.to_array());
                }
                let _ = std::fs::write(&path, bytes);
                eprintln!("[djos] tela salva em {:?}", path);
            }
        }
    }

    /// Salva cue, hotcues e loop 1 s depois da última mudança (gravação em segundo plano).
    fn persist(&mut self, snap: &Snapshot) {
        for d in 0..4 {
            let s = &snap.decks[d];
            if !s.loaded {
                continue;
            }
            let loop_ = match (s.loop_in, s.loop_out) {
                (Some(a), Some(b)) => Some((a, b, s.loop_beats)),
                _ => None,
            };
            let cur: CueState = (s.cue, s.hotcues, loop_);
            let p = &mut self.persist[d];
            if p.gen != s.track_gen {
                // Música nova no deck: o estado atual é o ponto de partida (nada a salvar).
                *p = Persist { gen: s.track_gen, state: Some(cur), changed_at: None };
                continue;
            }
            if p.state != Some(cur) {
                p.state = Some(cur);
                p.changed_at = Some(std::time::Instant::now());
            }
            if p.changed_at.is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(1)) {
                p.changed_at = None;
                let track = self.app.lock().decks[d].track.clone();
                if let Some(t) = track.filter(|t| t.gen == s.track_gen) {
                    let mut saved = crate::store::Saved::from_track(&t);
                    saved.cue = Some(cur.0);
                    saved.hotcues = cur.1;
                    saved.loop_ = cur.2;
                    std::thread::spawn(move || crate::store::save(&t.path, &saved));
                }
            }
        }
    }

    fn overview_for(&mut self, d: usize, t: &Track, width: usize) -> &Overview {
        let fresh = self.overview[d].as_ref().is_some_and(|o| o.gen == t.gen && o.width == width);
        if !fresh {
            let n = t.wave.low.len().max(1);
            let cols = (0..width)
                .map(|x| {
                    let a = x * n / width;
                    let b = ((x + 1) * n / width).max(a + 1).min(n);
                    let mut c = [0u8; 3];
                    for i in a..b {
                        c[0] = c[0].max(t.wave.low[i]);
                        c[1] = c[1].max(t.wave.mid[i]);
                        c[2] = c[2].max(t.wave.high[i]);
                    }
                    c
                })
                .collect();
            self.overview[d] = Some(Overview { gen: t.gen, width, cols });
        }
        self.overview[d].as_ref().unwrap()
    }
}

/// Waveform rolante de 3 bandas com grade de batidas, loop, cues e agulha no centro.
fn draw_wave(p: &Painter, r: Rect, d: usize, t: Option<&Track>, s: &DeckSnap, loading: Option<&str>, zoom_secs: f64) {
    let pal = wave_colors();
    p.rect_filled(r, 0.0, BG);
    let tag = Rect::from_min_size(r.min, vec2(26.0, r.height()));
    p.rect_filled(tag, 0.0, deck_color(d).gamma_multiply(0.85));
    text(p, tag.center(), Align2::CENTER_CENTER, d + 1, 20.0, BG);
    let r = Rect::from_min_max(pos2(r.left() + 26.0, r.top()), r.max);
    let mid = r.center().y;
    p.line_segment([pos2(r.left(), mid), pos2(r.right(), mid)], Stroke::new(1.0_f32, PANEL2));

    let Some(t) = t else {
        let msg = loading.map_or("Sem música".to_string(), |n| format!("Carregando {n}…"));
        text(p, r.center(), Align2::CENTER_CENTER, msg, 16.0, DIM);
        return;
    };
    let pos_s = s.pos / t.sr;
    let w = r.width();
    let px_per_s = w as f64 / zoom_secs;
    let hh = r.height() * 0.47;
    let nb = t.wave.low.len();
    let center_x = r.left() + w / 2.0;
    let t_of = |x: f32| pos_s + (x - center_x) as f64 / px_per_s;
    let x_of = |sec: f64| center_x + ((sec - pos_s) * px_per_s) as f32;

    // Região do loop.
    if let (true, Some(a), Some(b)) = (s.loop_on, s.loop_in, s.loop_out) {
        let (xa, xb) = (x_of(a / t.sr).max(r.left()), x_of(b / t.sr).min(r.right()));
        if xb > xa {
            p.rect_filled(Rect::from_x_y_ranges(xa..=xb, r.y_range()), 0.0, ACCENT.gamma_multiply(0.15));
        }
    }

    let mut mesh = Mesh::default();
    // Modo leve: colunas de 2 px e só graves + médios (4x menos retângulos).
    let (step, layers) = if light() { (2.0f32, 2) } else { (1.0f32, 3) };
    let mut x = r.left();
    while x < r.right() {
        let ts = t_of(x);
        if ts >= 0.0 {
            let b0 = (ts * BINS_PER_SEC) as usize;
            if b0 >= nb {
                break;
            }
            let b1 = ((t_of(x + step) * BINS_PER_SEC) as usize).clamp(b0 + 1, nb);
            let (mut l, mut m, mut h) = (0u8, 0u8, 0u8);
            for i in b0..b1 {
                l = l.max(t.wave.low[i]);
                m = m.max(t.wave.mid[i]);
                h = h.max(t.wave.high[i]);
            }
            let dim = if ts < pos_s { 0.45 } else { 1.0 };
            for (v, scale, col) in [(l, 1.0, pal[0]), (m, 0.85, pal[1]), (h, 0.6, pal[2])].into_iter().take(layers) {
                let a = v as f32 / 255.0 * hh * scale;
                if a > 0.5 {
                    mesh.add_colored_rect(Rect::from_min_max(pos2(x, mid - a), pos2(x + step, mid + a)), col.gamma_multiply(dim));
                }
            }
        }
        x += step;
    }
    p.add(Shape::mesh(mesh));

    // Grade de batidas (a cada 4, mais forte).
    if t.bpm() > 0.0 {
        let bl = t.beat_len() / t.sr;
        let fb = t.first_beat() / t.sr;
        let k0 = ((t_of(r.left()) - fb) / bl).ceil() as i64;
        let k1 = ((t_of(r.right()) - fb) / bl).floor() as i64;
        for k in k0..=k1 {
            let bx = x_of(fb + k as f64 * bl);
            let down = k.rem_euclid(4) == 0;
            let c = if down { Color32::from_rgba_unmultiplied(255, 255, 255, 110) } else { Color32::from_rgba_unmultiplied(255, 255, 255, 40) };
            p.line_segment([pos2(bx, r.top()), pos2(bx, r.bottom())], Stroke::new(1.0_f32, c));
            if down {
                p.rect_filled(Rect::from_min_size(pos2(bx - 3.0, r.top()), vec2(6.0, 4.0)), 0.0, RED);
            }
        }
    }

    // Cue e hotcues.
    let cx = x_of(s.cue / t.sr);
    if r.x_range().contains(cx) {
        p.add(Shape::convex_polygon(
            vec![pos2(cx - 6.0, r.bottom()), pos2(cx + 6.0, r.bottom()), pos2(cx, r.bottom() - 8.0)],
            ORANGE,
            Stroke::NONE,
        ));
    }
    for (i, h) in s.hotcues.iter().enumerate() {
        if let Some(hp) = h {
            let hx = x_of(hp / t.sr);
            if r.x_range().contains(hx) {
                p.line_segment([pos2(hx, r.top()), pos2(hx, r.bottom())], Stroke::new(1.5_f32, HOTCUE_COLORS[i]));
                let lab = Rect::from_min_size(pos2(hx, r.top()), vec2(14.0, 14.0));
                p.rect_filled(lab, 2.0, HOTCUE_COLORS[i]);
                text(p, lab.center(), Align2::CENTER_CENTER, i + 1, 11.0, BG);
            }
        }
    }

    p.line_segment([pos2(center_x, r.top()), pos2(center_x, r.bottom())], Stroke::new(2.0_f32, Color32::WHITE));
}

impl DjOsApp {
    fn draw_deck_info(&mut self, ui: &mut egui::Ui, p: &Painter, r: Rect, d: usize, t: Option<Arc<Track>>, s: &DeckSnap) {
        p.rect_filled(r, 6.0, PANEL);
        let col = deck_color(d);
        let r = r.shrink(10.0);
        let p = p.with_clip_rect(r.expand(2.0));

        let num = Rect::from_min_size(r.min, vec2(34.0, 34.0));
        p.rect_filled(num, 6.0, col);
        text(&p, num.center(), Align2::CENTER_CENTER, d + 1, 22.0, BG);

        let bpm_text = if s.loaded && s.bpm > 0.0 { format!("{:.2}", s.bpm) } else { "--.--".into() };
        let bpm_rect = mono(&p, pos2(r.right(), r.top() - 2.0), Align2::RIGHT_TOP, bpm_text, 30.0, TEXT);
        // Tom da música ao lado do BPM (mostra "--" enquanto não foi analisado).
        // Tocar no BPM abre o ajuste da grade de batidas.
        if t.is_some() && !s.live && ui.interact(bpm_rect.expand(4.0), ui.id().with(("bpm", d)), egui::Sense::click()).clicked() {
            let mut a = self.app.lock();
            let open = a.grid_edit.as_ref().is_some_and(|g| g.deck == d);
            a.grid_edit = if open { None } else { Some(crate::grid::GridEdit::new(d)) };
        }
        let bpm_rect = match t.as_ref().and_then(|t| crate::key::from_raw(t.key)).filter(|_| !s.live) {
            Some(k) => key_badge(&p, pos2(bpm_rect.left() - 8.0, bpm_rect.center().y), k, 15.0).union(bpm_rect),
            None => bpm_rect,
        };
        let pitch = (s.rate - 1.0) * 100.0;
        // Sem estado do motor (áudio parado) mostra traços em vez de números sem sentido.
        // Ganho automático aplicado (quando corrige mais de 0,1 dB) antes do pitch.
        let ag = if s.loaded && !s.live && s.auto_gain_db.abs() >= 0.1 { format!("AG {:+.1} dB   ", s.auto_gain_db) } else { String::new() };
        let pitch_text = if s.rate > 0.0 { format!("{ag}{:+.2}%  ±{:.0}", pitch, s.pitch_range * 100.0) } else { "--".into() };
        mono(
            &p,
            pos2(r.right(), bpm_rect.bottom() + 1.0),
            Align2::RIGHT_TOP,
            pitch_text,
            13.0,
            if pitch.abs() < 0.005 { DIM } else { ACCENT },
        );

        let title_clip = Rect::from_min_max(pos2(num.right() + 10.0, r.top()), pos2(bpm_rect.left() - 10.0, r.top() + 44.0));
        let tp = p.with_clip_rect(title_clip);
        match &t {
            _ if s.live => {
                let phone = self.app.lock().phone_status.clone();
                text(&tp, title_clip.left_top(), Align2::LEFT_TOP, "📱 CELULAR (Bluetooth)", 19.0, ACCENT);
                text(&tp, title_clip.left_top() + vec2(0.0, 23.0), Align2::LEFT_TOP, &phone, 14.0, DIM);
            }
            Some(t) => {
                text(&tp, title_clip.left_top(), Align2::LEFT_TOP, &t.title, 19.0, TEXT);
                text(&tp, title_clip.left_top() + vec2(0.0, 23.0), Align2::LEFT_TOP, &t.artist, 14.0, DIM);
            }
            None => {
                text(&tp, title_clip.left_top(), Align2::LEFT_TOP, "Deck vazio", 19.0, DIM);
            }
        }

        // Linha de tempo e estados.
        let y = r.top() + 58.0;
        if let Some(t) = &t {
            let rem = (s.frames - s.pos) / t.sr;
            let warn = rem < 30.0 && s.playing;
            let blink = warn && (s.pos / t.sr * 2.0) as i64 % 2 == 0;
            let rr = mono(&p, pos2(r.left(), y), Align2::LEFT_CENTER, format!("-{}", fmt_time(rem)), 24.0, if blink { RED } else { TEXT });
            mono(&p, pos2(rr.right() + 10.0, y + 3.0), Align2::LEFT_CENTER, fmt_time(s.pos / t.sr), 13.0, DIM);
        }
        let mut bx = r.left() + 250.0;
        bx = badge(&p, bx, y, "SYNC", s.sync_ok, ACCENT);
        bx = badge(&p, bx, y, if s.vinyl { "VINIL" } else { "CDJ" }, s.vinyl, col);
        // KEYLOCK: tocar liga/desliga.
        let kx = bx;
        bx = badge(&p, bx, y, "KEYLOCK", s.keylock, ACCENT);
        let kr = Rect::from_min_max(pos2(kx, y - 10.0), pos2(bx - 6.0, y + 10.0));
        if ui.interact(kr, ui.id().with(("keylock", d)), egui::Sense::click()).clicked() {
            self.cmd.send(Cmd::Keylock { deck: d, on: !s.keylock });
        }
        if s.loop_on {
            bx = badge(&p, bx, y, &format!("LOOP {}", beats_label(s.loop_beats as f32)), true, ACCENT);
        }
        if s.roll_beats > 0.0 {
            bx = badge(&p, bx, y, &format!("ROLL {}", beats_label(s.roll_beats as f32)), true, ORANGE);
        }
        if s.key_semis != 0.0 {
            bx = badge(&p, bx, y, &format!("TOM {:+}", s.key_semis), true, ORANGE);
        }
        let _ = bx;

        // Visão geral da música inteira.
        let ov = Rect::from_min_max(pos2(r.left(), y + 18.0), pos2(r.right(), r.bottom().max(y + 40.0)));
        p.rect_filled(ov, 3.0, BG);
        if let Some(t) = &t {
            let w = ov.width() as usize;
            let pal = wave_colors();
            let o = self.overview_for(d, t, w);
            let progress = (s.pos / s.frames.max(1.0)) as f32;
            let mut mesh = Mesh::default();
            let h2 = ov.height() / 2.0;
            // Modo leve: 1 coluna a cada 2 px e só uma camada.
            let (every, layers) = if light() { (2usize, 1) } else { (1usize, 3) };
            for (i, c) in o.cols.iter().enumerate().step_by(every) {
                let x = ov.left() + i as f32;
                let dim = if (i as f32) < progress * w as f32 { 0.4 } else { 1.0 };
                for (v, sc, colr) in [(c[0], 1.0, pal[0]), (c[1], 0.8, pal[1]), (c[2], 0.55, pal[2])].into_iter().take(layers) {
                    let a = v as f32 / 255.0 * h2 * sc;
                    if a > 0.3 {
                        mesh.add_colored_rect(
                            Rect::from_min_max(pos2(x, ov.center().y - a), pos2(x + every as f32, ov.center().y + a)),
                            colr.gamma_multiply(dim),
                        );
                    }
                }
            }
            p.add(Shape::mesh(mesh));
            for (i, h) in s.hotcues.iter().enumerate() {
                if let Some(hp) = h {
                    let hx = ov.left() + (hp / s.frames.max(1.0)) as f32 * ov.width();
                    p.line_segment([pos2(hx, ov.top()), pos2(hx, ov.bottom())], Stroke::new(1.0_f32, HOTCUE_COLORS[i]));
                }
            }
            let px = ov.left() + progress * ov.width();
            p.line_segment([pos2(px, ov.top() - 2.0), pos2(px, ov.bottom() + 2.0)], Stroke::new(2.0_f32, Color32::WHITE));
        }

    }

    /// Coluna do efeito ao lado da waveform: alavanca, nome, batida e força. Durante a escolha
    /// (FX 1) mostra a lista inteira. Ao toque: alavanca liga/desliga, nome troca, −/+ mudam a batida.
    /// Ajuste da grade de batidas de um deck (BPM, posição das batidas, TAP).
    fn draw_grid_editor(&mut self, ui: &mut egui::Ui, p: &Painter, r: Rect, d: usize, t: &Arc<Track>, s: &DeckSnap) {
        p.rect_filled(r, 8.0, PANEL);
        p.rect_stroke(r, 8.0, Stroke::new(2.0_f32, ACCENT));
        let tp = p.with_clip_rect(r.shrink(6.0));
        text(&tp, r.left_top() + vec2(14.0, 10.0), Align2::LEFT_TOP, format!("GRADE DE BATIDAS · DECK {} · {}", d + 1, t.title), 14.0, ACCENT);
        mono(&tp, r.center_top() + vec2(0.0, 34.0), Align2::CENTER_TOP, format!("{:.2} BPM", t.bpm()), 26.0, TEXT);
        let close = Rect::from_min_size(pos2(r.right() - 116.0, r.top() + 8.0), vec2(104.0, 28.0));
        p.rect_filled(close, 5.0, PANEL2);
        text(p, close.center(), Align2::CENTER_CENTER, "X  FECHAR", 12.0, TEXT);
        let mut changed = false;
        let mut done = ui.interact(close, ui.id().with("grade-fechar"), egui::Sense::click()).clicked();
        let taps = self.app.lock().grid_edit.as_ref().map_or(0, |g| g.taps());
        let tap_label = if taps > 0 && taps < 4 { format!("TAP ({taps})") } else { "TAP".to_string() };
        let rows: [Vec<(String, u8)>; 2] = [
            ["÷2", "-1", "-0.1", "-0.01", tap_label.as_str(), "+0.01", "+0.1", "+1", "×2"].iter().enumerate().map(|(i, s)| (s.to_string(), i as u8)).collect(),
            ["-½ BATIDA", "-10 ms", "-1 ms", "BATIDA AQUI", "+1 ms", "+10 ms", "+½ BATIDA"].iter().enumerate().map(|(i, s)| (s.to_string(), 20 + i as u8)).collect(),
        ];
        let bh = ((r.height() - 90.0) / 2.0 - 8.0).clamp(28.0, 52.0);
        for (ri, row) in rows.iter().enumerate() {
            let y = r.top() + 76.0 + ri as f32 * (bh + 8.0);
            let bw = (r.width() - 24.0 - (row.len() - 1) as f32 * 6.0) / row.len() as f32;
            for (k, (label, id)) in row.iter().enumerate() {
                let b = Rect::from_min_size(pos2(r.left() + 12.0 + k as f32 * (bw + 6.0), y), vec2(bw, bh));
                let main = matches!(*id, 4 | 23);
                p.rect_filled(b, 5.0, if main { ACCENT.gamma_multiply(0.3) } else { PANEL2 });
                p.rect_stroke(b, 5.0, Stroke::new(1.0_f32, if main { ACCENT } else { LINE }));
                text(&p.with_clip_rect(b), b.center(), Align2::CENTER_CENTER, label, 13.0, TEXT);
                if !ui.interact(b, ui.id().with(("grade", *id)), egui::Sense::click()).clicked() {
                    continue;
                }
                changed = true;
                match *id {
                    0 => crate::grid::set_bpm(t, t.bpm() / 2.0),
                    1 => crate::grid::set_bpm(t, t.bpm() - 1.0),
                    2 => crate::grid::set_bpm(t, t.bpm() - 0.1),
                    3 => crate::grid::set_bpm(t, t.bpm() - 0.01),
                    4 => {
                        let bpm = self.app.lock().grid_edit.as_mut().and_then(|g| g.tap(std::time::Instant::now(), s.rate));
                        match bpm {
                            Some(b) => crate::grid::set_bpm(t, (b * 100.0).round() / 100.0),
                            None => changed = false,
                        }
                    }
                    5 => crate::grid::set_bpm(t, t.bpm() + 0.01),
                    6 => crate::grid::set_bpm(t, t.bpm() + 0.1),
                    7 => crate::grid::set_bpm(t, t.bpm() + 1.0),
                    8 => crate::grid::set_bpm(t, t.bpm() * 2.0),
                    20 => crate::grid::shift_beats(t, -0.5),
                    21 => crate::grid::shift_secs(t, -0.010),
                    22 => crate::grid::shift_secs(t, -0.001),
                    23 => crate::grid::beat_here(t, s.pos),
                    24 => crate::grid::shift_secs(t, 0.001),
                    25 => crate::grid::shift_secs(t, 0.010),
                    _ => crate::grid::shift_beats(t, 0.5),
                }
            }
        }
        text(
            &tp,
            r.left_bottom() + vec2(14.0, -8.0),
            Align2::LEFT_BOTTOM,
            "TAP: bata 4 vezes no ritmo · BATIDA AQUI: põe uma batida onde a música está agora · grava sozinho",
            11.0,
            DIM,
        );
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            done = true;
        }
        if changed {
            // Grava na memória da música (em segundo plano; mantém os cues).
            let (bpm, path) = (t.bpm(), t.path.clone());
            let t = t.clone();
            std::thread::spawn(move || crate::grid::save(&t));
            let mut a = self.app.lock();
            for e in a.lib.tracks.iter_mut().filter(|e| e.path == path) {
                e.bpm = Some(bpm);
            }
        }
        if done {
            self.app.lock().grid_edit = None;
        }
    }

    fn draw_fx(&self, ui: &mut egui::Ui, p: &Painter, r: Rect, side: usize, d: usize, s: &DeckSnap, pick: Option<FxKind>) {
        let f = s.fx[0];
        let col = deck_color(d);
        p.rect_filled(r, 6.0, if f.active { col.gamma_multiply(0.25) } else { PANEL });
        let r = r.shrink(10.0);
        let p = p.with_clip_rect(r.expand(2.0));
        let id = ui.id().with(("fx", side));
        text(&p, r.left_top(), Align2::LEFT_TOP, format!("EFEITO · DECK {}", d + 1), 12.0, DIM);

        if let Some(k) = pick {
            let n = FxKind::ALL.len() as f32;
            let h = ((r.height() - 20.0) / n).min(26.0);
            for (i, kind) in FxKind::ALL.iter().enumerate() {
                let cell = Rect::from_min_size(pos2(r.left(), r.top() + 20.0 + i as f32 * h), vec2(r.width(), h - 3.0));
                let on = *kind == k;
                p.rect_filled(cell, 4.0, if on { ACCENT } else { PANEL2 });
                text(&p, cell.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, kind.name(), 14.0, if on { BG } else { DIM });
            }
            return;
        }

        let lever = Rect::from_min_size(pos2(r.right() - 64.0, r.top()), vec2(64.0, 30.0));
        p.rect_filled(lever, 5.0, if f.active { col } else { PANEL2 });
        text(&p, lever.center(), Align2::CENTER_CENTER, if f.active { "ON" } else { "OFF" }, 15.0, if f.active { BG } else { DIM });
        if ui.interact(lever, id.with("alavanca"), egui::Sense::click()).clicked() {
            if f.level < 0.01 {
                self.cmd.send(Cmd::FxLevel { deck: d, slot: 0, v: 0.6 });
            }
            self.cmd.send(Cmd::FxActive { deck: d, slot: 0, on: !f.active });
        }

        let name = Rect::from_min_size(pos2(r.left(), r.top() + 36.0), vec2(r.width(), 34.0));
        p.rect_filled(name, 5.0, PANEL2);
        text(&p, name.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, f.kind.name(), 22.0, if f.active || f.sounding { TEXT } else { DIM });
        text(&p, name.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, "FX 1", 11.0, DIM);
        if ui.interact(name, id.with("nome"), egui::Sense::click()).clicked() {
            let mut a = self.app.lock();
            let shown = a.fx_pick[side].map_or(f.kind, |(k, _)| k);
            a.fx_pick[side] = Some((shown.next(1), std::time::Instant::now()));
        }

        let row = Rect::from_min_size(pos2(r.left(), name.bottom() + 8.0), vec2(r.width(), 44.0));
        let minus = Rect::from_min_size(row.min, vec2(44.0, row.height()));
        let plus = Rect::from_min_size(pos2(row.right() - 44.0, row.top()), vec2(44.0, row.height()));
        for (b, label, dir) in [(minus, "-", -1), (plus, "+", 1)] {
            p.rect_filled(b, 5.0, PANEL2);
            mono(&p, b.center(), Align2::CENTER_CENTER, label, 22.0, TEXT);
            if f.kind.uses_beats() && ui.interact(b, id.with(label), egui::Sense::click()).clicked() {
                self.cmd.send(Cmd::FxBeats { deck: d, slot: 0, beats: step_beats(f.beats, dir) });
            }
        }
        let beats = if f.kind.uses_beats() { beats_label(f.beats) } else { "--".into() };
        mono(&p, row.center(), Align2::CENTER_CENTER, beats, 28.0, ACCENT);

        let bar_y = row.bottom() + 14.0;
        if bar_y + 20.0 < r.bottom() {
            text(&p, pos2(r.left(), bar_y), Align2::LEFT_TOP, "FORÇA", 11.0, DIM);
            mono(&p, pos2(r.right(), bar_y), Align2::RIGHT_TOP, format!("{:.0}%", f.level * 100.0), 13.0, TEXT);
            let bar = Rect::from_min_size(pos2(r.left(), bar_y + 18.0), vec2(r.width(), 10.0));
            p.rect_filled(bar, 4.0, BG);
            p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * f.level, 10.0)), 4.0, col);
        }
    }

    /// Pads de um deck: modo atual e o estado de cada pad. Tocar no nome do modo troca o modo;
    /// tocar num pad faz o mesmo que o pad da REV1 (segurar vale para hotcue, roll e trans).
    #[allow(clippy::too_many_arguments)]
    fn draw_pads(&mut self, ui: &mut egui::Ui, p: &Painter, r: Rect, d: usize, s: &DeckSnap, snap: &Snapshot, sampler_names: &[Option<String>], scratch_names: &[Option<String>]) {
        use crate::app::PadMode;
        use crate::midi::rev1::{pad_action, AUTOLOOP_PADS, BEATJUMP_PADS, ROLL_PADS, TRACKING_SEMIS};
        let mode = self.app.lock().pad_mode[d];
        let col = deck_color(d);
        p.rect_filled(r, 6.0, PANEL);
        let r = r.shrink(8.0);
        let head = Rect::from_min_size(r.min, vec2(r.width(), 22.0));
        let hp = p.with_clip_rect(head);
        text(&hp, head.left_center(), Align2::LEFT_CENTER, format!("PADS · DECK {}", d + 1), 11.0, DIM);
        text(&hp, head.right_center(), Align2::RIGHT_CENTER, mode.name(), 14.0, col);
        if ui.interact(head, ui.id().with(("modo", d)), egui::Sense::click()).clicked() {
            let next = PadMode::ALL[(PadMode::ALL.iter().position(|m| *m == mode).unwrap_or(0) + 1) % 8];
            self.app.lock().pad_mode[d] = next;
        }
        let blink = (self.frame / 24) % 2 == 0;
        let gw = (r.width() - 3.0 * 6.0) / 4.0;
        let gh = ((r.bottom() - head.bottom() - 6.0 - 6.0) / 2.0).max(20.0);
        const TRANS: [&str; 8] = ["1", "1/2", "1/4", "1/8", "1/16", "2/3", "1/3", "1/6"];
        for i in 0..8 {
            let cell = Rect::from_min_size(
                pos2(r.left() + (i % 4) as f32 * (gw + 6.0), head.bottom() + 6.0 + (i / 4) as f32 * (gh + 6.0)),
                vec2(gw, gh),
            );
            let short = |n: &Option<String>| n.clone().unwrap_or_else(|| "vazio".into());
            let (label, fill): (String, Option<Color32>) = match mode {
                PadMode::HotCue => ((i + 1).to_string(), s.hotcues[i].map(|_| HOTCUE_COLORS[i])),
                PadMode::AutoLoop => (
                    beats_label(AUTOLOOP_PADS[i] as f32),
                    (s.loop_on && (s.loop_beats - AUTOLOOP_PADS[i]).abs() < 1e-9).then_some(ACCENT),
                ),
                PadMode::Tracking => (format!("{:+}", TRACKING_SEMIS[i]), (s.key_semis == TRACKING_SEMIS[i] && s.key_semis != 0.0).then_some(ORANGE)),
                PadMode::Sampler => (
                    short(&sampler_names[i]),
                    sampler_names[i].as_ref().map(|_| if snap.sampler_playing[i] && blink { ACCENT } else { col.gamma_multiply(0.6) }),
                ),
                PadMode::BeatJump => (format!("{:+}", BEATJUMP_PADS[i]), None),
                PadMode::Roll => (beats_label(ROLL_PADS[i] as f32), ((s.roll_beats - ROLL_PADS[i]).abs() < 1e-9).then_some(ORANGE)),
                PadMode::Trans => (TRANS[i].to_string(), None),
                PadMode::ScratchBank => (short(&scratch_names[i]), scratch_names[i].as_ref().map(|_| col.gamma_multiply(0.6))),
            };
            match fill {
                Some(c) => {
                    p.rect_filled(cell, 5.0, c);
                }
                None => {
                    p.rect_filled(cell, 5.0, PANEL2);
                    p.rect_stroke(cell, 5.0, Stroke::new(1.0_f32, LINE));
                }
            }
            let cp = p.with_clip_rect(cell.shrink(2.0));
            text(&cp, cell.center(), Align2::CENTER_CENTER, label, 13.0, if fill.is_some() { BG } else { TEXT });

            let resp = ui.interact(cell, ui.id().with(("pad", d, i)), egui::Sense::click_and_drag());
            let down = resp.is_pointer_button_down_on();
            if down != self.pad_touch[d][i] {
                self.pad_touch[d][i] = down;
                pad_action(&self.app, &self.cmd, snap, d, mode, i, false, down);
            }
        }
    }

    fn draw_library(&self, ui: &mut egui::Ui, p: &Painter, r: Rect) {
        let mut a = self.app.lock();
        let a_import = a.import.clone();
        let lib = &mut a.lib;
        let row_h = 26.0;
        let gap = 8.0;
        let left_w = (r.width() * 0.27).round();
        let prep_w = if lib.prepare.is_empty() && lib.focus != Focus::Prepare { 0.0 } else { (r.width() * 0.22).round() };
        let folders = Rect::from_min_size(r.min, vec2(left_w, r.height()));
        let tracks = Rect::from_min_max(pos2(folders.right() + gap, r.top()), pos2(r.right() - if prep_w > 0.0 { prep_w + gap } else { 0.0 }, r.bottom()));
        let prep = Rect::from_min_max(pos2(tracks.right() + gap, r.top()), r.max);

        let header = |p: &Painter, rr: Rect, title: &str, focused: bool| {
            p.rect_filled(rr, 6.0, PANEL);
            if focused {
                p.rect_stroke(rr, 6.0, Stroke::new(2.0_f32, ACCENT));
            }
            text(p, rr.left_top() + vec2(12.0, 8.0), Align2::LEFT_TOP, title, 12.0, if focused { ACCENT } else { DIM });
        };
        // Janela visível que acompanha a seleção.
        let window = |sel: usize, len: usize, rows: usize| -> std::ops::Range<usize> {
            if len <= rows {
                return 0..len;
            }
            let start = sel.saturating_sub(rows / 2).min(len - rows);
            start..start + rows
        };

        // Pastas.
        header(p, folders, "PASTAS", lib.focus == Focus::Folders);
        let body = Rect::from_min_max(folders.min + vec2(4.0, 28.0), folders.max - vec2(4.0, 100.0));
        let rows = (body.height() / row_h) as usize;
        let fp = p.with_clip_rect(body);
        for (k, i) in window(lib.folder_sel, lib.folders.len(), rows).enumerate() {
            let e = &lib.folders[i];
            let rr = Rect::from_min_size(body.min + vec2(0.0, k as f32 * row_h), vec2(body.width(), row_h - 2.0));
            if i == lib.folder_sel {
                fp.rect_filled(rr, 4.0, if lib.focus == Focus::Folders { ACCENT.gamma_multiply(0.25) } else { PANEL2 });
            }
            let x = rr.left() + 8.0 + e.depth as f32 * 16.0;
            if e.has_children {
                mono(&fp, pos2(x, rr.center().y), Align2::LEFT_CENTER, if e.expanded { "-" } else { "+" }, 14.0, DIM);
            }
            let col = if e.kind.is_virtual() { ACCENT } else { TEXT };
            text(&fp, pos2(x + 14.0, rr.center().y), Align2::LEFT_CENTER, &e.name, 14.0, col);
            let resp = ui.interact(rr, ui.id().with(("pasta", i)), egui::Sense::click());
            if resp.clicked() || resp.double_clicked() {
                // Toque no +/-, de novo na pasta já selecionada ou duplo clique: abre/fecha a árvore.
                let on_toggle = resp.interact_pointer_pos().is_some_and(|pt| pt.x < x + 14.0);
                let toggle = e.has_children && (on_toggle || i == lib.folder_sel || resp.double_clicked());
                lib.focus = Focus::Folders;
                let delta = i as i32 - lib.folder_sel as i32;
                lib.scroll(delta);
                if toggle {
                    lib.open_selected();
                }
            }
        }
        if lib.folders.is_empty() {
            text(&fp, body.center(), Align2::CENTER_CENTER, "Conecte um pendrive ou SSD", 14.0, DIM);
        }

        // Ações (toque): salvar a pasta no sistema, apagar das músicas internas, andamento da cópia.
        let internal = crate::import::internal_dir();
        let sel = lib.selected_path();
        let recdir = crate::record::dir();
        // Gravação selecionada: o primeiro botão copia para o pendrive em vez de importar.
        let is_rec = sel.as_ref().is_some_and(|p| p.starts_with(&recdir) && p.is_file());
        let can_import = lib.focus == Focus::Folders && sel.as_ref().is_some_and(|p| !p.starts_with(&internal) && !p.starts_with(&recdir));
        let can_delete = sel.as_ref().is_some_and(|p| (p.starts_with(&internal) && *p != internal) || (p.starts_with(&recdir) && *p != recdir));
        let first = if is_rec { ("COPIAR P/ PENDRIVE", true, "exportar") } else { ("SALVAR NO SISTEMA", can_import, "importar") };
        let actions = Rect::from_min_max(pos2(folders.left() + 8.0, folders.bottom() - 94.0), folders.max - vec2(8.0, 8.0));
        let bw = (actions.width() - 6.0) / 2.0;
        let mut action: Option<&str> = None;
        for (k, (label, enabled, act)) in [first, ("APAGAR", can_delete, "apagar")].into_iter().enumerate() {
            let b = Rect::from_min_size(pos2(actions.left() + k as f32 * (bw + 6.0), actions.top()), vec2(bw, 40.0));
            p.rect_filled(b, 5.0, if enabled { PANEL2 } else { PANEL });
            p.rect_stroke(b, 5.0, Stroke::new(1.0_f32, if enabled { if act == "apagar" { RED } else { ACCENT } } else { LINE }));
            let bp = p.with_clip_rect(b);
            text(&bp, b.center(), Align2::CENTER_CENTER, label, 12.0, if enabled { TEXT } else { DIM });
            if enabled && ui.interact(b, ui.id().with(("acao", k)), egui::Sense::click()).clicked() {
                action = Some(act);
            }
        }
        let status = Rect::from_min_max(pos2(actions.left(), actions.top() + 48.0), actions.max);
        match &a_import {
            Some(pr) if !pr.finished => {
                let frac = pr.bytes_done as f32 / pr.bytes_total.max(1) as f32;
                let bar = Rect::from_min_size(status.left_bottom() - vec2(0.0, 10.0), vec2(status.width(), 8.0));
                p.rect_filled(bar, 3.0, BG);
                p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * frac, 8.0)), 3.0, ACCENT);
                let sp = p.with_clip_rect(status);
                text(&sp, status.left_top(), Align2::LEFT_TOP, format!("Copiando {}: {}/{}", pr.name, pr.files_done, pr.files_total), 12.0, TEXT);
            }
            _ => {
                text(p, status.left_top(), Align2::LEFT_TOP, "SHIFT + LOAD na pasta: salvar no sistema", 11.0, DIM);
            }
        }

        // Músicas.
        header(p, tracks, &format!("MÚSICAS ({})", lib.tracks.len()), lib.focus == Focus::Tracks);
        let body = Rect::from_min_max(tracks.min + vec2(4.0, 28.0), tracks.max - vec2(4.0, 4.0));
        let rows = (body.height() / row_h) as usize;
        let tp = p.with_clip_rect(body);
        let artist_x = body.left() + body.width() * 0.58;
        for (k, i) in window(lib.track_sel, lib.tracks.len(), rows).enumerate() {
            let e = &lib.tracks[i];
            let rr = Rect::from_min_size(body.min + vec2(0.0, k as f32 * row_h), vec2(body.width(), row_h - 2.0));
            if i == lib.track_sel {
                tp.rect_filled(rr, 4.0, if lib.focus == Focus::Tracks { ACCENT.gamma_multiply(0.25) } else { PANEL2 });
            }
            let cp = tp.with_clip_rect(Rect::from_min_max(rr.min, pos2(artist_x - 10.0, rr.bottom())));
            // Já tocou nesta noite: ✓ e apagadinha (fora das "Tocadas hoje", onde todas tocaram).
            let played = e.when.is_none() && lib.played.contains(&e.path);
            let tx = if played {
                // ✓ desenhado (a fonte não tem o símbolo).
                let c = rr.left_center() + vec2(14.0, 0.0);
                let st = Stroke::new(2.0_f32, ACCENT);
                cp.line_segment([c + vec2(-5.0, 0.0), c + vec2(-1.5, 4.0)], st);
                cp.line_segment([c + vec2(-1.5, 4.0), c + vec2(5.0, -4.5)], st);
                26.0
            } else {
                10.0
            };
            text(&cp, rr.left_center() + vec2(tx, 0.0), Align2::LEFT_CENTER, &e.title, 14.0, if played { DIM } else { TEXT });
            let ap = tp.with_clip_rect(Rect::from_min_max(pos2(artist_x, rr.top()), pos2(rr.right() - 150.0, rr.bottom())));
            text(&ap, pos2(artist_x, rr.center().y), Align2::LEFT_CENTER, &e.artist, 14.0, DIM);
            if let Some(k) = e.key {
                key_badge(&tp, rr.right_center() - vec2(104.0, 0.0), k, 12.0);
            }
            let bpm = e.bpm.filter(|b| *b > 0.0).map_or("--".to_string(), |b| format!("{b:.1}"));
            mono(&tp, rr.right_center() - vec2(52.0, 0.0), Align2::RIGHT_CENTER, bpm, 13.0, if e.bpm.is_some() { ACCENT } else { DIM });
            // Nas "Tocadas hoje", a hora em que tocou no lugar do tipo do arquivo.
            match &e.when {
                Some(w) => mono(&tp, rr.right_center() - vec2(4.0, 0.0), Align2::RIGHT_CENTER, w.clone(), 12.0, ACCENT),
                None => text(&tp, rr.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, &e.ext, 11.0, DIM),
            };
            if ui.interact(rr, ui.id().with(("musica", i)), egui::Sense::click()).clicked() {
                lib.focus = Focus::Tracks;
                lib.track_sel = i;
            }
        }
        if lib.tracks.is_empty() {
            text(&tp, body.center(), Align2::CENTER_CENTER, "Nenhuma música nesta pasta", 14.0, DIM);
        }

        // Preparar.
        if prep_w > 0.0 {
            header(p, prep, "PREPARAR", lib.focus == Focus::Prepare);
            let body = Rect::from_min_max(prep.min + vec2(4.0, 28.0), prep.max - vec2(4.0, 4.0));
            let rows = (body.height() / row_h) as usize;
            let pp = p.with_clip_rect(body);
            for (k, i) in window(lib.prepare_sel, lib.prepare.len(), rows).enumerate() {
                let e = &lib.prepare[i];
                let rr = Rect::from_min_size(body.min + vec2(0.0, k as f32 * row_h), vec2(body.width(), row_h - 2.0));
                if i == lib.prepare_sel && lib.focus == Focus::Prepare {
                    pp.rect_filled(rr, 4.0, ACCENT.gamma_multiply(0.25));
                }
                text(&pp, rr.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, &e.title, 13.0, TEXT);
            }
        }
        // As ações rodam depois de soltar a trava do estado (elas mesmas travam).
        drop(a);
        match action {
            Some("importar") => {
                if let Some(path) = sel {
                    let msg = crate::import::start(&self.app, path);
                    self.app.lock().toast(msg);
                }
            }
            Some("apagar") => self.app.lock().delete_selected(),
            Some("exportar") => {
                if let Some(path) = sel {
                    crate::record::export(&self.app, path);
                }
            }
            _ => {}
        }
    }
}

impl DjOsApp {
    /// Teclado da pesquisa por cima do browser: texto, contagem ao vivo e teclas (dá para tocar).
    fn draw_search(&mut self, ui: &mut egui::Ui, p: &Painter, r: Rect) {
        let mut a = self.app.lock();
        let Some(st) = a.search.clone() else { return };
        if self.search_count.0 != st.query {
            let n = if st.query.trim().is_empty() { 0 } else { a.lib.count_matches(&st.query) };
            self.search_count = (st.query.clone(), n);
        }
        let scanning = a.lib.index.lock().scanning;
        p.rect_filled(r, 8.0, Color32::from_rgba_unmultiplied(9, 11, 14, 245));
        p.rect_stroke(r, 8.0, Stroke::new(2.0_f32, ACCENT));
        let inner = r.shrink(16.0);

        // Campo de texto.
        let field = Rect::from_min_size(inner.min, vec2(inner.width(), 46.0));
        p.rect_filled(field, 6.0, PANEL2);
        let title = match &st.purpose {
            search::Purpose::Search => "PESQUISAR".to_string(),
            search::Purpose::WifiPassword(ssid) => format!("SENHA DE {ssid}"),
        };
        let tp = p.with_clip_rect(Rect::from_min_size(field.min, vec2(100.0, field.height())));
        text(&tp, field.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, title, 12.0, DIM);
        let cursor = if (self.frame / 30) % 2 == 0 { "_" } else { " " };
        mono(p, field.left_center() + vec2(110.0, 0.0), Align2::LEFT_CENTER, format!("{}{cursor}", st.query), 22.0, TEXT);
        let info = if st.purpose != search::Purpose::Search {
            "ENTER conecta".to_string()
        } else if scanning {
            "lendo as músicas…".to_string()
        } else if st.query.trim().is_empty() {
            "digite o nome da música ou do artista".to_string()
        } else {
            format!("{} música(s)", self.search_count.1)
        };
        text(p, field.right_center() - vec2(14.0, 0.0), Align2::RIGHT_CENTER, info, 14.0, ACCENT);

        // Teclas.
        let top = field.bottom() + 12.0;
        let hint_h = 22.0;
        let rows = st.rows();
        let row_h = ((inner.bottom() - top - hint_h) / rows.len() as f32 - 6.0).max(26.0);
        let key_w = (inner.width() - 9.0 * 6.0) / 10.0;
        let mut clicked: Option<&str> = None;
        for (ri, row) in rows.iter().enumerate() {
            let wide = ri == rows.len() - 1;
            let n = row.len() as f32;
            let w = if wide { (inner.width() - (n - 1.0) * 6.0) / n } else { key_w };
            let row_w = row.len() as f32 * (w + 6.0) - 6.0;
            let x0 = inner.left() + (inner.width() - row_w) / 2.0;
            let y = top + ri as f32 * (row_h + 6.0);
            for (ci, key) in row.iter().enumerate() {
                let kr = Rect::from_min_size(pos2(x0 + ci as f32 * (w + 6.0), y), vec2(w, row_h));
                let sel = st.row == ri && st.col == ci;
                p.rect_filled(kr, 5.0, if sel { ACCENT } else { PANEL2 });
                if *key == "ENTER" && !sel {
                    p.rect_stroke(kr, 5.0, Stroke::new(1.5_f32, ACCENT));
                }
                text(p, kr.center(), Align2::CENTER_CENTER, st.label(key), if wide { 15.0 } else { 20.0 }, if sel { BG } else { TEXT });
                if ui.interact(kr, ui.id().with(("tecla", ri, ci)), egui::Sense::click()).clicked() {
                    clicked = Some(key);
                }
            }
        }
        text(
            p,
            pos2(inner.center().x, inner.bottom() - hint_h / 2.0),
            Align2::CENTER_CENTER,
            "BROWSE: girar escolhe · apertar digita   |   pads: 1 cima · 2 baixo · 3 esq · 4 dir · 5 confirma · 6 apaga · 7 espaço · 8 fecha",
            12.0,
            DIM,
        );
        if let Some(k) = clicked {
            if let Some(u) = a.search.as_mut() {
                u.go_to(k);
            }
            search::press_key(&mut a, k);
        }
    }
}

impl DjOsApp {
    /// Tela de configurações: seções com itens; girar o BROWSE escolhe, apertar (ou tocar) muda.
    fn draw_settings(&self, ui: &mut egui::Ui, p: &Painter, r: Rect) {
        use crate::settings::{rows, selectable, tab_summary, Row, Tab};
        let (rows, sel_row, tab, on_tabs, summaries) = {
            let a = self.app.lock();
            let rows = rows(&a);
            let (sel, tab, on_tabs) = a.settings_ui.as_ref().map_or((0, 0, true), |u| (u.sel, u.tab, u.on_tabs));
            let sel_row = if on_tabs { None } else { selectable(&rows).get(sel).copied() };
            let summaries: Vec<String> = Tab::ALL.iter().map(|t| tab_summary(&a, *t)).collect();
            (rows, sel_row, tab, on_tabs, summaries)
        };
        p.rect_filled(r, 8.0, PANEL);
        p.rect_stroke(r, 8.0, Stroke::new(2.0_f32, ACCENT));
        text(p, r.left_top() + vec2(16.0, 12.0), Align2::LEFT_TOP, "CONFIGURAÇÕES", 18.0, ACCENT);
        text(
            p,
            pos2(r.center().x + 60.0, r.top() + 14.0),
            Align2::CENTER_TOP,
            if on_tabs { "BROWSE: gira = aba · aperta = entra" } else { "BROWSE: gira = item · aperta = escolhe · para cima no 1º = abas" },
            12.0,
            DIM,
        );
        // Fechar (toque); também SHIFT + MASTER CUE, F10 ou Esc.
        let close = Rect::from_min_size(pos2(r.right() - 120.0, r.top() + 8.0), vec2(108.0, 30.0));
        p.rect_filled(close, 5.0, PANEL2);
        text(p, close.center(), Align2::CENTER_CENTER, "X  FECHAR", 13.0, TEXT);
        if ui.interact(close, ui.id().with("config-fechar"), egui::Sense::click()).clicked() {
            crate::settings::close(&mut self.app.lock());
            return;
        }

        // Abas à esquerda.
        let tabs = Rect::from_min_max(r.min + vec2(12.0, 48.0), pos2(r.left() + 250.0, r.bottom() - 12.0));
        let tab_h = ((tabs.height() - 6.0 * 6.0) / Tab::ALL.len() as f32).min(64.0);
        let mut tab_click = None;
        for (i, t) in Tab::ALL.iter().enumerate() {
            let tr = Rect::from_min_size(tabs.min + vec2(0.0, i as f32 * (tab_h + 6.0)), vec2(tabs.width(), tab_h));
            let open = i == tab;
            p.rect_filled(tr, 6.0, if open { ACCENT.gamma_multiply(if on_tabs { 0.35 } else { 0.18 }) } else { PANEL2 });
            if open && on_tabs {
                p.rect_stroke(tr, 6.0, Stroke::new(2.0_f32, ACCENT));
            }
            let tp = p.with_clip_rect(tr.shrink(4.0));
            text(&tp, tr.left_center() + vec2(14.0, -9.0), Align2::LEFT_CENTER, t.name(), 15.0, if open { TEXT } else { DIM });
            text(&tp, tr.left_center() + vec2(14.0, 11.0), Align2::LEFT_CENTER, &summaries[i], 12.0, if open { ACCENT } else { DIM });
            if ui.interact(tr, ui.id().with(("config-aba", i)), egui::Sense::click()).clicked() {
                tab_click = Some(i);
            }
        }

        // Itens da aba à direita.
        let full_body = Rect::from_min_max(pos2(tabs.right() + 14.0, r.top() + 48.0), r.max - vec2(12.0, 12.0));
        text(p, full_body.left_top() + vec2(4.0, 2.0), Align2::LEFT_TOP, Tab::ALL[tab].name(), 15.0, ACCENT);
        let full_body = Rect::from_min_max(full_body.min + vec2(0.0, 30.0), full_body.max);
        // Barra de rolagem à direita (arrastar com o mouse ou o dedo).
        let bar = Rect::from_min_max(pos2(full_body.right() - 14.0, full_body.top()), full_body.max);
        let body = Rect::from_min_max(full_body.min, pos2(bar.left() - 8.0, full_body.bottom()));
        let row_h = 40.0;
        let visible = (body.height() / row_h) as usize;
        let center = sel_row.unwrap_or(0);
        let start = center.saturating_sub(visible / 2).min(rows.len().saturating_sub(visible));
        let bp = p.with_clip_rect(body);
        let mut clicked = None;
        for (k, i) in (start..rows.len().min(start + visible)).enumerate() {
            let rr = Rect::from_min_size(body.min + vec2(0.0, k as f32 * row_h), vec2(body.width(), row_h - 5.0));
            match &rows[i] {
                Row::Header(h) => {
                    text(&bp, rr.left_bottom() + vec2(4.0, -6.0), Align2::LEFT_BOTTOM, *h, 13.0, ACCENT);
                    bp.line_segment([rr.left_bottom(), rr.right_bottom()], Stroke::new(1.0_f32, LINE));
                }
                Row::Info(t) => {
                    text(&bp, rr.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, t, 13.0, DIM);
                }
                Row::Item { label, value, .. } => {
                    let sel = Some(i) == sel_row;
                    bp.rect_filled(rr, 5.0, if sel { ACCENT.gamma_multiply(0.25) } else { PANEL2 });
                    if sel {
                        bp.rect_stroke(rr, 5.0, Stroke::new(1.5_f32, ACCENT));
                    }
                    text(&bp, rr.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, label, 16.0, TEXT);
                    text(&bp, rr.right_center() - vec2(14.0, 0.0), Align2::RIGHT_CENTER, value, 14.0, if sel { ACCENT } else { DIM });
                    if ui.interact(rr, ui.id().with(("config", tab, i)), egui::Sense::click()).clicked() {
                        clicked = Some(i);
                    }
                }
            }
        }
        if rows.is_empty() {
            text(&bp, body.center_top() + vec2(0.0, 30.0), Align2::CENTER_TOP, "Nada para ajustar aqui.", 14.0, DIM);
        }
        let sel_list = selectable(&rows);
        let n = sel_list.len();
        // Roda do mouse: sobre as abas troca de aba; sobre a lista anda um item por "clique".
        let over_tabs = ui.rect_contains_pointer(tabs);
        let mut steps = 0;
        if over_tabs || ui.rect_contains_pointer(full_body) {
            let acc = self.wheel.get() - ui.input(|i| i.raw_scroll_delta.y);
            steps = (acc / 40.0).trunc() as i32;
            self.wheel.set(acc - steps as f32 * 40.0);
        }
        let mut jump = None;
        if rows.len() > visible && n > 1 {
            let pos = sel_list.iter().position(|&x| Some(x) == sel_row).unwrap_or(0);
            let frac = pos as f32 / (n - 1) as f32;
            let shown = visible as f32 / rows.len() as f32;
            let th = (bar.height() * shown).max(30.0);
            let thumb = Rect::from_min_size(pos2(bar.left(), bar.top() + (bar.height() - th) * frac), vec2(bar.width(), th));
            bp.rect_filled(bar, 7.0, PANEL2);
            bp.rect_filled(thumb.shrink(2.0), 6.0, ACCENT.gamma_multiply(0.7));
            let resp = ui.interact(bar, ui.id().with("config-barra"), egui::Sense::click_and_drag());
            if let Some(pt) = resp.interact_pointer_pos() {
                let f = ((pt.y - bar.top() - th / 2.0) / (bar.height() - th)).clamp(0.0, 1.0);
                jump = Some((f * (n - 1) as f32).round() as usize);
            }
        }
        let mut a = self.app.lock();
        if let Some(i) = tab_click {
            crate::settings::select_tab(&mut a, i);
        }
        if steps != 0 || jump.is_some() {
            if !over_tabs {
                crate::settings::focus_tabs(&mut a, false);
            } else {
                crate::settings::focus_tabs(&mut a, true);
            }
            match jump {
                Some(j) => {
                    if let Some(u) = a.settings_ui.as_mut() {
                        u.sel = j;
                    }
                }
                None => crate::settings::scroll(&mut a, steps),
            }
        }
        if let Some(i) = clicked {
            let pos = sel_list.iter().position(|&x| x == i).unwrap_or(0);
            if let Some(u) = a.settings_ui.as_mut() {
                u.on_tabs = false;
                u.sel = pos;
            }
            drop(a);
            crate::settings::activate(&self.app, &self.cmd);
        }
    }
}

impl eframe::App for DjOsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Músicas descartadas pelo motor são liberadas aqui, fora da thread de áudio.
        while self.trash.pop().is_ok() {}
        self.diagnostics(ctx);
        let snap = *self.snap.lock();
        self.persist(&snap);
        self.keyboard(ctx, &snap);

        egui::CentralPanel::default().frame(egui::Frame::none().fill(BG)).show(ctx, |ui| {
            let full = ui.max_rect();
            let p = ui.painter_at(full);
            let (bt_mode, bt_status) = {
                let a = self.app.lock();
                (a.bt_mode, a.bt_status.clone())
            };
            let (side, fx_pick, zoom_idx, big, tracks, loading, midi, audio, toast, sampler_names, scratch_names) = {
                let mut a = self.app.lock();
                a.lib.tick();
                let dt = self.history_at.elapsed().as_secs_f32();
                self.history_at = std::time::Instant::now();
                crate::history::tick(&mut a, &snap, dt);
                if self.resume_at.elapsed() > Duration::from_secs(2) {
                    self.resume_at = std::time::Instant::now();
                    crate::resume::save(&a, &snap, &mut self.resume_saved);
                }
                a.commit_fx_picks(&self.cmd);
                a.tick_browse_hold();
                if crate::navegador::is_on(&a) {
                    let st: &mut crate::app::AppState = &mut a;
                    st.nav.tick(&st.lib);
                }
                if a.toast.as_ref().is_some_and(|(_, t)| t.elapsed() > Duration::from_secs(3)) {
                    a.toast = None;
                }
                (
                    a.side,
                    a.fx_pick.map(|p| p.map(|(k, _)| k)),
                    a.wave_zoom,
                    a.big_browser,
                    a.decks.iter().map(|m| m.track.clone()).collect::<Vec<_>>(),
                    a.decks.iter().map(|m| m.loading.clone()).collect::<Vec<_>>(),
                    a.midi_status.clone(),
                    a.audio_status.clone(),
                    a.toast.as_ref().map(|(m, _)| m.clone()),
                    a.samples.iter().map(|t| t.as_ref().map(|t| t.title.clone())).collect::<Vec<_>>(),
                    a.scratch.iter().map(|t| t.as_ref().map(|t| t.title.clone())).collect::<Vec<_>>(),
                )
            };

            // Barra superior (escondida no browser novo: ele usa a tela inteira).
            let nav_on = big && crate::navegador::is_on(&self.app.lock());
            let top = Rect::from_min_size(full.min, vec2(full.width(), if nav_on { 0.0 } else { 34.0 }));
            if !nav_on {
                p.rect_filled(top, 0.0, PANEL);
                text(&p, top.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, "DJ OS", 18.0, ACCENT);
                mono(&p, top.center(), Align2::CENTER_CENTER, chrono::Local::now().format("%H:%M").to_string(), 18.0, TEXT);
                // Botão das configurações.
                let cfg = Rect::from_min_size(pos2(top.left() + 408.0, top.top() + 4.0), vec2(96.0, 26.0));
                p.rect_filled(cfg, 5.0, PANEL2);
                text(&p, cfg.center(), Align2::CENTER_CENTER, "CONFIG", 12.0, TEXT);
                if ui.interact(cfg, ui.id().with("config"), egui::Sense::click()).clicked() {
                    crate::settings::toggle(&mut self.app.lock());
                }
                // Botão de gravar o set: vermelho com o tempo enquanto grava.
                let recr = Rect::from_min_size(pos2(cfg.right() + 8.0, top.top() + 4.0), vec2(104.0, 26.0));
                let rec_since = self.app.lock().rec.active.as_ref().map(|(_, t, _)| t.elapsed().as_secs());
                match rec_since {
                    Some(s) => {
                        p.rect_filled(recr, 5.0, RED);
                        let blink = if (self.frame / 30) % 2 == 0 { "●" } else { " " };
                        mono(&p, recr.center(), Align2::CENTER_CENTER, format!("{blink} {:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60), 12.0, TEXT);
                    }
                    None => {
                        p.rect_filled(recr, 5.0, PANEL2);
                        p.circle_filled(recr.left_center() + vec2(18.0, 0.0), 5.0, RED);
                        text(&p, recr.center() + vec2(8.0, 0.0), Align2::CENTER_CENTER, "GRAVAR", 12.0, TEXT);
                    }
                }
                if ui.interact(recr, ui.id().with("gravar"), egui::Sense::click()).clicked() {
                    crate::record::toggle(&self.app, &self.cmd, snap.sample_rate);
                }
                // Botão do Bluetooth: tocar alterna desligado → master → retorno.
                let btr = Rect::from_min_size(pos2(top.left() + 150.0, top.top() + 4.0), vec2(250.0, 26.0));
                let on = bt_mode != crate::bluetooth::BtMode::Off;
                p.rect_filled(btr, 5.0, if on { Color32::from_rgb(40, 90, 200) } else { PANEL2 });
                let bp = p.with_clip_rect(btr);
                text(&bp, btr.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, &bt_status, 12.0, if on { TEXT } else { DIM });
                if ui.interact(btr, ui.id().with("bt"), egui::Sense::click()).clicked() {
                    let mut a = self.app.lock();
                    a.bt_mode = a.bt_mode.next();
                    a.settings.bt_mode = a.bt_mode;
                    a.settings.save();
                    a.bt_status = format!("BT {} · procurando…", a.bt_mode.name());
                }
                // Áudio parado: o motor ainda não rodou nenhum bloco depois de 3 s.
                let audio_dead = snap.sample_rate == 0.0 && self.frame > 180;
                // Na barra só o resumo; o texto completo fica em CONFIG → ÁUDIO.
                let audio = if audio.starts_with(crate::pw_out::NO_OUTPUT) || audio.starts_with("SEM ÁUDIO") || audio.contains("FONE SEM SOM") {
                    audio
                } else if audio.contains("DDJ-REV1") {
                    "Áudio: DDJ-REV1".to_string()
                } else if audio.is_empty() || audio.contains("abrindo") {
                    audio
                } else {
                    "Áudio: placa do computador".to_string()
                };
                let audio = if audio_dead { format!("ÁUDIO PARADO · {audio}") } else { audio };
                let midi_ok = midi.contains("conectada") && !midi.contains("des");
                let mr = text(&p, top.right_center() - vec2(150.0, 0.0), Align2::RIGHT_CENTER, &midi, 13.0, if midi_ok { ACCENT } else { ORANGE });
                // Estado do áudio entre o relógio e a controladora, cortado se não couber.
                let no_out = audio.starts_with(crate::pw_out::NO_OUTPUT);
                let ar = Rect::from_min_max(pos2(top.center().x + 44.0, top.top()), pos2(mr.left() - 16.0, top.bottom()));
                let col = if audio_dead { RED } else if no_out { ORANGE } else { DIM };
                text(&p.with_clip_rect(ar), pos2(ar.right(), top.center().y), Align2::RIGHT_CENTER, &audio, 12.0, col);
                // Limitador trabalhando (baixou mais de 0,3 dB): "LIM" por 1 s ao lado do medidor.
                if snap.master_limit > 0.0 && snap.master_limit < 0.966 {
                    self.lim_until = std::time::Instant::now() + Duration::from_secs(1);
                }

                for (i, v) in snap.master_peak.iter().enumerate() {
                    let m = Rect::from_min_size(pos2(top.right() - 140.0, top.top() + 9.0 + i as f32 * 9.0), vec2(126.0, 6.0));
                    p.rect_filled(m, 2.0, BG);
                    let lvl = v.clamp(0.0, 1.0);
                    let c = if lvl > 0.95 { RED } else if lvl > 0.7 { ORANGE } else { ACCENT };
                    p.rect_filled(Rect::from_min_size(m.min, vec2(m.width() * lvl, 6.0)), 2.0, c);
                }
                // "LIM" por cima dos medidores (sem disputar espaço com o texto da controladora).
                if std::time::Instant::now() < self.lim_until {
                    let lr = Rect::from_center_size(pos2(top.right() - 77.0, top.center().y), vec2(40.0, 18.0));
                    p.rect_filled(lr, 3.0, ORANGE);
                    text(&p, lr.center(), Align2::CENTER_CENTER, "LIM", 12.0, BG);
                }
            }

            let pad = 8.0;
            let gap = 6.0;
            let footer_h = 18.0;
            let body = Rect::from_min_max(pos2(full.left() + pad, top.bottom() + gap), pos2(full.right() - pad, full.bottom() - footer_h - 4.0));

            let (settings_open, search_open, palette, scale) = {
                let a = self.app.lock();
                (a.settings_ui.is_some(), a.search.is_some(), a.settings.palette, a.settings.scale)
            };
            PALETTE.store(palette, std::sync::atomic::Ordering::Relaxed);
            let is_light = {
                let a = self.app.lock();
                std::env::var("DJOS_MODO_LEVE").is_ok_and(|v| v == "1")
                    || match a.settings.light_mode {
                        1 => true,
                        2 => false,
                        _ => weak_gpu(&a.gpu),
                    }
            };
            LIGHT.store(is_light, std::sync::atomic::Ordering::Relaxed);
            let zoom = crate::settings::SCALES[scale.min(crate::settings::SCALES.len() - 1)];
            if (ctx.zoom_factor() - zoom).abs() > 0.001 {
                ctx.set_zoom_factor(zoom);
            }
            if settings_open {
                self.draw_settings(ui, &p, body);
                if search_open {
                    self.draw_search(ui, &p, body);
                }
            } else if !big {
                // Tela principal (src/ui/principal.rs): 2 ou 4 decks, estilo equipamento de cabine.
                let zoom = ZOOMS[zoom_idx.min(ZOOMS.len() - 1)];
                let _ = (&sampler_names, &scratch_names);
                self.draw_principal(ui, &p, body, &snap, side, &tracks, &loading, fx_pick, zoom);
            } else {
                // Browser novo (estilo cabine): em teste, até ser aprovado.
                if nav_on {
                    self.draw_navegador(ui, &p, body, &snap, &tracks);
                } else {
                    self.draw_library(ui, &p, body);
                }
                if self.app.lock().search.is_some() {
                    self.draw_search(ui, &p, body);
                }
            }

            let foot = Rect::from_min_max(pos2(full.left(), full.bottom() - footer_h), full.max);
            p.rect_filled(foot, 0.0, PANEL);
            text(
                &p,
                foot.left_center() + vec2(12.0, 0.0),
                Align2::LEFT_CENTER,
                if nav_on {
                    "Browser: setas andam · Enter abre · Backspace volta · Tab categorias/lista · 1-4 ou clicar no deck carrega · dois cliques carrega · Ctrl+F pesquisa · Esc ou B fecha"
                } else {
                    "Teclado: B browser · Ctrl+F pesquisa · 1-4 carregar · Z/M play · X/N cue · S/K sync · F1-F4 / F5-F8 efeito · Q/P keylock · F10 configurações · +/- zoom · setas esq/dir crossfader"
                },
                11.0,
                DIM,
            );
            if let Some(msg) = toast {
                let g = p.layout_no_wrap(msg, FontId::proportional(16.0), TEXT);
                let tr = Rect::from_center_size(pos2(full.center().x, full.bottom() - 60.0), g.size() + vec2(32.0, 18.0));
                p.rect_filled(tr, 8.0, PANEL2);
                p.rect_stroke(tr, 8.0, Stroke::new(1.0_f32, ACCENT));
                p.galley(tr.min + vec2(16.0, 9.0), g, TEXT);
            }
        });
        // Modo leve: no máximo 30 quadros por segundo (espera o resto dos 33 ms aqui mesmo, o que
        // funciona em qualquer sistema de janelas; o áudio roda em outra thread e não espera).
        if light() {
            let frame = Duration::from_millis(33);
            let el = self.last_frame.elapsed();
            if el < frame {
                std::thread::sleep(frame - el);
            }
        }
        self.last_frame = std::time::Instant::now();
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::weak_gpu;

    #[test]
    fn detecta_placa_fraca() {
        assert!(weak_gpu("Mesa Intel(R) HD Graphics 3000 (SNB GT2)"));
        assert!(weak_gpu("llvmpipe (LLVM 17.0.6, 256 bits)"));
        assert!(!weak_gpu("Mesa Intel(R) Iris(R) Xe Graphics (ADL GT2)"));
        assert!(!weak_gpu(""), "sem informação: modo normal");
    }
}
