//! DJ OS Preparar: o "rekordbox" do DJ OS. Roda no computador de casa (Windows ou Linux), analisa
//! as músicas do pendrive e grava a análise dentro dele (`DJOS/analise/`). No aparelho as músicas
//! já entram no deck analisadas, sem gastar processador na hora de tocar.

#![cfg_attr(windows, windows_subsystem = "windows")] // sem a janela preta do terminal no Windows

use dj_os_analise::prepare::{self, Outcome, Totals};
use eframe::egui::{self, Color32, RichText};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DJ OS Preparar")
            .with_app_id("dj-os-preparar")
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../pacote/dj-os-preparar.png")).unwrap_or_default())
            .with_inner_size([760.0, 560.0])
            .with_min_inner_size([520.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native("DJ OS Preparar", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

const ACCENT: Color32 = Color32::from_rgb(214, 232, 70);
const MUTED: Color32 = Color32::from_rgb(150, 156, 166);
const BAD: Color32 = Color32::from_rgb(240, 110, 90);

/// O que está acontecendo com o pendrive escolhido.
#[derive(Default)]
enum Phase {
    /// Nenhum pendrive escolhido.
    #[default]
    Idle,
    /// Contando as músicas.
    Counting,
    /// Pronto para começar: total e quantas já estavam preparadas.
    Ready { files: Arc<Vec<PathBuf>>, had: usize },
    Working,
    Finished(Totals),
}

#[derive(Default)]
struct Shared {
    phase: Phase,
    done_count: usize,
    total: usize,
    current: String,
    errors: Vec<String>,
}

struct App {
    drives: Vec<PathBuf>,
    chosen: Option<PathBuf>,
    state: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    last_scan: std::time::Instant,
    frame: u32,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.iter_mut().for_each(|(_, f)| f.size *= 1.15);
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        cc.egui_ctx.set_style(style);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        Self { drives: drives(), chosen: None, state: Arc::default(), stop: Arc::default(), last_scan: std::time::Instant::now(), frame: 0 }
    }

    fn busy(&self) -> bool {
        matches!(self.state.lock().unwrap().phase, Phase::Counting | Phase::Working)
    }

    /// Escolheu um pendrive: conta as músicas e quantas já estão preparadas (em segundo plano).
    fn choose(&mut self, root: PathBuf, ctx: &egui::Context) {
        self.chosen = Some(root.clone());
        *self.state.lock().unwrap() = Shared { phase: Phase::Counting, ..Default::default() };
        let (state, ctx) = (self.state.clone(), ctx.clone());
        std::thread::spawn(move || {
            let files = prepare::list_tracks(&root);
            let had = files.iter().filter(|f| dj_os_analise::store::has_portable(&root, f)).count();
            state.lock().unwrap().phase = Phase::Ready { files: Arc::new(files), had };
            ctx.request_repaint();
        });
    }

    fn start(&mut self, root: PathBuf, files: Arc<Vec<PathBuf>>, ctx: &egui::Context) {
        self.stop.store(false, Ordering::Relaxed);
        {
            let mut s = self.state.lock().unwrap();
            *s = Shared { phase: Phase::Working, total: files.len(), ..Default::default() };
        }
        let (state, stop, ctx) = (self.state.clone(), self.stop.clone(), ctx.clone());
        std::thread::spawn(move || {
            let totals = prepare::prepare_files(&root, &files, prepare::default_threads(), &stop, |f, out| {
                let mut s = state.lock().unwrap();
                s.done_count += 1;
                s.current = file_name(f);
                if let Outcome::Failed(e) = out {
                    s.errors.push(format!("{}: {e}", file_name(f)));
                }
                ctx.request_repaint();
            });
            state.lock().unwrap().phase = Phase::Finished(totals);
            ctx.request_repaint();
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.diagnostics(ctx);
        // A lista de pendrives se atualiza sozinha (pendrive colocado ou tirado).
        if !self.busy() && self.last_scan.elapsed().as_secs() >= 2 {
            self.drives = drives();
            self.last_scan = std::time::Instant::now();
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(2));

        egui::CentralPanel::default().frame(egui::Frame::central_panel(&ctx.style()).inner_margin(24.0)).show(ctx, |ui| {
            ui.label(RichText::new("DJ OS Preparar").size(30.0).strong().color(ACCENT));
            ui.label(RichText::new("Analisa as músicas do pendrive aqui no computador. No aparelho DJ OS elas já entram no deck com BPM, grade, tom, volume e waveform prontos.").color(MUTED));
            ui.add_space(18.0);

            let busy = self.busy();
            ui.label(RichText::new("1. Escolha o pendrive").strong());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if self.drives.is_empty() {
                    ui.label(RichText::new("Nenhum pendrive encontrado. Coloque um ou escolha uma pasta.").color(MUTED));
                }
                for d in self.drives.clone() {
                    let selected = self.chosen.as_deref() == Some(d.as_path());
                    let text = RichText::new(format!("💾 {}", drive_label(&d))).color(if selected { Color32::BLACK } else { ui.visuals().text_color() });
                    let button = egui::Button::new(text).fill(if selected { ACCENT } else { ui.visuals().widgets.inactive.bg_fill });
                    if ui.add_enabled(!busy, button).clicked() {
                        self.choose(d, ctx);
                    }
                }
                if ui.add_enabled(!busy, egui::Button::new("Outra pasta…")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().set_title("Pasta do pendrive (ou com as músicas)").pick_folder() {
                        self.choose(p, ctx);
                    }
                }
            });
            ui.add_space(18.0);

            let Some(root) = self.chosen.clone() else { return };
            ui.label(RichText::new(format!("2. Preparar  ·  {}", drive_label(&root))).strong());
            ui.label(RichText::new(root.display().to_string()).small().color(MUTED));
            ui.add_space(6.0);
            let mut start = None;
            {
                let s = self.state.lock().unwrap();
                match &s.phase {
                    Phase::Idle => {}
                    Phase::Counting => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Procurando as músicas…");
                        });
                    }
                    Phase::Ready { files, had } => {
                        let total = files.len();
                        if total == 0 {
                            ui.label(RichText::new("Nenhuma música nesse lugar (mp3, flac, wav, m4a, aac, ogg, aiff).").color(MUTED));
                        } else {
                            ui.label(format!("{total} músicas · {had} já preparadas · {} para analisar", total - had));
                            ui.add_space(10.0);
                            let label = if *had == total { "Conferir de novo" } else { "PREPARAR" };
                            if ui.add(egui::Button::new(RichText::new(label).size(20.0).strong().color(Color32::BLACK)).fill(ACCENT).min_size(egui::vec2(220.0, 48.0))).clicked() {
                                start = Some(files.clone());
                            }
                        }
                    }
                    Phase::Working => {
                        let frac = if s.total > 0 { s.done_count as f32 / s.total as f32 } else { 0.0 };
                        ui.add(egui::ProgressBar::new(frac).text(format!("{} de {}", s.done_count, s.total)).desired_height(26.0));
                        ui.label(RichText::new(&s.current).color(MUTED));
                        ui.add_space(8.0);
                        if ui.button("Parar").clicked() {
                            self.stop.store(true, Ordering::Relaxed);
                        }
                        ui.label(RichText::new("Pode parar a qualquer hora: o que já foi analisado fica guardado no pendrive.").color(MUTED).small());
                    }
                    Phase::Finished(t) => {
                        let stopped = self.stop.load(Ordering::Relaxed);
                        let title = if stopped { "Parado." } else { "Pronto!" };
                        ui.label(RichText::new(title).size(22.0).strong().color(ACCENT));
                        ui.label(format!("{} analisadas agora · {} já estavam · {} com erro", t.done, t.had, t.failed));
                        ui.add_space(6.0);
                        ui.label(RichText::new("Antes de tirar o pendrive, use \"Remover com segurança\" (ou \"Ejetar\").").color(MUTED));
                        if !s.errors.is_empty() {
                            ui.add_space(8.0);
                            ui.label(RichText::new("Músicas que não deram para analisar:").color(BAD));
                            egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                                for e in &s.errors {
                                    ui.label(RichText::new(e).small());
                                }
                            });
                        }
                    }
                }
            }
            if let Some(files) = start {
                self.start(root.clone(), files, ctx);
            }
            if matches!(self.state.lock().unwrap().phase, Phase::Finished(_)) {
                ui.add_space(8.0);
                if ui.button("Preparar outro pendrive").clicked() {
                    self.chosen = None;
                    *self.state.lock().unwrap() = Shared::default();
                }
            }
        });
    }
}

