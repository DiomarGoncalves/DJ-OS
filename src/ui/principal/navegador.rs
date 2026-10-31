//! Browser em tela cheia (SHIFT + BROWSE) no estilo dos aparelhos de cabine: categorias à
//! esquerda, músicas com a mini-waveform, BPM e tom no meio, e os decks embaixo.
//!
//! ```text
//! ┌────────┬───┬─────────────────────────────────────┬─────────┬───────┐
//! │ 💾 USB │ ← │ BROWSE  PASTAS › Funk               │ PREVIEW │ A A A │
//! ├────────┼───┴──────────┬──────────────────────────┴───┬─────┬─┴─────┤
//! │ ARTISTA│ ▁▃▇█▆▃▁▂▅██  │ ♪ Música                     │ BPM │  TOM  │
//! │ ÁLBUM  │ ▁▂▅▇▆▃▁▂▃▅▂  │ ▶ Música carregada num deck  │ 128 │  8A   │
//! │ …      │              │                              │     │       │
//! ├────────┴──────────────┴──────────────────────────────┴─────┴───────┤
//! │ deck 1 │ deck 2 │ deck 3 │ deck 4   (tempo, BPM, tom, música inteira) │
//! └──────────────────────────────────────────────────────────────────────┘
//! ```

use super::*;
use crate::app::AppState;
use crate::navegador::{self as nav, Area, Cat, Item, TopButton};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;

const K_SEL: Color32 = Color32::from_rgb(20, 110, 220);
/// Linha selecionada com o BROWSE em outra área (categorias ou barra de cima).
const K_SEL_OFF: Color32 = Color32::from_rgb(38, 58, 92);
const PREVIEW_COLS: usize = 96;
/// Análises lidas por quadro no máximo (girar rápido por milhares de músicas não trava a tela).
const LOADS_PER_FRAME: usize = 4;

/// O que a lista mostra de cada música vindo da análise guardada: a mini-waveform, o BPM e o tom.
#[derive(Clone)]
struct RowInfo {
    mini: Option<Arc<Vec<[u8; 3]>>>,
    bpm: Option<f64>,
    key: Option<u8>,
}

static INFOS: Mutex<Option<HashMap<PathBuf, RowInfo>>> = Mutex::new(None);

/// Lida uma vez só; `None` enquanto não chegou a vez dela (`budget` acabou neste quadro).
fn row_info(path: &PathBuf, budget: &mut usize) -> Option<RowInfo> {
    let mut cache = INFOS.lock();
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(m) = cache.get(path) {
        return Some(m.clone());
    }
    if *budget == 0 {
        return None;
    }
    *budget -= 1;
    let saved = crate::store::load(path);
    let mini = saved.as_ref().filter(|s| !s.low.is_empty()).map(|s| {
        let n = s.low.len();
        Arc::new(
            (0..PREVIEW_COLS)
                .map(|x| {
                    let (a, b) = (x * n / PREVIEW_COLS, ((x + 1) * n / PREVIEW_COLS).max(x * n / PREVIEW_COLS + 1).min(n));
                    let mut c = [0u8; 3];
                    for i in a..b {
                        c = [c[0].max(s.low[i]), c[1].max(s.mid[i]), c[2].max(s.high[i])];
                    }
                    c
                })
                .collect(),
        )
    });
    let info = RowInfo { mini, bpm: saved.as_ref().map(|s| s.bpm).filter(|b| *b > 0.0), key: saved.as_ref().and_then(|s| crate::key::from_raw(s.key)) };
    cache.insert(path.clone(), info.clone());
    Some(info)
}

/// Músicas analisadas agora (no LOAD ou pela análise de todas) passam a mostrar a análise: as
/// "sem análise" de um deck são lidas de novo, e tudo quando a análise de todas avança.
fn forget_new(loaded: &[PathBuf], analyzed: u64) {
    static SEEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut cache = INFOS.lock();
    let Some(c) = cache.as_mut() else { return };
    if SEEN.swap(analyzed, std::sync::atomic::Ordering::Relaxed) != analyzed {
        c.clear();
    }
    for p in loaded {
        if c.get(p).is_some_and(|i| i.mini.is_none()) {
            c.remove(p);
        }
    }
}

