//! Browser em tela cheia (SHIFT + BROWSE) no estilo dos aparelhos de cabine: a mesma lógica do
//! browser de árvore (pastas à esquerda, músicas da pasta à direita, BROWSE alterna entre os dois),
//! com o fundo preto e os decks embaixo.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────────┐
//! │ BROWSE  PASTAS › NOVO › Funk                                         │
//! ├───────────────────┬──────────────────────────────────────┬─────┬─────┤
//! │ 💾 NOVO           │ ♪ Música                             │ BPM │ TOM │
//! │   📁 Funk         │ ▶ Música carregada num deck          │ 128 │ 8A  │
//! │     📁 Antigas    │                                      │     │     │
//! ├───────────────────┴──────────────────────────────────────┴─────┴─────┤
//! │ deck 1 │ deck 2 │ deck 3 │ deck 4   (tempo, BPM, tom, música inteira) │
//! └──────────────────────────────────────────────────────────────────────┘
//! ```

use super::*;
use crate::library::Focus;
use parking_lot::Mutex;
use std::path::PathBuf;

fn k_sel() -> Color32 {
    super::super::skin::atual().selecao
}
/// Seleção do lado que não está com o BROWSE (pastas ou músicas).
fn k_sel_off() -> Color32 {
    super::super::skin::atual().selecao_fraca
}

/// Primeira linha que aparece em cada lado (pastas, músicas): a lista só rola quando a seleção
/// sai da tela, para o clique do mouse não mudar de linha.
static FIRST: Mutex<(usize, usize)> = Mutex::new((0, 0));

fn into_view(first: &mut usize, sel: usize, rows: usize, n: usize) {
    if sel < *first {
        *first = sel;
    } else if sel >= *first + rows {
        *first = sel + 1 - rows;
    }
    *first = (*first).min(n.saturating_sub(rows));
}

/// Tons que combinam na roda Camelot: o mesmo, ±1 com a mesma letra, ou o relativo.
pub fn keys_match(a: u8, b: u8) -> bool {
    let ((na, la), (nb, lb)) = (crate::key::camelot(a), crate::key::camelot(b));
    let step = (na as i32 - nb as i32).rem_euclid(12);
    (la == lb && matches!(step, 0 | 1 | 11)) || (na == nb)
}

/// O que o mouse fez neste quadro (aplicado depois de desenhar).
enum Click {
    Folder(usize, bool),
    Track(usize, bool),
    Queue(usize, bool),
    Deck(usize),
}

