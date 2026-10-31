//! Tela principal no estilo de equipamento de cabine: fundo preto, tipografia condensada e
//! informação organizada (operada pela controladora, sem depender de toque).
//!
//! ```text
//! ┌──────────┬───────────────────────────────────────────┬──────────────┐
//! │ DECK 1   │  ╷ ╷ ╷ ╷ ╷ ╷  batidas e compassos          │  EFEITO      │
//! │ origem   │  ▁▃▇█▆▃▁▂▅██▇▅▂  waveform de 3 bandas      │  deck 1      │
//! │ tom/loop │  ╵ ╵ ╵ ╵ ╵ ╵  (reage ao EQ e ao TRIM)      │              │
//! │ DECK 2   │  … deck 2 …                                │  EFEITO      │
//! ├──────────┴───────────────────────────────────────────┴──────────────┤
//! │  HOT CUE  A B C D / E F G H            HOT CUE  A B C D / E F G H   │
//! ├─────────────────────────────────┬────────────────────────────────────┤
//! │ DECK │ ♪ título          NO AR  │ DECK │ ♪ título                    │
//! │  1   │ RESTANTE  TEMPO     BPM  │  2   │ …                           │
//! │      │ ▁▂▃▅▃▂▁ música inteira   │      │                             │
//! └─────────────────────────────────┴────────────────────────────────────┘
//! ```
//! Com 4 decks (padrão): em cima continuam as 2 waveforms dos decks da frente; embaixo, os 4
//! painéis (2 x 2), compactos.

use super::{fmt_time, key_badge, light, DjOsApp, ZOOMS};
use crate::engine::fx::{beats_label, step_beats, FxKind};
use crate::shared::{DeckSnap, Snapshot};
use crate::track::{Track, BINS_PER_SEC};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontFamily, FontId, Mesh, Painter, Rect, Shape, Stroke};
use std::sync::Arc;

// ---- cores (sóbrias: preto, cinzas, branco; cor só onde informa algo) ----
pub(super) const K_BG: Color32 = Color32::from_rgb(0, 0, 0);
pub(super) const K_PANEL: Color32 = Color32::from_rgb(14, 15, 17);
const K_PANEL2: Color32 = Color32::from_rgb(26, 27, 31);
const K_LINE: Color32 = Color32::from_rgb(52, 54, 60);
const K_TEXT: Color32 = Color32::from_rgb(236, 236, 236);
const K_DIM: Color32 = Color32::from_rgb(128, 130, 136);
const K_BLUE: Color32 = Color32::from_rgb(30, 140, 255);
const K_ORANGE: Color32 = Color32::from_rgb(255, 140, 20);
const K_RED: Color32 = Color32::from_rgb(230, 30, 40);
const K_GREEN: Color32 = Color32::from_rgb(60, 210, 90);

/// Cores dos hot cues A–H.
pub(super) const CUE_COLORS: [Color32; 8] = [
    Color32::from_rgb(235, 60, 60),
    Color32::from_rgb(40, 190, 230),
    Color32::from_rgb(70, 205, 80),
    Color32::from_rgb(140, 80, 245),
    Color32::from_rgb(130, 220, 60),
    Color32::from_rgb(255, 130, 30),
    Color32::from_rgb(50, 95, 255),
    Color32::from_rgb(240, 205, 40),
];
const CUE_LETTERS: [&str; 8] = ["A", "B", "C", "D", "E", "F", "G", "H"];

/// Cor de cada deck (faixa lateral, número).
fn deck_tint(d: usize) -> Color32 {
    [Color32::from_rgb(30, 140, 255), Color32::from_rgb(255, 140, 20), Color32::from_rgb(170, 90, 255), Color32::from_rgb(40, 200, 150)][d % 4]
}

/// Cores da waveform (grave, médio, agudo) conforme a paleta escolhida em CONFIG → TELA.
fn band_colors() -> [Color32; 3] {
    match super::PALETTE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => [Color32::from_rgb(220, 40, 40), Color32::from_rgb(40, 200, 80), Color32::from_rgb(60, 140, 255)],
        2 => [Color32::from_rgb(20, 70, 200), Color32::from_rgb(60, 150, 255), Color32::from_rgb(190, 225, 255)],
        _ => [Color32::from_rgb(0, 95, 255), Color32::from_rgb(255, 150, 30), Color32::from_rgb(255, 238, 220)],
    }
}

