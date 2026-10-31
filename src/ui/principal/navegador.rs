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
use crate::library::{Focus, TrackEntry};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;

/// Categorias da coluna da esquerda (ícone, nome).
pub const CATEGORIES: [(&str, &str); 10] = [
    ("👤", "ARTISTA"),
    ("💿", "ÁLBUM"),
    ("♪", "MÚSICAS"),
    ("b#", "TOM"),
    ("☰", "PLAYLIST"),
    ("🕘", "HISTÓRICO"),
    ("🔗", "COMBINAM"),
    ("📁", "PASTAS"),
    ("⏭", "FILA"),
    ("⏺", "GRAVAÇÕES"),
];

const K_SEL: Color32 = Color32::from_rgb(20, 110, 220);
const PREVIEW_COLS: usize = 96;

/// Mini-waveform de cada música (da análise guardada), lida uma vez só.
type Mini = Option<Arc<Vec<[u8; 3]>>>;
static MINIS: Mutex<Option<HashMap<PathBuf, Mini>>> = Mutex::new(None);

fn mini_wave(path: &PathBuf) -> Mini {
    let mut cache = MINIS.lock();
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(m) = cache.get(path) {
        return m.clone();
    }
    let m = crate::store::load(path).filter(|s| !s.low.is_empty()).map(|s| {
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
    cache.insert(path.clone(), m.clone());
    m
}

/// Tons que combinam na roda Camelot: o mesmo, ±1 com a mesma letra, ou o relativo.
pub fn keys_match(a: u8, b: u8) -> bool {
    let ((na, la), (nb, lb)) = (crate::key::camelot(a), crate::key::camelot(b));
    let step = (na as i32 - nb as i32).rem_euclid(12);
    (la == lb && matches!(step, 0 | 1 | 11)) || (na == nb)
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

        // Quem está tocando (para o ▶ e para o tom que combina).
        let loaded: Vec<PathBuf> = tracks.iter().flatten().map(|t| t.path.clone()).collect();
        let master_key = (0..tracks.len())
            .filter(|&d| snap.decks[d].playing && snap.decks[d].fader > 0.1)
            .find_map(|d| tracks[d].as_ref().and_then(|t| crate::key::from_raw(t.key)));

        let a = self.app.lock();
        let lib = &a.lib;
        let scale = a.settings.scale;
        let folder = lib.folders.get(lib.folder_sel).map(|n| n.name.clone()).unwrap_or_default();
        let device = lib
            .folders
            .get(lib.folder_sel)
            .and_then(|n| lib.folders[..=lib.folder_sel].iter().rev().find(|f| f.depth == 0 && f.path.as_os_str().len() > 0 && n.path.starts_with(&f.path)))
            .map_or("TODOS".to_string(), |f| f.name.trim_start_matches(|c: char| !c.is_alphanumeric()).to_uppercase());

        // ---- barra de cima: dispositivo, voltar, título, preview, tamanho da letra ----
        p.rect_filled(top, 0.0, K_PANEL);
        let dev = Rect::from_min_size(top.min, vec2(cat_w, top_h));
        p.rect_filled(dev, 0.0, K_PANEL2);
        tr(p, dev.center_top() + vec2(0.0, 4.0), Align2::CENTER_TOP, "💾", 16.0, K_TEXT);
        let dp = p.with_clip_rect(dev.shrink(4.0));
        tb(&dp, dev.center_bottom() - vec2(0.0, 4.0), Align2::CENTER_BOTTOM, &device, 11.0, K_TEXT);
        let back = Rect::from_min_size(pos2(dev.right() + 10.0, top.top() + 6.0), vec2(36.0, top_h - 12.0));
        p.rect_stroke(back, 3.0, Stroke::new(1.0_f32, K_LINE));
        arrow_left(p, back.center(), 9.0, K_TEXT);
        let t = tb(p, pos2(back.right() + 14.0, top.center().y), Align2::LEFT_CENTER, "BROWSE", 20.0, K_TEXT);
        tr(p, pos2(t.right() + 14.0, top.center().y + 1.0), Align2::LEFT_CENTER, format!("PASTAS  ›  {}", folder.trim_start_matches(|c: char| !c.is_alphanumeric())), 15.0, K_DIM);
        let size = Rect::from_min_size(pos2(top.right() - 110.0, top.top()), vec2(110.0, top_h));
        p.rect_filled(size, 0.0, K_PANEL2);
        for (i, s) in [12.0f32, 16.0, 21.0].into_iter().enumerate() {
            let on = i == scale.min(2);
            let c = pos2(size.left() + 24.0 + i as f32 * 31.0, top.center().y + 2.0);
            tb(p, c, Align2::CENTER_CENTER, "A", s, if on { K_TEXT } else { K_DIM });
            if on {
                p.line_segment([c + vec2(-6.0, 12.0), c + vec2(6.0, 12.0)], Stroke::new(1.5_f32, K_TEXT));
            }
        }
        let prev = Rect::from_min_size(pos2(size.left() - 120.0, top.top()), vec2(116.0, top_h));
        p.rect_filled(prev, 0.0, K_BLUE);
        tb(p, prev.center(), Align2::CENTER_CENTER, "PREVIEW", 15.0, K_TEXT);

        // ---- categorias ----
        let ch = cats.height() / CATEGORIES.len() as f32;
        let current = CATEGORIES.iter().position(|c| c.1 == "PASTAS").unwrap_or(0);
        for (i, (icon, name)) in CATEGORIES.iter().enumerate() {
            let r = Rect::from_min_size(pos2(cats.left(), cats.top() + i as f32 * ch), vec2(cat_w, ch - 1.0));
            let on = i == current;
            p.rect_filled(r, 0.0, if on { Color32::from_rgb(70, 72, 80) } else { K_PANEL2 });
            let fg = if on { K_TEXT } else { Color32::from_rgb(190, 192, 198) };
            tr(p, r.center() - vec2(0.0, 8.0), Align2::CENTER_CENTER, icon, 20.0, fg);
            tb(p, r.center_bottom() - vec2(0.0, 5.0), Align2::CENTER_BOTTOM, name, 11.0, fg);
        }

        // ---- lista ----
        let head_h = 30.0;
        let head = Rect::from_min_size(list.min, vec2(list.width(), head_h));
        p.rect_filled(head, 0.0, K_PANEL2);
        let wave_w = 170.0;
        let key_w = 86.0;
        let bpm_w = 92.0;
        let x_wave = list.left();
        let x_title = x_wave + wave_w + 1.0;
        let x_bpm = list.right() - key_w - bpm_w;
        let x_key = list.right() - key_w;
        tr(p, pos2(x_wave + 10.0, head.center().y), Align2::LEFT_CENTER, "Preview", 13.0, K_DIM);
        let tt = tr(p, pos2(x_title + 10.0, head.center().y), Align2::LEFT_CENTER, "Música", 13.0, K_BLUE);
        let tt = triangle_up(p, pos2(tt.right() + 9.0, head.center().y), 5.0, K_BLUE);
        tr(p, pos2(tt.right() + 12.0, head.center().y), Align2::LEFT_CENTER, format!("[{:04}]", lib.tracks.len()), 13.0, K_DIM);
        tr(p, pos2(x_bpm + bpm_w - 10.0, head.center().y), Align2::RIGHT_CENTER, "BPM", 13.0, K_DIM);
        tr(p, pos2(x_key + 10.0, head.center().y), Align2::LEFT_CENTER, "Tom", 13.0, K_DIM);
        for x in [x_title - 1.0, x_bpm, x_key] {
            p.line_segment([pos2(x, list.top()), pos2(x, list.bottom())], Stroke::new(1.0_f32, K_LINE));
        }

        let rows_r = Rect::from_min_max(pos2(list.left(), head.bottom()), list.max);
        let row_h = [44.0f32, 52.0, 60.0][scale.min(2)];
        let font = [17.0f32, 22.0, 26.0][scale.min(2)];
        let rows = (rows_r.height() / row_h).floor().max(1.0) as usize;
        let n = lib.tracks.len();
        let start = if n <= rows { 0 } else { lib.track_sel.saturating_sub(rows / 2).min(n - rows) };
        let lp = p.with_clip_rect(rows_r);
        let pal = band_colors();
        let focused = lib.focus != Focus::Folders;
        for (k, i) in (start..(start + rows).min(n)).enumerate() {
            let e: &TrackEntry = &lib.tracks[i];
            let r = Rect::from_min_size(pos2(rows_r.left(), rows_r.top() + k as f32 * row_h), vec2(rows_r.width(), row_h));
            let sel = i == lib.track_sel;
            let bg = if sel && focused { K_SEL } else if k % 2 == 0 { Color32::from_rgb(24, 25, 28) } else { K_BG };
            lp.rect_filled(Rect::from_min_max(pos2(x_title, r.top()), r.max), 0.0, bg);
            lp.line_segment([pos2(r.left(), r.bottom()), pos2(r.right(), r.bottom())], Stroke::new(1.0_f32, K_PANEL2));
            // Mini-waveform da música inteira.
            let wr = Rect::from_min_max(pos2(x_wave + 4.0, r.top() + 3.0), pos2(x_title - 5.0, r.bottom() - 3.0));
            lp.rect_filled(wr, 0.0, Color32::from_rgb(10, 10, 12));
            if let Some(m) = mini_wave(&e.path) {
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
            } else {
                tr(&lp, wr.center(), Align2::CENTER_CENTER, "sem análise", 11.0, K_DIM);
            }
            // Nome (▶ verde se está num deck; ✓ se já tocou).
            let is_loaded = loaded.contains(&e.path);
            let played = lib.played.contains(&e.path);
            let col = if is_loaded { K_GREEN } else if played { K_DIM } else { K_TEXT };
            let mark = if is_loaded { "▶" } else if played { "✓" } else { "♪" };
            tr(&lp, pos2(x_title + 12.0, r.center().y), Align2::LEFT_CENTER, mark, font * 0.8, if is_loaded { K_GREEN } else { K_DIM });
            let np = lp.with_clip_rect(Rect::from_min_max(pos2(x_title, r.top()), pos2(x_bpm - 8.0, r.bottom())));
            let name = if e.artist.is_empty() { e.title.clone() } else { format!("{}  ·  {}", e.title, e.artist) };
            tr(&np, pos2(x_title + 38.0, r.center().y), Align2::LEFT_CENTER, name, font, col);
            // BPM e tom.
            let bpm = e.bpm.filter(|b| *b > 0.0).map_or(String::new(), |b| format!("{b:.1}"));
            tr(&lp, pos2(x_bpm + bpm_w - 10.0, r.center().y), Align2::RIGHT_CENTER, bpm, font, col);
            if let Some(key) = e.key {
                let kr = Rect::from_min_max(pos2(x_key + 1.0, r.top()), r.max);
                let good = master_key.is_some_and(|m| keys_match(m, key));
                if good {
                    lp.rect_filled(kr, 0.0, K_GREEN);
                }
                tr(&lp, pos2(x_key + 12.0, r.center().y), Align2::LEFT_CENTER, crate::key::label(key), font, if good { K_BG } else { col });
            }
        }
        if n == 0 {
            tr(&lp, rows_r.center(), Align2::CENTER_CENTER, "Nenhuma música aqui", 16.0, K_DIM);
        }
        // Barra de rolagem.
        if n > rows {
            let bar = Rect::from_min_max(pos2(list.right() - 4.0, rows_r.top()), rows_r.max);
            let h = (bar.height() * rows as f32 / n as f32).max(20.0);
            let y = bar.top() + (bar.height() - h) * start as f32 / (n - rows) as f32;
            p.rect_filled(Rect::from_min_size(pos2(bar.left(), y), vec2(4.0, h)), 2.0, K_DIM);
        }
        drop(a);

        // ---- decks embaixo ----
        let shown: Vec<usize> = if four { vec![0, 1, 2, 3] } else { vec![0, 1] };
        let w = (decks.width() - gap * (shown.len() as f32 - 1.0)) / shown.len() as f32;
        for (k, &d) in shown.iter().enumerate() {
            let r = Rect::from_min_size(pos2(decks.left() + k as f32 * (w + gap), decks.top()), vec2(w, decks_h));
            self.draw_mini_deck(p, r, d, tracks[d].clone(), &snap.decks[d]);
        }
        let _ = ui;
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