impl DjOsApp {
    /// Browser em tela cheia.
    pub(in crate::ui) fn draw_navegador(&mut self, ui: &mut egui::Ui, p: &Painter, body: Rect, snap: &Snapshot, tracks: &[Option<Arc<Track>>]) {
        p.rect_filled(body.expand(8.0), 0.0, k_bg());
        let four = self.app.lock().settings.four_decks;
        let gap = 4.0;
        let top_h = 44.0;
        let decks_h = 98.0;
        let head_h = 30.0;
        let top = Rect::from_min_size(body.min, vec2(body.width(), top_h));
        let decks = Rect::from_min_max(pos2(body.left(), body.bottom() - decks_h), body.max);
        let mid = Rect::from_min_max(pos2(body.left(), top.bottom() + gap), pos2(body.right(), decks.top() - gap));
        let tree_w = (mid.width() * 0.3).clamp(260.0, 420.0);
        let tree = Rect::from_min_size(mid.min, vec2(tree_w, mid.height()));
        // Fila (SHIFT + LOAD) à direita, só quando tem música nela, como no browser de árvore.
        let (has_queue, queue_focus) = {
            let a = self.app.lock();
            (!a.lib.prepare.is_empty(), a.lib.focus == Focus::Prepare)
        };
        let queue_w = if has_queue || queue_focus { (mid.width() * 0.22).clamp(220.0, 360.0) } else { 0.0 };
        let list = Rect::from_min_max(pos2(tree.right() + 2.0, mid.top()), pos2(mid.right() - if queue_w > 0.0 { queue_w + 2.0 } else { 0.0 }, mid.bottom()));
        let queue = Rect::from_min_max(pos2(list.right() + 2.0, mid.top()), mid.max);
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

        let a = self.app.lock();
        let lib = &a.lib;
        let s = &a.settings;
        let font_i = s.browser_font.min(2);
        let row_h = [44.0f32, 52.0, 60.0][font_i];
        let font = [17.0f32, 22.0, 26.0][font_i];
        let on_folders = lib.focus == Focus::Folders;
        let on_tracks = lib.focus == Focus::Tracks;

        // ---- barra de cima: BROWSE e a pasta aberta (com as pastas de cima) ----
        p.rect_filled(top, 0.0, k_panel());
        let t = tb(p, pos2(top.left() + 14.0, top.center().y), Align2::LEFT_CENTER, "BROWSE", 20.0, k_text());
        let mut path = Vec::new();
        if let Some(n) = lib.folders.get(lib.folder_sel) {
            let mut depth = n.depth;
            path.push(n.name.as_str());
            for f in lib.folders[..lib.folder_sel].iter().rev() {
                if f.depth < depth {
                    depth = f.depth;
                    path.push(f.name.as_str());
                }
            }
        }
        let crumb = std::iter::once("PASTAS".to_string()).chain(path.iter().rev().map(|n| plain(n).to_string())).collect::<Vec<_>>().join("  ›  ");
        let cp = p.with_clip_rect(Rect::from_min_max(pos2(t.right() + 14.0, top.top()), pos2(top.right() - 8.0, top.bottom())));
        tr(&cp, pos2(t.right() + 14.0, top.center().y + 1.0), Align2::LEFT_CENTER, crumb, 15.0, k_dim());

        // ---- pastas (árvore) ----
        let th = Rect::from_min_size(tree.min, vec2(tree.width(), head_h));
        p.rect_filled(th, 0.0, k_panel2());
        tr(p, pos2(th.left() + 10.0, th.center().y), Align2::LEFT_CENTER, "Pastas", 13.0, if on_folders { k_blue() } else { k_dim() });
        p.line_segment([pos2(tree.right() + 1.0, tree.top()), pos2(tree.right() + 1.0, tree.bottom())], Stroke::new(1.0_f32, k_line()));
        let tree_rows = Rect::from_min_max(pos2(tree.left(), th.bottom()), tree.max);
        let rows = (tree_rows.height() / row_h).floor().max(1.0) as usize;
        let mut first = FIRST.lock();
        into_view(&mut first.0, lib.folder_sel, rows, lib.folders.len());
        let tp = p.with_clip_rect(tree_rows);
        for (k, i) in (first.0..(first.0 + rows).min(lib.folders.len())).enumerate() {
            let n = &lib.folders[i];
            let r = Rect::from_min_size(pos2(tree_rows.left(), tree_rows.top() + k as f32 * row_h), vec2(tree_rows.width(), row_h));
            let bg = match (i == lib.folder_sel, on_folders) {
                (true, true) => k_sel(),
                (true, false) => k_sel_off(),
                _ if k % 2 == 0 => Color32::from_rgb(24, 25, 28),
                _ => k_bg(),
            };
            tp.rect_filled(r, 0.0, bg);
            let x = r.left() + 10.0 + n.depth as f32 * 18.0;
            // Seta de abrir/fechar (só se tiver subpastas).
            if n.has_children {
                let c = pos2(x + 5.0, r.center().y);
                let tri = if n.expanded {
                    vec![c + vec2(-5.0, -3.0), c + vec2(5.0, -3.0), c + vec2(0.0, 4.0)]
                } else {
                    vec![c + vec2(-3.0, -5.0), c + vec2(4.0, 0.0), c + vec2(-3.0, 5.0)]
                };
                tp.add(Shape::convex_polygon(tri, k_dim(), Stroke::NONE));
            }
            // Unidades, pesquisa e tocadas hoje já vêm com o ícone no nome (o microfone das
            // gravações a fonte não tem: vai o de gravar).
            let (icon, name) = match n.name.split_once(' ') {
                Some((ic, rest)) if !ic.chars().next().is_some_and(char::is_alphanumeric) => (if ic == "🎙" { "⏺" } else { ic }, rest),
                _ => ("📁", n.name.as_str()),
            };
            tr(&tp, pos2(x + 16.0, r.center().y), Align2::LEFT_CENTER, icon, font * 0.8, k_dim());
            tr(&tp, pos2(x + 40.0, r.center().y), Align2::LEFT_CENTER, name, font * 0.9, k_text());
            let (c, dc) = click(ui, r.intersect(tree_rows), ("nav-pasta", i));
            if c || dc {
                clicks.push(Click::Folder(i, dc));
            }
        }
        if lib.folders.len() > rows {
            scrollbar(p, tree_rows, first.0, rows, lib.folders.len());
        }

        // ---- músicas da pasta ----
        let head = Rect::from_min_size(list.min, vec2(list.width(), head_h));
        p.rect_filled(head, 0.0, k_panel2());
        let key_w = if s.browser_key { 86.0 } else { 0.0 };
        let bpm_w = if s.browser_bpm { 92.0 } else { 0.0 };
        let x_title = list.left();
        let x_bpm = list.right() - key_w - bpm_w;
        let x_key = list.right() - key_w;
        let tt = tr(p, pos2(x_title + 10.0, head.center().y), Align2::LEFT_CENTER, "Música", 13.0, if on_tracks { k_blue() } else { k_dim() });
        tr(p, pos2(tt.right() + 10.0, head.center().y), Align2::LEFT_CENTER, format!("[{:04}]", lib.tracks.len()), 13.0, k_dim());
        if s.browser_bpm {
            tr(p, pos2(x_bpm + bpm_w - 10.0, head.center().y), Align2::RIGHT_CENTER, "BPM", 13.0, k_dim());
            p.line_segment([pos2(x_bpm, list.top()), pos2(x_bpm, list.bottom())], Stroke::new(1.0_f32, k_line()));
        }
        if s.browser_key {
            tr(p, pos2(x_key + 10.0, head.center().y), Align2::LEFT_CENTER, "Tom", 13.0, k_dim());
            p.line_segment([pos2(x_key, list.top()), pos2(x_key, list.bottom())], Stroke::new(1.0_f32, k_line()));
        }
        let rows_r = Rect::from_min_max(pos2(list.left(), head.bottom()), list.max);
        let rows = (rows_r.height() / row_h).floor().max(1.0) as usize;
        let n = lib.tracks.len();
        into_view(&mut first.1, lib.track_sel, rows, n);
        let lp = p.with_clip_rect(rows_r);
        for (k, i) in (first.1..(first.1 + rows).min(n)).enumerate() {
            let e = &lib.tracks[i];
            let r = Rect::from_min_size(pos2(rows_r.left(), rows_r.top() + k as f32 * row_h), vec2(rows_r.width(), row_h));
            let bg = match (i == lib.track_sel, on_tracks) {
                (true, true) => k_sel(),
                (true, false) => k_sel_off(),
                _ if k % 2 == 0 => Color32::from_rgb(24, 25, 28),
                _ => k_bg(),
            };
            lp.rect_filled(r, 0.0, bg);
            lp.line_segment([pos2(r.left(), r.bottom()), pos2(r.right(), r.bottom())], Stroke::new(1.0_f32, k_panel2()));
            // Nome (▶ verde se está num deck; ✓ se já tocou).
            let is_loaded = loaded.contains(&e.path);
            let played = lib.played.contains(&e.path);
            let col = if is_loaded { k_green() } else if played { k_dim() } else { k_text() };
            let mark = if is_loaded { "▶" } else if played { "✓" } else { "♪" };
            tr(&lp, pos2(x_title + 12.0, r.center().y), Align2::LEFT_CENTER, mark, font * 0.8, if is_loaded { k_green() } else { k_dim() });
            let np = lp.with_clip_rect(Rect::from_min_max(pos2(x_title, r.top()), pos2(x_bpm - 8.0, r.bottom())));
            let name = if e.artist.is_empty() { e.title.clone() } else { format!("{}  ·  {}", e.title, e.artist) };
            let name = match &e.when {
                Some(w) => format!("{w}   {name}"),
                None => name,
            };
            tr(&np, pos2(x_title + 38.0, r.center().y), Align2::LEFT_CENTER, name, font, col);
            if s.browser_bpm {
                let bpm = e.bpm.filter(|b| *b > 0.0).map_or(String::new(), |b| format!("{b:.1}"));
                tr(&lp, pos2(x_bpm + bpm_w - 10.0, r.center().y), Align2::RIGHT_CENTER, bpm, font, col);
            }
            if let Some(key) = e.key.filter(|_| s.browser_key) {
                let kr = Rect::from_min_max(pos2(x_key + 1.0, r.top()), r.max);
                let good = master_key.is_some_and(|m| keys_match(m, key));
                if good {
                    lp.rect_filled(kr, 0.0, k_green());
                }
                tr(&lp, pos2(x_key + 12.0, r.center().y), Align2::LEFT_CENTER, crate::key::label(key), font, if good { k_bg() } else { col });
            }
            let (c, dc) = click(ui, r.intersect(rows_r), ("nav-musica", i));
            if c || dc {
                clicks.push(Click::Track(i, dc));
            }
        }
        if n == 0 {
            let msg = if lib.folders.get(lib.folder_sel).is_some_and(|f| f.has_children) { "Nenhuma música aqui (LOAD abre as subpastas)" } else { "Nenhuma música aqui" };
            tr(&lp, rows_r.center(), Align2::CENTER_CENTER, msg, 16.0, k_dim());
        }
        if n > rows {
            scrollbar(p, rows_r, first.1, rows, n);
        }
        // ---- fila ----
        let queue_rows = Rect::from_min_max(pos2(queue.left(), queue.top() + head_h), queue.max);
        if queue_w > 0.0 {
            p.line_segment([pos2(queue.left() - 1.0, queue.top()), pos2(queue.left() - 1.0, queue.bottom())], Stroke::new(1.0_f32, k_line()));
            let qh = Rect::from_min_size(queue.min, vec2(queue.width(), head_h));
            p.rect_filled(qh, 0.0, k_panel2());
            tr(p, pos2(qh.left() + 10.0, qh.center().y), Align2::LEFT_CENTER, format!("Fila [{}]", lib.prepare.len()), 13.0, if queue_focus { k_blue() } else { k_dim() });
            let rows = (queue_rows.height() / row_h).floor().max(1.0) as usize;
            let start = if lib.prepare.len() <= rows { 0 } else { lib.prepare_sel.saturating_sub(rows - 1).min(lib.prepare.len() - rows) };
            let qp = p.with_clip_rect(queue_rows);
            for (k, i) in (start..(start + rows).min(lib.prepare.len())).enumerate() {
                let e = &lib.prepare[i];
                let r = Rect::from_min_size(pos2(queue_rows.left(), queue_rows.top() + k as f32 * row_h), vec2(queue_rows.width(), row_h));
                let bg = match (i == lib.prepare_sel, queue_focus) {
                    (true, true) => k_sel(),
                    (true, false) => k_sel_off(),
                    _ if k % 2 == 0 => Color32::from_rgb(24, 25, 28),
                    _ => k_bg(),
                };
                qp.rect_filled(r, 0.0, bg);
                let col = if loaded.contains(&e.path) { k_green() } else { k_text() };
                tr(&qp, pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, &e.title, font * 0.85, col);
                let (c, dc) = click(ui, r.intersect(queue_rows), ("nav-fila", i));
                if c || dc {
                    clicks.push(Click::Queue(i, dc));
                }
            }
        }
        drop(first);
        // Roda do mouse: anda uma linha por "clique" no lado onde o ponteiro está.
        let wheel_on = [(tree_rows, Focus::Folders), (rows_r, Focus::Tracks), (queue_rows, Focus::Prepare)].into_iter().filter(|(r, _)| r.width() > 0.0).find(|(r, _)| ui.rect_contains_pointer(*r)).map(|(_, f)| f);
        let mut wheel_steps = 0;
        if wheel_on.is_some() {
            let acc = self.wheel.get() - ui.input(|i| i.raw_scroll_delta.y);
            wheel_steps = (acc / 40.0).trunc() as i32;
            self.wheel.set(acc - wheel_steps as f32 * 40.0);
        }
        drop(a);

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
        if let (Some(f), true) = (wheel_on, wheel_steps != 0) {
            a.lib.focus = f;
            a.lib.scroll(wheel_steps);
        }
        for c in clicks {
            match c {
                Click::Folder(i, double) => {
                    a.lib.select_folder(i);
                    // Dois cliques: abre/fecha as subpastas (sem subpastas, vai para as músicas).
                    if double {
                        a.lib.open_selected();
                    }
                }
                Click::Track(i, double) | Click::Queue(i, double) => {
                    if matches!(c, Click::Queue(..)) {
                        a.lib.prepare_sel = i;
                        a.lib.focus = Focus::Prepare;
                    } else {
                        a.lib.select_track(i);
                    }
                    if double {
                        // Dois cliques numa música: carrega no primeiro deck parado.
                        let t = a.lib.selected_track();
                        let order = [a.side[0], a.side[1], 0, 1, 2, 3];
                        match (t, order.into_iter().filter(|&d| four || d < 2).find(|&d| !snap.decks[d].playing)) {
                            (Some(t), Some(d)) => {
                                self.loader.load_deck(&mut a, d, t.path);
                                a.big_browser = false;
                            }
                            (Some(_), None) => a.toast("Todos os decks tocando: clique no deck de baixo ou use o LOAD"),
                            _ => {}
                        }
                    }
                }
                Click::Deck(d) => match a.lib.selected_track().filter(|_| a.lib.focus != Focus::Folders) {
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
        p.rect_filled(r, 0.0, k_panel());
        let head = Rect::from_min_size(r.min, vec2(r.width(), 22.0));
        p.rect_filled(head, 0.0, k_panel2());
        let num = Rect::from_min_size(head.min, vec2(22.0, 22.0));
        p.rect_filled(num, 0.0, if on_air(s) { deck_tint(d) } else { k_line() });
        tb(p, num.center(), Align2::CENTER_CENTER, d + 1, 14.0, if on_air(s) { k_bg() } else { k_text() });
        let mut right = head.right() - 6.0;
        if let Some(k) = t.as_ref().and_then(|t| crate::key::from_raw(t.key)) {
            right = tb(p, pos2(right, head.center().y), Align2::RIGHT_CENTER, crate::key::label(k), 13.0, k_text()).left() - 8.0;
        }
        let hp = p.with_clip_rect(Rect::from_min_max(pos2(num.right(), head.top()), pos2(right, head.bottom())));
        let title = t.as_ref().map_or("Sem música".to_string(), |t| format!("♪ {}", t.title));
        tr(&hp, pos2(num.right() + 8.0, head.center().y), Align2::LEFT_CENTER, title, 13.0, if t.is_some() { k_text() } else { k_dim() });

        let y = head.bottom() + 4.0;
        tr(p, pos2(r.left() + 8.0, y), Align2::LEFT_TOP, "RESTANTE", 9.0, k_dim());
        let time = if t.is_some() { remain_text(s) } else { "--:--.-".into() };
        tb(p, pos2(r.left() + 8.0, y + 9.0), Align2::LEFT_TOP, time, 24.0, k_text());
        tr(p, pos2(r.right() - 70.0, y), Align2::RIGHT_TOP, "BPM", 9.0, k_dim());
        let bpm = if s.loaded && s.bpm > 0.0 { format!("{:.1}", s.bpm) } else { "--.-".into() };
        tb(p, pos2(r.right() - 8.0, y + 9.0), Align2::RIGHT_TOP, bpm, 24.0, if s.sync_ok && s.loaded { k_green() } else { k_text() });

        // Música inteira, com o que já tocou mais escuro, hot cues e posição.
        let ov = Rect::from_min_max(pos2(r.left() + 8.0, r.bottom() - 26.0), pos2(r.right() - 8.0, r.bottom() - 6.0));
        p.rect_filled(ov, 0.0, k_bg());
        let Some(t) = t else { return };
        let width = ov.width().max(1.0) as usize;
        let pal = band_colors();
        let played = if s.frames > 0.0 { (s.pos / s.frames).clamp(0.0, 1.0) as f32 * width as f32 } else { 0.0 };
        let o = self.overview_for(d, &t, width);
        let mut mesh = Mesh::default();
        for (x, c) in o.cols.iter().enumerate() {
            let fx = ov.left() + x as f32;
            let dim = if (x as f32) < played { 0.35 } else { 1.0 };
            let hs = layers(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0);
            for (b, v) in hs.into_iter().enumerate() {
                let a = v * ov.height();
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
                p.rect_filled(lab, 1.0, cue_cor(i));
                tb(p, lab.center(), Align2::CENTER_CENTER, CUE_LETTERS[i], 9.0, k_bg());
            }
        }
        let nx = x_of(s.pos);
        p.line_segment([pos2(nx, ov.top() - 2.0), pos2(nx, ov.bottom() + 2.0)], Stroke::new(2.0_f32, k_text()));
    }
}

/// Nome sem o ícone do começo ("💾 NOVO" → "NOVO").
fn plain(name: &str) -> &str {
    name.trim_start_matches(|c: char| !c.is_alphanumeric())
}

/// Barra de rolagem fina à direita de uma lista.
fn scrollbar(p: &Painter, r: Rect, first: usize, rows: usize, n: usize) {
    let bar = Rect::from_min_max(pos2(r.right() - 4.0, r.top()), r.max);
    let h = (bar.height() * rows as f32 / n as f32).max(20.0);
    let y = bar.top() + (bar.height() - h) * first as f32 / (n - rows) as f32;
    p.rect_filled(Rect::from_min_size(pos2(bar.left(), y), vec2(4.0, h)), 2.0, k_dim());
}