// ---- fonte condensada (Noto Sans Condensed, licença OFL, em assets/fonts) ----
const FONT_REG: &str = "cond";
const FONT_BOLD: &str = "cond-bold";

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        (FONT_REG, &include_bytes!("../../assets/fonts/NotoSans-CondensedMedium.ttf")[..]),
        (FONT_BOLD, &include_bytes!("../../assets/fonts/NotoSans-CondensedSemiBold.ttf")[..]),
    ] {
        fonts.font_data.insert(name.into(), egui::FontData::from_static(bytes));
        // Família própria, com as fontes padrão de reserva (ícones e emoji).
        let mut fam = vec![name.to_string()];
        fam.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), fam);
    }
    ctx.set_fonts(fonts);
}

fn tr(p: &Painter, pos: egui::Pos2, align: Align2, s: impl ToString, size: f32, color: Color32) -> Rect {
    p.text(pos, align, s, FontId::new(size, FontFamily::Name(FONT_REG.into())), color)
}

fn tb(p: &Painter, pos: egui::Pos2, align: Align2, s: impl ToString, size: f32, color: Color32) -> Rect {
    p.text(pos, align, s, FontId::new(size, FontFamily::Name(FONT_BOLD.into())), color)
}

/// Etiqueta pequena com fundo (ex.: "NO AR", "SYNC").
fn tag(p: &Painter, right_center: egui::Pos2, label: &str, bg: Color32, fg: Color32) -> Rect {
    let g = p.layout_no_wrap(label.to_string(), FontId::new(12.0, FontFamily::Name(FONT_BOLD.into())), fg);
    let r = Rect::from_min_size(pos2(right_center.x - g.size().x - 10.0, right_center.y - 9.0), vec2(g.size().x + 10.0, 18.0));
    p.rect_filled(r, 2.0, bg);
    p.galley(pos2(r.left() + 5.0, r.center().y - g.size().y / 2.0), g, fg);
    r
}

/// "USB · NOME", "INTERNO" ou "CELULAR".
fn source_label(t: &Track) -> String {
    let p = t.path.to_string_lossy();
    if p.starts_with("/media/") || p.starts_with("/run/media/") {
        let name = t.path.components().nth(3).map(|c| c.as_os_str().to_string_lossy().to_string()).unwrap_or_default();
        format!("USB · {name}")
    } else {
        "INTERNO".into()
    }
}

fn on_air(s: &DeckSnap) -> bool {
    s.loaded && s.playing && s.fader > 0.1
}

/// Tempo "mm:ss.c" com sinal de menos para o restante.
fn remain_text(s: &DeckSnap) -> String {
    if s.sr <= 0.0 || s.frames <= 0.0 {
        return "--:--.-".into();
    }
    format!("-{}", fmt_time((s.frames - s.pos) / s.sr / s.rate.max(0.01)))
}