/// Tons que combinam na roda Camelot: o mesmo, ±1 com a mesma letra, ou o relativo.
pub fn keys_match(a: u8, b: u8) -> bool {
    let ((na, la), (nb, lb)) = (crate::key::camelot(a), crate::key::camelot(b));
    let step = (na as i32 - nb as i32).rem_euclid(12);
    (la == lb && matches!(step, 0 | 1 | 11)) || (na == nb)
}

/// O que o mouse fez neste quadro (aplicado depois de desenhar).
enum Click {
    Top(TopButton),
    Cat(Cat),
    Row(usize, bool),
    Deck(usize),
}

impl DjOsApp {
    /// Browser em tela cheia.
    pub(in crate::ui) fn draw_navegador(&mut self, ui: &mut egui::Ui, p: &Painter, body: Rect, snap: &Snapshot, tracks: &[Option<Arc<Track>>]) {
        p.rect_filled(body.expand(8.0), 0.0, K_BG);
        let four = self.app.lock().settings.four_decks;
        let gap = 4.0;
        let top_h = 44.0;
        let decks_h = 98.0;
        let cat_w = 104.0;
        let top = Rect::from_min_size(body.min, vec2(body.width(), top_h));
        let decks = Rect::from_min_max(pos2(body.left(), body.bottom() - decks_h), body.max);
        let mid = Rect::from_min_max(pos2(body.left(), top.bottom() + gap), pos2(body.right(), decks.top() - gap));
        let cats = Rect::from_min_size(mid.min, vec2(cat_w, mid.height()));
        let list = Rect::from_min_max(pos2(cats.right() + 2.0, mid.top()), mid.max);
        let mut clicks = Vec::new();
        let click = |ui: &mut egui::Ui, r: Rect, id: (&str, usize)| {
            let resp = ui.interact(r, ui.id().with(id), egui::Sense::click());
            (resp.clicked(), resp.double_clicked())
        };

        // Quem está tocando (para o ▶ e para o tom que combina).
        let loaded: Vec<PathBuf> = tracks.iter().flatten().map(|t| t.path.clone()).collect();
        let master_key = (0..tracks.len())
            .filter(|&d| snap.decks[d].playing && snap.decks[d].fader > 0.1)
            .find_map(|d| tracks[d].as_ref().and_then(|t| crate::key::from_raw(t.key)));

        let mut guard = self.app.lock();
        let font_i = guard.settings.browser_font.min(2);
        let head_h = 30.0;
        let rows_r = Rect::from_min_max(pos2(list.left(), list.top() + head_h), list.max);
        let row_h = [44.0f32, 52.0, 60.0][font_i];
        let font = [17.0f32, 22.0, 26.0][font_i];
        let rows = (rows_r.height() / row_h).floor().max(1.0) as usize;
        if let Some(l) = guard.nav.levels.last_mut() {
            l.scroll_into_view(rows);
        }
        let a: &AppState = &guard;
        forget_new(&loaded, a.lib.analyzed.load(std::sync::atomic::Ordering::Relaxed));
        let n = &a.nav;
        let s = &a.settings;
        let on_top = |b: TopButton| n.area == Area::Top && n.top == b;
        // Botão em foco na barra de cima: contorno branco.
        let focus_ring = |r: Rect, on: bool| {
            if on {
                p.rect_stroke(r.shrink(1.5), 2.0, Stroke::new(2.5_f32, K_TEXT));
            }
        };

        // ---- barra de cima: dispositivo, voltar, título, preview, tamanho da letra ----
        p.rect_filled(top, 0.0, K_PANEL);
        let dev = Rect::from_min_size(top.min, vec2(cat_w, top_h));
        p.rect_filled(dev, 0.0, K_PANEL2);
        tr(p, dev.center_top() + vec2(0.0, 4.0), Align2::CENTER_TOP, "💾", 16.0, K_TEXT);
        let dp = p.with_clip_rect(dev.shrink(4.0));
        tb(&dp, dev.center_bottom() - vec2(0.0, 4.0), Align2::CENTER_BOTTOM, n.device_name(), 11.0, K_TEXT);
        focus_ring(dev, on_top(TopButton::Device));
        let back = Rect::from_min_size(pos2(dev.right() + 10.0, top.top() + 6.0), vec2(36.0, top_h - 12.0));
        p.rect_stroke(back, 3.0, Stroke::new(1.0_f32, K_LINE));
        arrow_left(p, back.center(), 9.0, K_TEXT);
        focus_ring(back.expand(3.0), on_top(TopButton::Back));
        let size = Rect::from_min_size(pos2(top.right() - 110.0, top.top()), vec2(110.0, top_h));
        let prev = Rect::from_min_size(pos2(size.left() - 120.0, top.top()), vec2(116.0, top_h));
        let t = tb(p, pos2(back.right() + 14.0, top.center().y), Align2::LEFT_CENTER, "BROWSE", 20.0, K_TEXT);
        let crumb = p.with_clip_rect(Rect::from_min_max(pos2(t.right() + 14.0, top.top()), pos2(prev.left() - 8.0, top.bottom())));
        tr(&crumb, pos2(t.right() + 14.0, top.center().y + 1.0), Align2::LEFT_CENTER, n.breadcrumb(), 15.0, K_DIM);
        p.rect_filled(size, 0.0, K_PANEL2);
        for (i, sz) in [12.0f32, 16.0, 21.0].into_iter().enumerate() {
            let on = i == font_i;
            let c = pos2(size.left() + 24.0 + i as f32 * 31.0, top.center().y + 2.0);
            tb(p, c, Align2::CENTER_CENTER, "A", sz, if on { K_TEXT } else { K_DIM });
            if on {
                p.line_segment([c + vec2(-6.0, 12.0), c + vec2(6.0, 12.0)], Stroke::new(1.5_f32, K_TEXT));
            }
        }
        focus_ring(size, on_top(TopButton::Font));
        p.rect_filled(prev, 0.0, if s.browser_wave { K_BLUE } else { K_PANEL2 });
        tb(p, prev.center(), Align2::CENTER_CENTER, "PREVIEW", 15.0, if s.browser_wave { K_TEXT } else { K_DIM });
        focus_ring(prev, on_top(TopButton::Preview));
        for (r, b) in [(dev, TopButton::Device), (back, TopButton::Back), (prev, TopButton::Preview), (size, TopButton::Font)] {
            if click(ui, r, ("nav-top", b as usize)).0 {
                clicks.push(Click::Top(b));
            }
        }

        // ---- categorias ----
        let ch = cats.height() / Cat::ALL.len() as f32;
        for (i, c) in Cat::ALL.iter().enumerate() {
            let r = Rect::from_min_size(pos2(cats.left(), cats.top() + i as f32 * ch), vec2(cat_w, ch - 1.0));
            let on = *c == n.cat;
            let bg = match (on, n.area == Area::Cats) {
                (true, true) => K_SEL,
                (true, false) => Color32::from_rgb(70, 72, 80),
                _ => K_PANEL2,
            };
            p.rect_filled(r, 0.0, bg);
            let fg = if on { K_TEXT } else { Color32::from_rgb(190, 192, 198) };
            tr(p, r.center() - vec2(0.0, 8.0), Align2::CENTER_CENTER, c.icon(), 20.0, fg);
            tb(p, r.center_bottom() - vec2(0.0, 5.0), Align2::CENTER_BOTTOM, c.name(), 11.0, fg);
            if click(ui, r, ("nav-cat", i)).0 {
                clicks.push(Click::Cat(*c));
            }
        }

        // ---- lista: colunas conforme CONFIG → TELA ----
        let head = Rect::from_min_size(list.min, vec2(list.width(), head_h));
        p.rect_filled(head, 0.0, K_PANEL2);
        let wave_w = if s.browser_wave { 170.0 } else { 0.0 };
        let key_w = if s.browser_key { 86.0 } else { 0.0 };
        let bpm_w = if s.browser_bpm { 92.0 } else { 0.0 };
        let x_wave = list.left();
        let x_title = x_wave + wave_w + if s.browser_wave { 1.0 } else { 0.0 };
        let x_bpm = list.right() - key_w - bpm_w;
        let x_key = list.right() - key_w;
        if s.browser_wave {
            tr(p, pos2(x_wave + 10.0, head.center().y), Align2::LEFT_CENTER, "Preview", 13.0, K_DIM);
        }
        let tt = tr(p, pos2(x_title + 10.0, head.center().y), Align2::LEFT_CENTER, "Música", 13.0, K_BLUE);
        let tt = triangle_up(p, pos2(tt.right() + 9.0, head.center().y), 5.0, K_BLUE);
        tr(p, pos2(tt.right() + 12.0, head.center().y), Align2::LEFT_CENTER, format!("[{:04}]", n.track_count()), 13.0, K_DIM);
        if s.browser_bpm {
            tr(p, pos2(x_bpm + bpm_w - 10.0, head.center().y), Align2::RIGHT_CENTER, "BPM", 13.0, K_DIM);
        }
        if s.browser_key {
            tr(p, pos2(x_key + 10.0, head.center().y), Align2::LEFT_CENTER, "Tom", 13.0, K_DIM);
        }
        let mut cols = vec![];
        if s.browser_wave {
            cols.push(x_title - 1.0);
        }
        if s.browser_bpm {
            cols.push(x_bpm);
        }
        if s.browser_key {
            cols.push(x_key);
        }
        for x in cols {
            p.line_segment([pos2(x, list.top()), pos2(x, list.bottom())], Stroke::new(1.0_f32, K_LINE));
        }

        let empty = Vec::new();
        let (items, sel, start) = n.level().map_or((&empty, 0, 0), |l| (&l.items, l.sel, l.first));
        let count = items.len();
        let lp = p.with_clip_rect(rows_r);
        let pal = band_colors();
        let mut budget = LOADS_PER_FRAME;
        for (k, i) in (start..(start + rows).min(count)).enumerate() {
            let item = &items[i];
            let r = Rect::from_min_size(pos2(rows_r.left(), rows_r.top() + k as f32 * row_h), vec2(rows_r.width(), row_h));
            let is_sel = i == sel;
            let bg = match (is_sel, n.area == Area::List) {
                (true, true) => K_SEL,
                (true, false) => K_SEL_OFF,
                _ if k % 2 == 0 => Color32::from_rgb(24, 25, 28),
                _ => K_BG,
            };
            lp.rect_filled(Rect::from_min_max(pos2(x_title, r.top()), r.max), 0.0, bg);
            lp.line_segment([pos2(r.left(), r.bottom()), pos2(r.right(), r.bottom())], Stroke::new(1.0_f32, K_PANEL2));
            let (c, dc) = click(ui, r.intersect(rows_r), ("nav-row", i));
            if c || dc {
                clicks.push(Click::Row(i, dc));
            }
            let name_clip = lp.with_clip_rect(Rect::from_min_max(pos2(x_title, r.top()), pos2(x_bpm - 8.0, r.bottom())));
            let label = |icon: &str, name: &str, col: Color32| {
                tr(&lp, pos2(x_title + 12.0, r.center().y), Align2::LEFT_CENTER, icon, font * 0.8, col);
                tr(&name_clip, pos2(x_title + 38.0, r.center().y), Align2::LEFT_CENTER, name, font, col);
            };
            let e = match item {
                Item::Track(e) => e,
                Item::Back => {
                    arrow_left(&lp, pos2(x_title + 22.0, r.center().y), 8.0, K_DIM);
                    tr(&name_clip, pos2(x_title + 38.0, r.center().y), Align2::LEFT_CENTER, "Voltar", font, K_DIM);
                    continue;
                }
                Item::Device(d, name) => {
                    let current = *d == n.device;
                    label(if d.is_none() { "📚" } else { "💾" }, name.trim_start_matches(|c: char| !c.is_alphanumeric()), if current { K_GREEN } else { K_TEXT });
                    continue;
                }
                Item::Folder(f) => {
                    // Unidades e itens do rekordbox já vêm com o ícone no nome.
                    let (icon, name) = match f.name.split_once(' ') {
                        Some((i, rest)) if !i.chars().next().is_some_and(char::is_alphanumeric) => (i, rest),
                        _ => ("📁", f.name.as_str()),
                    };
                    label(icon, name, K_TEXT);
                    continue;
                }
                Item::Artist(name, t) => {
                    label("👤", &format!("{name}  ({})", t.len()), K_TEXT);
                    continue;
                }
                Item::Night(_, name) => {
                    label("🕘", name.trim_start_matches(|c: char| !c.is_alphanumeric()), K_TEXT);
                    continue;
                }
                Item::Info(msg) => {
                    tr(&name_clip, pos2(x_title + 16.0, r.center().y), Align2::LEFT_CENTER, msg, font * 0.8, K_DIM);
                    continue;
                }
            };
            let info = row_info(&e.path, &mut budget);
            // Mini-waveform da música inteira.
            if s.browser_wave {
                let wr = Rect::from_min_max(pos2(x_wave + 4.0, r.top() + 3.0), pos2(x_title - 5.0, r.bottom() - 3.0));
                lp.rect_filled(wr, 0.0, Color32::from_rgb(10, 10, 12));
                match info.as_ref().map(|i| i.mini.clone()) {
                    Some(Some(m)) => {
                        let cw = wr.width() / m.len() as f32;
                        let mut mesh = Mesh::default();
                        for (x, c) in m.iter().enumerate() {
                            let x0 = wr.left() + x as f32 * cw;
                            // Grave inteiro de baixo para cima; médio e agudo por cima, menores.
                            for (b, scale) in [(0usize, 1.0f32), (1, 0.45), (2, 0.22)] {
                                let h = (c[b] as f32 / 255.0).powf(0.8) * scale * wr.height();
                                if h > 0.5 {
                                    mesh.add_colored_rect(Rect::from_min_max(pos2(x0, wr.bottom() - h), pos2(x0 + cw, wr.bottom())), pal[b]);
                                }
                            }
                        }
                        lp.add(Shape::mesh(mesh));
                    }
                    Some(None) => {
                        tr(&lp, wr.center(), Align2::CENTER_CENTER, "sem análise", 11.0, K_DIM);
                    }
                    None => {}
                }
            }
            // Nome (▶ verde se está num deck; ✓ se já tocou).
            let is_loaded = loaded.contains(&e.path);
            let played = a.lib.played.contains(&e.path);
            let col = if is_loaded { K_GREEN } else if played { K_DIM } else { K_TEXT };
            let mark = if is_loaded { "▶" } else if played { "✓" } else { "♪" };
            tr(&lp, pos2(x_title + 12.0, r.center().y), Align2::LEFT_CENTER, mark, font * 0.8, if is_loaded { K_GREEN } else { K_DIM });
            let name = if e.artist.is_empty() { e.title.clone() } else { format!("{}  ·  {}", e.title, e.artist) };
            let name = match &e.when {
                Some(w) => format!("{w}   {name}"),
                None => name,
            };
            tr(&name_clip, pos2(x_title + 38.0, r.center().y), Align2::LEFT_CENTER, name, font, col);
            // BPM e tom (do rekordbox/lista ou da análise guardada).
            if s.browser_bpm {
                let bpm = e.bpm.or(info.as_ref().and_then(|i| i.bpm)).filter(|b| *b > 0.0).map_or(String::new(), |b| format!("{b:.1}"));
                tr(&lp, pos2(x_bpm + bpm_w - 10.0, r.center().y), Align2::RIGHT_CENTER, bpm, font, col);
            }
            if let Some(key) = e.key.or(info.as_ref().and_then(|i| i.key)).filter(|_| s.browser_key) {
                let kr = Rect::from_min_max(pos2(x_key + 1.0, r.top()), r.max);
                let good = master_key.is_some_and(|m| keys_match(m, key));
                if good {
                    lp.rect_filled(kr, 0.0, K_GREEN);
                }
                tr(&lp, pos2(x_key + 12.0, r.center().y), Align2::LEFT_CENTER, crate::key::label(key), font, if good { K_BG } else { col });
            }
        }
        if count == 0 {
            tr(&lp, rows_r.center(), Align2::CENTER_CENTER, "Nada aqui", 16.0, K_DIM);
        }
        // Barra de rolagem.
        if count > rows {
            let bar = Rect::from_min_max(pos2(list.right() - 4.0, rows_r.top()), rows_r.max);
            let h = (bar.height() * rows as f32 / count as f32).max(20.0);
            let y = bar.top() + (bar.height() - h) * start as f32 / (count - rows) as f32;
            p.rect_filled(Rect::from_min_size(pos2(bar.left(), y), vec2(4.0, h)), 2.0, K_DIM);
        }
        // Roda do mouse: sobre a lista anda uma música por "clique"; sobre as categorias, troca.
        let mut wheel_steps = 0;
        let over_cats = ui.rect_contains_pointer(cats);
        if over_cats || ui.rect_contains_pointer(rows_r) {
            let acc = self.wheel.get() - ui.input(|i| i.raw_scroll_delta.y);
            wheel_steps = (acc / 40.0).trunc() as i32;
            self.wheel.set(acc - wheel_steps as f32 * 40.0);
        }
        drop(guard);

        // ---- decks embaixo (clicar carrega a música selecionada) ----
        let shown: Vec<usize> = if four { vec![0, 1, 2, 3] } else { vec![0, 1] };
        let w = (decks.width() - gap * (shown.len() as f32 - 1.0)) / shown.len() as f32;
        for (k, &d) in shown.iter().enumerate() {
            let r = Rect::from_min_size(pos2(decks.left() + k as f32 * (w + gap), decks.top()), vec2(w, decks_h));
            self.draw_mini_deck(p, r, d, tracks[d].clone(), &snap.decks[d]);
            if click(ui, r, ("nav-deck", d)).0 {
                clicks.push(Click::Deck(d));
            }
        }

        // ---- o que o mouse fez ----
        let mut a = self.app.lock();
        if wheel_steps != 0 {
            if over_cats {
                a.nav.area = Area::Cats;
            } else if a.nav.area != Area::List {
                a.nav.area = Area::List;
            }
            // Na lista a roda não sobe para a barra de cima (só o BROWSE e as setas).
            let st: &mut AppState = &mut a;
            if st.nav.area == Area::List && st.nav.level().is_some_and(|l| l.sel as i32 + wheel_steps < 0) {
                st.nav.select(0);
            } else {
                st.nav.scroll(&st.lib, wheel_steps);
            }
        }
        for c in clicks {
            match c {
                Click::Top(b) => nav::activate(&mut a, b),
                Click::Cat(c) => {
                    let st: &mut AppState = &mut a;
                    st.nav.set_cat(&st.lib, c);
                    st.nav.area = Area::Cats;
                }
                Click::Row(i, double) => {
                    a.nav.select(i);
                    if double {
                        match a.nav.selected_track() {
                            Some(t) => {
                                // Dois cliques numa música: carrega no primeiro deck parado.
                                let order = [a.side[0], a.side[1], 0, 1, 2, 3];
                                match order.into_iter().filter(|&d| four || d < 2).find(|&d| !snap.decks[d].playing) {
                                    Some(d) => {
                                        self.loader.load_deck(&mut a, d, t.path);
                                        a.big_browser = false;
                                    }
                                    None => a.toast("Todos os decks tocando: clique no deck de baixo ou use o LOAD"),
                                }
                            }
                            None => {
                                let st: &mut AppState = &mut a;
                                st.nav.open(&st.lib);
                            }
                        }
                    }
                }
                Click::Deck(d) => match a.nav.selected_track() {
                    Some(_) if snap.decks[d].playing => a.toast(format!("Deck {} tocando: pare ele ou use o LOAD", d + 1)),
                    Some(t) => {
                        self.loader.load_deck(&mut a, d, t.path);
                        a.big_browser = false;
                    }
                    None => {}
                },
            }
        }
    }