impl App {
    /// Para o README e testes: DJOS_PREPARAR_PASTA=<pasta> já escolhe a pasta;
    /// DJOS_SCREENSHOT=<arquivo> salva a tela (RGBA cru com cabeçalho "largura altura\n").
    fn diagnostics(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        if self.frame == 2 {
            if let Some(p) = std::env::var_os("DJOS_PREPARAR_PASTA") {
                self.choose(PathBuf::from(p), ctx);
            }
        }
        let Some(path) = std::env::var_os("DJOS_SCREENSHOT") else { return };
        ctx.request_repaint();
        if self.frame == 90 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
        let shot = ctx.input(|i| i.raw.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
        if let Some(img) = shot {
            let mut bytes = format!("{} {}\n", img.size[0], img.size[1]).into_bytes();
            img.pixels.iter().for_each(|c| bytes.extend_from_slice(&c.to_array()));
            let _ = std::fs::write(&path, bytes);
            std::process::exit(0);
        }
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Nome mostrado no botão do pendrive: o nome da pasta onde ele está montado (no Windows, a letra).
fn drive_label(p: &Path) -> String {
    let name = file_name(p);
    if name.is_empty() {
        p.display().to_string()
    } else {
        name
    }
}

/// Pendrives e discos externos ligados agora.
#[cfg(windows)]
fn drives() -> Vec<PathBuf> {
    // Letras de D: a Z: (C: é o disco do Windows).
    (b'D'..=b'Z').map(|c| PathBuf::from(format!("{}:\\", c as char))).filter(|p| p.is_dir()).collect()
}

#[cfg(not(windows))]
fn drives() -> Vec<PathBuf> {
    let user = std::env::var("USER").unwrap_or_default();
    let bases = [PathBuf::from("/media").join(&user), PathBuf::from("/run/media").join(&user), PathBuf::from("/media"), PathBuf::from("/Volumes")];
    let mut out: Vec<PathBuf> = Vec::new();
    for b in bases {
        let Ok(rd) = std::fs::read_dir(&b) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && p != PathBuf::from("/media").join(&user) && !out.contains(&p) && std::fs::read_dir(&p).is_ok() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}