impl DjOsApp {
    /// Tela principal (2 ou 4 decks). `side` são os 2 decks da frente (os que a REV1 controla).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_principal(
        &mut self,
        ui: &mut egui::Ui,
        p: &Painter,
        body: Rect,
        snap: &Snapshot,
        side: [usize; 2],
        tracks: &[Option<Arc<Track>>],
        loading: &[Option<String>],
        fx_pick: [Option<FxKind>; 2],
        zoom: f64,
    ) {
        let four = self.app.lock().settings.four_decks;
        p.rect_filled(body.expand(8.0), 0.0, K_BG);
        // Em cima, sempre os 2 decks da frente (os que a controladora está comandando).
        let shown: Vec<usize> = side.to_vec();
        let gap = 4.0;
        let card_w = 176.0;
        let fx_w = 236.0;
        let ph = if four { 98.0 } else { 150.0 };
        let panel_h = if four { 2.0 * ph + gap } else { ph };
        let cues_h = 54.0;
        let waves = Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - panel_h - cues_h - 2.0 * gap));

        // Coluna da esquerda (um cartão por deck) e waveforms.
        let n = shown.len() as f32;
        let row_h = (waves.height() - gap * (n - 1.0)) / n;
        let wave_area = Rect::from_min_max(pos2(waves.left() + card_w + gap, waves.top()), pos2(waves.right() - fx_w - gap, waves.bottom()));
        for (k, &d) in shown.iter().enumerate() {
            let y = waves.top() + k as f32 * (row_h + gap);
            let card = Rect::from_min_size(pos2(waves.left(), y), vec2(card_w, row_h));
            self.draw_card(p, card, d, tracks[d].as_deref(), &snap.decks[d], side.contains(&d));
            let wr = Rect::from_min_size(pos2(wave_area.left(), y), vec2(wave_area.width(), row_h));
            draw_wave_pro(p, wr, d, tracks[d].as_deref(), &snap.decks[d], loading[d].as_deref(), zoom, true);
            if ui.interact(wr, ui.id().with(("onda", d)), egui::Sense::click()).clicked() {
                let mut a = self.app.lock();
                a.wave_zoom = (a.wave_zoom + 1) % ZOOMS.len();
            }
        }
        tr(p, wave_area.right_top() + vec2(-6.0, 2.0), Align2::RIGHT_TOP, format!("ZOOM {zoom:.0}s"), 11.0, K_DIM);

        // Efeitos (dos 2 decks da frente) na coluna da direita.
        for (k, &d) in side.iter().enumerate() {
            let h = (waves.height() - gap) / 2.0;
            let fr = Rect::from_min_size(pos2(waves.right() - fx_w, waves.top() + k as f32 * (h + gap)), vec2(fx_w, h));
            draw_fx_pro(p, fr, d, &snap.decks[d], fx_pick[k]);
        }

        // Faixa dos hot cues dos 2 decks da frente.
        let col_w = (body.width() - gap) / 2.0;
        let y = waves.bottom() + gap;
        for (k, &d) in side.iter().enumerate() {
            let r = Rect::from_min_size(pos2(body.left() + k as f32 * (col_w + gap), y), vec2(col_w, cues_h));
            draw_cue_strip(p, r, &snap.decks[d], tracks[d].as_deref());
        }

        // Painéis dos decks embaixo: os 4 (1 e 2 em cima, 3 e 4 embaixo) ou só os 2 da frente.
        let py = body.bottom() - panel_h;
        let panels: Vec<usize> = if four { vec![0, 1, 2, 3] } else { side.to_vec() };
        for (k, &d) in panels.iter().enumerate() {
            let (cx, cy) = ((k % 2) as f32, (k / 2) as f32);
            let r = Rect::from_min_size(pos2(body.left() + cx * (col_w + gap), py + cy * (ph + gap)), vec2(col_w, ph));
            self.draw_deck_panel(ui, p, r, d, tracks[d].clone(), &snap.decks[d], four, side.contains(&d));
        }

        // Ajuste da grade (tocar no BPM): em cima da waveform do outro deck.
        let editing = self.app.lock().grid_edit.as_ref().map(|g| g.deck);
        if let Some(d) = editing {
            match (shown.iter().position(|&x| x == d), tracks[d].clone()) {
                (Some(k), Some(t)) => {
                    let other = if shown.len() > 1 { (k + 1) % shown.len() } else { k };
                    let y = waves.top() + other as f32 * (row_h + gap);
                    let gr = Rect::from_min_size(pos2(wave_area.left(), y), vec2(wave_area.width(), row_h.max(170.0)));
                    self.draw_grid_editor(ui, p, gr, d, &t, &snap.decks[d]);
                }
                _ => self.app.lock().grid_edit = None,
            }
        }
    }

    /// Cartão do deck na coluna da esquerda: origem, tom, loop e modo dos pads.
    fn draw_card(&self, p: &Painter, r: Rect, d: usize, t: Option<&Track>, s: &DeckSnap, front: bool) {
        p.rect_filled(r, 0.0, K_PANEL);
        let head = Rect::from_min_size(r.min, vec2(r.width(), 24.0));
        p.rect_filled(head, 0.0, if front { K_PANEL2 } else { K_PANEL });
        p.rect_filled(Rect::from_min_size(head.min, vec2(4.0, head.height())), 0.0, deck_tint(d));
        tb(p, head.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, format!("DECK {}", d + 1), 14.0, if front { deck_tint(d) } else { K_DIM });
        if on_air(s) {
            tag(p, head.right_center() - vec2(4.0, 0.0), "NO AR", K_RED, K_TEXT);
        }
        let mut y = head.bottom() + 4.0;
        let line_h = ((r.bottom() - y) / 4.0).clamp(16.0, 30.0);
        let row = |p: &Painter, y: f32, label: &str| {
            tr(p, pos2(r.left() + 12.0, y + line_h / 2.0), Align2::LEFT_CENTER, label, 11.0, K_DIM);
            p.line_segment([pos2(r.left() + 8.0, y + line_h), pos2(r.right() - 8.0, y + line_h)], Stroke::new(1.0_f32, K_PANEL2));
        };
        let vx = r.left() + 62.0;
        // Origem.
        row(p, y, "ORIGEM");
        let src = if s.live { "CELULAR".to_string() } else { t.map_or("—".into(), source_label) };
        tb(p, pos2(vx, y + line_h / 2.0), Align2::LEFT_CENTER, src, 14.0, K_TEXT);
        y += line_h;
        // Tom.
        row(p, y, "TOM");
        match t.and_then(|t| crate::key::from_raw(t.key)) {
            Some(k) if !s.live => {
                let rb = key_badge(p, pos2(vx + 44.0, y + line_h / 2.0), k, 13.0);
                tr(p, pos2(rb.right() + 6.0, y + line_h / 2.0), Align2::LEFT_CENTER, crate::key::name(k), 11.0, K_DIM);
            }
            _ => {
                tb(p, pos2(vx, y + line_h / 2.0), Align2::LEFT_CENTER, "—", 14.0, K_DIM);
            }
        }
        y += line_h;
        // Loop.
        row(p, y, "LOOP");
        let lp = if s.loop_on { format!("{} BAT.", beats_label(s.loop_beats as f32)) } else { "—".into() };
        tb(p, pos2(vx, y + line_h / 2.0), Align2::LEFT_CENTER, lp, 14.0, if s.loop_on { K_ORANGE } else { K_DIM });
        y += line_h;
        // Pads.
        if y + line_h <= r.bottom() + 1.0 {
            row(p, y, "PADS");
            let mode = self.app.lock().pad_mode[d].name();
            tb(p, pos2(vx, y + line_h / 2.0), Align2::LEFT_CENTER, mode, 13.0, K_TEXT);
        }
    }

    /// Painel do deck embaixo: título, tempo restante, pitch, BPM e a música inteira (mini-onda).
    #[allow(clippy::too_many_arguments)]
    fn draw_deck_panel(&mut self, ui: &mut egui::Ui, p: &Painter, r: Rect, d: usize, t: Option<Arc<Track>>, s: &DeckSnap, compact: bool, front: bool) {
        p.rect_filled(r, 0.0, K_PANEL);
        p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, if front { K_LINE } else { K_PANEL2 }));
        // Número do deck.
        let num = Rect::from_min_size(r.min + vec2(6.0, 6.0), vec2(46.0, if compact { 40.0 } else { 52.0 }));
        p.rect_filled(num, 2.0, if front { deck_tint(d) } else { K_PANEL2 });
        tr(p, num.center_top() + vec2(0.0, 3.0), Align2::CENTER_TOP, "DECK", 10.0, if front { K_BG } else { K_DIM });
        tb(p, num.center_bottom() - vec2(0.0, 2.0), Align2::CENTER_BOTTOM, d + 1, if compact { 22.0 } else { 28.0 }, if front { K_BG } else { K_TEXT });

        let x0 = num.right() + 10.0;
        // Título (e artista) com as etiquetas à direita.
        let mut right = r.right() - 8.0;
        if on_air(s) {
            right = tag(p, pos2(right, r.top() + 16.0), "NO AR", K_RED, K_TEXT).left() - 6.0;
        }
        if s.sync_ok && s.loaded {
            right = tag(p, pos2(right, r.top() + 16.0), "SYNC", K_GREEN, K_BG).left() - 6.0;
        }
        if s.keylock {
            right = tag(p, pos2(right, r.top() + 16.0), "KEYLOCK", K_PANEL2, K_TEXT).left() - 6.0;
        }
        if s.slip {
            // Aceso enquanto a música segue "por baixo" (scratch ou loop com slip).
            let (bg, fg) = if s.slip_pos.is_some() { (K_ORANGE, K_BG) } else { (K_PANEL2, K_TEXT) };
            right = tag(p, pos2(right, r.top() + 16.0), "SLIP", bg, fg).left() - 6.0;
        }
        let title_clip = p.with_clip_rect(Rect::from_min_max(pos2(x0, r.top()), pos2(right - 4.0, r.top() + 32.0)));
        match (&t, s.live) {
            (_, true) => {
                tb(&title_clip, pos2(x0, r.top() + 16.0), Align2::LEFT_CENTER, "CELULAR (Bluetooth)", 18.0, K_TEXT);
            }
            (Some(t), _) => {
                let tt = tb(&title_clip, pos2(x0, r.top() + 16.0), Align2::LEFT_CENTER, format!("♪ {}", t.title), 18.0, K_TEXT);
                if !t.artist.is_empty() {
                    tr(&title_clip, pos2(tt.right() + 10.0, r.top() + 17.0), Align2::LEFT_CENTER, &t.artist, 13.0, K_DIM);
                }
            }
            (None, _) => {
                let msg = self.app.lock().decks[d].loading.clone().map_or("Sem música".to_string(), |n| format!("Carregando {n}…"));
                tr(&title_clip, pos2(x0, r.top() + 16.0), Align2::LEFT_CENTER, msg, 16.0, K_DIM);
            }
        }

        // BPM (tocar abre o ajuste da grade).
        // Compacto (4 decks): o BPM desce até o pé do painel e a mini-onda fica à esquerda dele.
        let bpm_h = if compact { r.height() - 42.0 } else { 58.0 };
        let bpm_box = Rect::from_min_size(pos2(r.right() - 112.0, r.top() + 34.0), vec2(104.0, bpm_h));
        p.rect_stroke(bpm_box, 2.0, Stroke::new(1.5_f32, if s.sync_ok && s.loaded { K_GREEN } else { K_TEXT }));
        tr(p, bpm_box.left_top() + vec2(5.0, 2.0), Align2::LEFT_TOP, "BPM", 10.0, K_DIM);
        let bpm_txt = if s.loaded && s.bpm > 0.0 { format!("{:.1}", s.bpm) } else { "--.-".into() };
        tb(p, bpm_box.right_bottom() - vec2(6.0, 1.0), Align2::RIGHT_BOTTOM, bpm_txt, if compact { 26.0 } else { 34.0 }, K_TEXT);
        if t.is_some() && !s.live && ui.interact(bpm_box, ui.id().with(("bpm", d)), egui::Sense::click()).clicked() {
            let mut a = self.app.lock();
            let open = a.grid_edit.as_ref().is_some_and(|g| g.deck == d);
            a.grid_edit = if open { None } else { Some(crate::grid::GridEdit::new(d)) };
        }

        // Tempo restante e decorrido.
        let ty = r.top() + 34.0;
        tr(p, pos2(x0, ty), Align2::LEFT_TOP, "RESTANTE", 10.0, K_DIM);
        let big = if compact { 24.0 } else { 40.0 };
        let tt = tb(p, pos2(x0, ty + 8.0), Align2::LEFT_TOP, if s.live { "AO VIVO".into() } else { remain_text(s) }, big, K_TEXT);
        if !compact && s.sr > 0.0 && !s.live {
            tr(p, pos2(tt.right() + 8.0, tt.bottom() - 8.0), Align2::LEFT_BOTTOM, fmt_time(s.pos / s.sr), 13.0, K_DIM);
        }
        // Pitch e faixa.
        let px = bpm_box.left() - 120.0;
        tr(p, pos2(px, ty), Align2::LEFT_TOP, "TEMPO", 10.0, K_DIM);
        let pitch = (s.rate - 1.0) * 100.0;
        let range = Rect::from_min_size(pos2(px + 40.0, ty - 1.0), vec2(30.0, 13.0));
        p.rect_filled(range, 2.0, if s.vinyl { K_PANEL2 } else { K_ORANGE });
        tb(p, range.center(), Align2::CENTER_CENTER, format!("±{:.0}", s.pitch_range * 100.0), 10.0, if s.vinyl { K_TEXT } else { K_BG });
        let ptxt = if s.rate > 0.0 { format!("{:+.2}%", pitch) } else { "--".into() };
        tb(p, pos2(px, ty + 14.0), Align2::LEFT_TOP, ptxt, if compact { 18.0 } else { 22.0 }, if pitch.abs() < 0.005 { K_TEXT } else { K_ORANGE });
        if !compact && s.auto_gain_db.abs() >= 0.1 && s.loaded && !s.live {
            tr(p, pos2(px, ty + if compact { 34.0 } else { 40.0 }), Align2::LEFT_TOP, format!("AG {:+.1} dB", s.auto_gain_db), 11.0, K_DIM);
        }

        // Música inteira (mini-onda) com o que já tocou, cues e posição.
        let oh = if compact { 16.0 } else { 30.0 };
        let ov_right = if compact { bpm_box.left() - 8.0 } else { r.right() - 8.0 };
        let ov = Rect::from_min_max(pos2(x0, r.bottom() - oh - 8.0), pos2(ov_right, r.bottom() - 8.0));
        p.rect_filled(ov, 0.0, K_BG);
        if let Some(t) = &t {
            let width = ov.width().max(1.0) as usize;
            let pal = band_colors();
            let o = self.overview_for(d, t, width);
            let mut mesh = Mesh::default();
            let mid = ov.bottom() - 2.0;
            let frac_played = if s.frames > 0.0 { (s.pos / s.frames).clamp(0.0, 1.0) as f32 } else { 0.0 };
            for (x, c) in o.cols.iter().enumerate() {
                let fx = ov.left() + x as f32;
                let played = (x as f32) < frac_played * width as f32;
                let dim = if played { 0.35 } else { 1.0 };
                for (v, col, scale) in [(c[0], pal[0], 1.0f32), (c[1], pal[1], 0.46), (c[2], pal[2], 0.28)] {
                    let a = (v as f32 / 255.0).powf(0.75) * scale * (oh - 4.0);
                    if a > 0.5 {
                        mesh.add_colored_rect(Rect::from_min_max(pos2(fx, mid - a), pos2(fx + 1.0, mid)), col.gamma_multiply(dim));
                    }
                }
            }
            p.add(Shape::mesh(mesh));
            let x_of = |pos: f64| ov.left() + (pos / s.frames.max(1.0)) as f32 * ov.width();
            // Cue (triângulo laranja) e hot cues (letras).
            let cx = x_of(s.cue);
            p.add(Shape::convex_polygon(vec![pos2(cx - 5.0, ov.top() - 1.0), pos2(cx + 5.0, ov.top() - 1.0), pos2(cx, ov.top() + 6.0)], K_ORANGE, Stroke::NONE));
            for (i, h) in s.hotcues.iter().enumerate() {
                if let Some(hp) = h {
                    let hx = x_of(*hp);
                    p.line_segment([pos2(hx, ov.top()), pos2(hx, ov.bottom())], Stroke::new(1.0_f32, CUE_COLORS[i]));
                }
            }
            let nx = x_of(s.pos);
            p.line_segment([pos2(nx, ov.top() - 2.0), pos2(nx, ov.bottom() + 2.0)], Stroke::new(2.0_f32, K_TEXT));
        }
    }
}