    /// Deck em miniatura embaixo do browser: música, tom, tempo restante, BPM e a música inteira
    /// com os hot cues.
    fn draw_mini_deck(&mut self, p: &Painter, r: Rect, d: usize, t: Option<Arc<Track>>, s: &DeckSnap) {
        p.rect_filled(r, 0.0, K_PANEL);
        let head = Rect::from_min_size(r.min, vec2(r.width(), 22.0));
        p.rect_filled(head, 0.0, K_PANEL2);
        let num = Rect::from_min_size(head.min, vec2(22.0, 22.0));
        p.rect_filled(num, 0.0, if on_air(s) { deck_tint(d) } else { K_LINE });
        tb(p, num.center(), Align2::CENTER_CENTER, d + 1, 14.0, if on_air(s) { K_BG } else { K_TEXT });
        let mut right = head.right() - 6.0;
        if let Some(k) = t.as_ref().and_then(|t| crate::key::from_raw(t.key)) {
            right = tb(p, pos2(right, head.center().y), Align2::RIGHT_CENTER, crate::key::label(k), 13.0, K_TEXT).left() - 8.0;
        }
        let hp = p.with_clip_rect(Rect::from_min_max(pos2(num.right(), head.top()), pos2(right, head.bottom())));
        let title = t.as_ref().map_or("Sem música".to_string(), |t| format!("♪ {}", t.title));
        tr(&hp, pos2(num.right() + 8.0, head.center().y), Align2::LEFT_CENTER, title, 13.0, if t.is_some() { K_TEXT } else { K_DIM });

        let y = head.bottom() + 4.0;
        tr(p, pos2(r.left() + 8.0, y), Align2::LEFT_TOP, "RESTANTE", 9.0, K_DIM);
        let time = if t.is_some() { remain_text(s) } else { "--:--.-".into() };
        tb(p, pos2(r.left() + 8.0, y + 9.0), Align2::LEFT_TOP, time, 24.0, K_TEXT);
        tr(p, pos2(r.right() - 70.0, y), Align2::RIGHT_TOP, "BPM", 9.0, K_DIM);
        let bpm = if s.loaded && s.bpm > 0.0 { format!("{:.1}", s.bpm) } else { "--.-".into() };
        tb(p, pos2(r.right() - 8.0, y + 9.0), Align2::RIGHT_TOP, bpm, 24.0, if s.sync_ok && s.loaded { K_GREEN } else { K_TEXT });

        // Música inteira, com o que já tocou mais escuro, hot cues e posição.
        let ov = Rect::from_min_max(pos2(r.left() + 8.0, r.bottom() - 26.0), pos2(r.right() - 8.0, r.bottom() - 6.0));
        p.rect_filled(ov, 0.0, K_BG);
        let Some(t) = t else { return };
        let width = ov.width().max(1.0) as usize;
        let pal = band_colors();
        let played = if s.frames > 0.0 { (s.pos / s.frames).clamp(0.0, 1.0) as f32 * width as f32 } else { 0.0 };
        let o = self.overview_for(d, &t, width);
        let mut mesh = Mesh::default();
        for (x, c) in o.cols.iter().enumerate() {
            let fx = ov.left() + x as f32;
            let dim = if (x as f32) < played { 0.35 } else { 1.0 };
            for (b, scale) in [(0usize, 1.0f32), (1, 0.46), (2, 0.28)] {
                let a = (c[b] as f32 / 255.0).powf(0.75) * scale * ov.height();
                if a > 0.5 {
                    mesh.add_colored_rect(Rect::from_min_max(pos2(fx, ov.bottom() - a), pos2(fx + 1.0, ov.bottom())), pal[b].gamma_multiply(dim));
                }
            }
        }
        p.add(Shape::mesh(mesh));
        let x_of = |pos: f64| ov.left() + (pos / s.frames.max(1.0)) as f32 * ov.width();
        for (i, h) in s.hotcues.iter().enumerate() {
            if let Some(hp) = h {
                let hx = x_of(*hp);
                let lab = Rect::from_center_size(pos2(hx, ov.top() - 4.0), vec2(11.0, 11.0));
                p.rect_filled(lab, 1.0, CUE_COLORS[i]);
                tb(p, lab.center(), Align2::CENTER_CENTER, CUE_LETTERS[i], 9.0, K_BG);
            }
        }
        let nx = x_of(s.pos);
        p.line_segment([pos2(nx, ov.top() - 2.0), pos2(nx, ov.bottom() + 2.0)], Stroke::new(2.0_f32, K_TEXT));
    }
}

/// Seta para a esquerda desenhada (a fonte não tem o símbolo).
fn arrow_left(p: &Painter, c: egui::Pos2, s: f32, col: Color32) {
    let st = Stroke::new(2.0_f32, col);
    p.line_segment([c + vec2(-s, 0.0), c + vec2(s, 0.0)], st);
    p.line_segment([c + vec2(-s, 0.0), c + vec2(-s * 0.4, -s * 0.6)], st);
    p.line_segment([c + vec2(-s, 0.0), c + vec2(-s * 0.4, s * 0.6)], st);
}

/// Triângulo para cima (ordem da coluna).
fn triangle_up(p: &Painter, c: egui::Pos2, s: f32, col: Color32) -> Rect {
    p.add(Shape::convex_polygon(vec![c + vec2(0.0, -s), c + vec2(s, s * 0.7), c + vec2(-s, s * 0.7)], col, Stroke::NONE));
    Rect::from_center_size(c, vec2(2.0 * s, 2.0 * s))
}