/// Waveform rolante de 3 bandas: cada banda cresce ou some com o EQ e o TRIM do canal (tirou o
/// grave no knob, o azul diminui na hora). Batidas e compassos em cima e embaixo; hot cues A–H.
#[allow(clippy::too_many_arguments)]
fn draw_wave_pro(p: &Painter, r: Rect, d: usize, t: Option<&Track>, s: &DeckSnap, loading: Option<&str>, zoom_secs: f64, ticks: bool) {
    p.rect_filled(r, 0.0, K_BG);
    let tick_h = if ticks { 12.0 } else { 0.0 };
    let wr = Rect::from_min_max(pos2(r.left(), r.top() + tick_h), pos2(r.right(), r.bottom() - tick_h));
    let mid = wr.center().y;
    let Some(t) = t else {
        let msg = loading.map_or(String::new(), |n| format!("Carregando {n}…"));
        tr(p, wr.center(), Align2::CENTER_CENTER, msg, 15.0, K_DIM);
        p.line_segment([pos2(wr.left(), mid), pos2(wr.right(), mid)], Stroke::new(1.0_f32, K_PANEL2));
        return;
    };
    let pal = band_colors();
    let pos_s = s.pos / t.sr;
    let w = wr.width();
    let px_per_s = w as f64 / zoom_secs;
    let hh = wr.height() * 0.5 - 1.0;
    let nb = t.wave.low.len();
    let center_x = wr.left() + w / 2.0;
    let t_of = |x: f32| pos_s + (x - center_x) as f64 / px_per_s;
    let x_of = |sec: f64| center_x + ((sec - pos_s) * px_per_s) as f32;

    // Loop.
    if let (true, Some(a), Some(b)) = (s.loop_on, s.loop_in, s.loop_out) {
        let (xa, xb) = (x_of(a / t.sr).max(wr.left()), x_of(b / t.sr).min(wr.right()));
        if xb > xa {
            p.rect_filled(Rect::from_x_y_ranges(xa..=xb, wr.y_range()), 0.0, K_ORANGE.gamma_multiply(0.14));
        }
    }

    // Ganho de cada banda: EQ × TRIM (o TRIM no meio = 1). Tirar o grave some com o azul.
    let trim = s.trim.max(0.0);
    let gains = [s.eq[0] * trim, s.eq[1] * trim, s.eq[2] * trim];
    let mut mesh = Mesh::default();
    let (step, layers) = if light() { (2.0f32, 2) } else { (1.0f32, 3) };
    let mut x = wr.left();
    while x < wr.right() {
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
            let dim = if ts < pos_s { 0.55 } else { 1.0 };
            // Grave por fora, médio no meio, agudo no centro (a curva realça o que é baixo).
            for (k, (v, scale)) in [(l, 1.0f32), (m, 0.46), (h, 0.28)].into_iter().enumerate().take(layers) {
                let a = ((v as f32 / 255.0).powf(0.75) * scale * gains[k]).min(1.0) * hh;
                if a > 0.5 {
                    mesh.add_colored_rect(Rect::from_min_max(pos2(x, mid - a), pos2(x + step, mid + a)), pal[k].gamma_multiply(dim));
                }
            }
        }
        x += step;
    }
    p.add(Shape::mesh(mesh));

    // Batidas e compassos (marcas em cima e embaixo; a 1ª do compasso em vermelho).
    if t.bpm() > 0.0 {
        let bl = t.beat_len() / t.sr;
        let fb = t.first_beat() / t.sr;
        let k0 = ((t_of(wr.left()) - fb) / bl).ceil() as i64;
        let k1 = ((t_of(wr.right()) - fb) / bl).floor() as i64;
        for k in k0..=k1 {
            let bx = x_of(fb + k as f64 * bl);
            let down = k.rem_euclid(4) == 0;
            if ticks {
                let (len, col) = if down { (9.0, K_RED) } else { (5.0, K_DIM) };
                p.line_segment([pos2(bx, r.top() + 1.0), pos2(bx, r.top() + 1.0 + len)], Stroke::new(1.5_f32, col));
                p.line_segment([pos2(bx, r.bottom() - 1.0 - len), pos2(bx, r.bottom() - 1.0)], Stroke::new(1.5_f32, col));
                // Número do compasso a cada 4 compassos.
                let bar = k.div_euclid(4) + 1;
                if down && bar % 4 == 1 {
                    tr(p, pos2(bx + 3.0, r.bottom() - 1.0), Align2::LEFT_BOTTOM, bar, 10.0, K_DIM);
                }
            }
            if down {
                p.line_segment([pos2(bx, wr.top()), pos2(bx, wr.bottom())], Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 34)));
            }
        }
        // Compasso.batida na agulha.
        let b = t.beats_at(s.pos);
        if b >= 0.0 {
            let txt = format!("{}.{}", (b / 4.0).floor() as i64 + 1, (b.rem_euclid(4.0)).floor() as i64 + 1);
            tb(p, pos2(center_x - 6.0, wr.top() + 2.0), Align2::RIGHT_TOP, txt, 13.0, K_ORANGE);
        }
    }

    // Slip: onde a música estaria (linha tracejada; ao soltar, o som volta para ela).
    if let Some(sp) = s.slip_pos {
        let sx = x_of(sp / t.sr);
        if wr.x_range().contains(sx) {
            p.add(Shape::dashed_line(&[pos2(sx, wr.top()), pos2(sx, wr.bottom())], Stroke::new(2.0_f32, K_ORANGE), 5.0, 4.0));
        }
    }

    // Cue (triângulo) e hot cues (letra colorida em cima da linha).
    let cx = x_of(s.cue / t.sr);
    if wr.x_range().contains(cx) {
        p.add(Shape::convex_polygon(vec![pos2(cx - 6.0, wr.bottom()), pos2(cx + 6.0, wr.bottom()), pos2(cx, wr.bottom() - 9.0)], K_ORANGE, Stroke::NONE));
    }
    for (i, h) in s.hotcues.iter().enumerate() {
        if let Some(hp) = h {
            let hx = x_of(hp / t.sr);
            if wr.x_range().contains(hx) {
                p.line_segment([pos2(hx, wr.top()), pos2(hx, wr.bottom())], Stroke::new(1.0_f32, CUE_COLORS[i]));
                let lab = Rect::from_min_size(pos2(hx, wr.bottom() - 15.0), vec2(14.0, 14.0));
                p.rect_filled(lab, 1.0, CUE_COLORS[i]);
                tb(p, lab.center(), Align2::CENTER_CENTER, CUE_LETTERS[i], 11.0, K_BG);
            }
        }
    }
    // Agulha e a cor do deck à esquerda.
    p.line_segment([pos2(center_x, r.top()), pos2(center_x, r.bottom())], Stroke::new(2.0_f32, K_TEXT));
    p.rect_filled(Rect::from_min_size(r.min, vec2(3.0, r.height())), 0.0, deck_tint(d));
}

/// Painel do efeito do deck: nome, batida (anterior | atual | seguinte), tempo e força.
fn draw_fx_pro(p: &Painter, r: Rect, d: usize, s: &DeckSnap, pick: Option<FxKind>) {
    let f = s.fx[0];
    p.rect_filled(r, 0.0, K_PANEL);
    let head = Rect::from_min_size(r.min, vec2(r.width(), 22.0));
    p.rect_filled(head, 0.0, K_PANEL2);
    tr(p, head.center(), Align2::CENTER_CENTER, format!("EFEITO · DECK {}", d + 1), 12.0, K_DIM);
    if f.active {
        tag(p, head.right_center() - vec2(4.0, 0.0), "ON", K_ORANGE, K_BG);
    }
    let name = pick.unwrap_or(f.kind).name().to_uppercase();
    tb(p, pos2(r.center().x, head.bottom() + 6.0), Align2::CENTER_TOP, name, 24.0, if pick.is_some() { K_ORANGE } else { K_TEXT });
    // Batidas.
    let by = head.bottom() + 40.0;
    let cw = (r.width() - 16.0) / 3.0;
    for (k, b) in [step_beats(f.beats, -1), f.beats, step_beats(f.beats, 1)].into_iter().enumerate() {
        let c = Rect::from_min_size(pos2(r.left() + 8.0 + k as f32 * cw, by), vec2(cw - 2.0, 30.0));
        let cur = k == 1;
        p.rect_filled(c, 0.0, if cur { K_LINE } else { K_PANEL2 });
        tb(p, c.center(), Align2::CENTER_CENTER, beats_label(b), if cur { 18.0 } else { 14.0 }, if cur { K_TEXT } else { K_DIM });
    }
    let mut y = by + 38.0;
    // Tempo da batida em ms (no BPM do deck).
    if s.bpm > 0.0 && y + 18.0 < r.bottom() {
        tr(p, pos2(r.left() + 10.0, y), Align2::LEFT_TOP, "TEMPO", 11.0, K_DIM);
        tb(p, pos2(r.right() - 10.0, y - 2.0), Align2::RIGHT_TOP, format!("{:.0} ms", 60000.0 / s.bpm * f.beats as f64), 16.0, K_TEXT);
        y += 24.0;
    }
    // Força (dry/wet).
    if y + 20.0 < r.bottom() {
        tr(p, pos2(r.left() + 10.0, y), Align2::LEFT_TOP, "FORÇA", 11.0, K_DIM);
        tb(p, pos2(r.right() - 10.0, y - 2.0), Align2::RIGHT_TOP, format!("{:.0}%", f.level * 100.0), 14.0, K_TEXT);
        let bar = Rect::from_min_size(pos2(r.left() + 10.0, y + 16.0), vec2(r.width() - 20.0, 4.0));
        p.rect_filled(bar, 0.0, K_PANEL2);
        p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * f.level.clamp(0.0, 1.0), 4.0)), 0.0, if f.active { K_ORANGE } else { K_DIM });
    }
}

/// Faixa dos hot cues de um deck: A–D em cima, E–H embaixo, com a cor e o tempo de cada um.
fn draw_cue_strip(p: &Painter, r: Rect, s: &DeckSnap, t: Option<&Track>) {
    tr(p, pos2(r.center().x, r.top()), Align2::CENTER_TOP, "HOT CUE", 10.0, K_DIM);
    let top = r.top() + 13.0;
    let cw = r.width() / 4.0;
    let ch = (r.height() - 15.0) / 2.0;
    for i in 0..8 {
        let c = Rect::from_min_size(pos2(r.left() + (i % 4) as f32 * cw, top + (i / 4) as f32 * ch), vec2(cw - 3.0, ch - 2.0));
        p.rect_filled(c, 0.0, K_PANEL);
        let lab = Rect::from_min_size(pos2(c.left() + 3.0, c.center().y - 7.0), vec2(14.0, 14.0));
        match (s.hotcues[i], t) {
            (Some(pos), Some(t)) => {
                p.rect_filled(lab, 1.0, CUE_COLORS[i]);
                tb(p, lab.center(), Align2::CENTER_CENTER, CUE_LETTERS[i], 11.0, K_BG);
                let secs = pos / t.sr;
                tr(p, pos2(lab.right() + 6.0, c.center().y), Align2::LEFT_CENTER, format!("{:02}:{:02}", (secs / 60.0) as u32, (secs % 60.0) as u32), 12.0, K_TEXT);
            }
            _ => {
                tr(p, lab.center(), Align2::CENTER_CENTER, CUE_LETTERS[i], 11.0, K_DIM);
            }
        }
    }
}
