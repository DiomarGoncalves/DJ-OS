//! DJ OS Preparar: o "rekordbox" do DJ OS. Roda no computador de casa (Windows ou Linux): mostra
//! as pastas do computador e do pendrive, copia as músicas para o pendrive (arrastando ou pelo
//! botão) já analisando, e organiza as pastas do pendrive (criar, renomear, mover, apagar). A
//! análise vai para dentro do pendrive (`DJOS/analise/`): no aparelho as músicas já entram no
//! deck analisadas, sem gastar processador.

#![cfg_attr(windows, windows_subsystem = "windows")] // sem a janela preta do terminal no Windows

mod biblioteca;

use biblioteca::{Arvore, Copia, Lado, Musica};
use dj_os_analise::prepare::{self, Outcome};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontFamily, FontId, Rect, Sense, Stroke};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DJ OS Preparar")
            .with_app_id("dj-os-preparar")
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../pacote/dj-os-preparar.png")).unwrap_or_default())
            .with_inner_size([1180.0, 720.0])
            .with_min_inner_size([820.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native("DJ OS Preparar", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

// ---- cores do aparelho (preto, cinzas, branco; cor só onde informa algo) ----
const K_BG: Color32 = Color32::from_rgb(0, 0, 0);
const K_PANEL: Color32 = Color32::from_rgb(14, 15, 17);
const K_PANEL2: Color32 = Color32::from_rgb(26, 27, 31);
const K_ROW: Color32 = Color32::from_rgb(24, 25, 28);
const K_LINE: Color32 = Color32::from_rgb(52, 54, 60);
const K_TEXT: Color32 = Color32::from_rgb(236, 236, 236);
const K_DIM: Color32 = Color32::from_rgb(128, 130, 136);
const K_BLUE: Color32 = Color32::from_rgb(30, 140, 255);
const K_SEL: Color32 = Color32::from_rgb(20, 110, 220);
const K_GREEN: Color32 = Color32::from_rgb(60, 210, 90);
const K_RED: Color32 = Color32::from_rgb(230, 30, 40);
const K_ORANGE: Color32 = Color32::from_rgb(255, 140, 20);

const ROW_H: f32 = 34.0;

fn cond(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

fn cond_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("cond-bold".into()))
}

/// A mesma fonte do aparelho (Noto Sans Condensed, licença OFL), com as de reserva para ícones.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("cond".into(), egui::FontData::from_static(include_bytes!("../../assets/fonts/NotoSans-CondensedMedium.ttf")));
    fonts.font_data.insert("cond-bold".into(), egui::FontData::from_static(include_bytes!("../../assets/fonts/NotoSans-CondensedSemiBold.ttf")));
    let reserva = fonts.families[&FontFamily::Proportional].clone();
    fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "cond".into());
    fonts.families.insert(FontFamily::Name("cond-bold".into()), std::iter::once("cond-bold".to_string()).chain(reserva).collect());
    ctx.set_fonts(fonts);
}

fn estilo(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = K_BG;
    v.window_fill = K_PANEL;
    v.window_stroke = Stroke::new(1.0_f32, K_LINE);
    v.extreme_bg_color = K_PANEL2;
    v.faint_bg_color = K_ROW;
    v.selection.bg_fill = K_SEL;
    v.selection.stroke = Stroke::new(1.0_f32, K_TEXT);
    v.hyperlink_color = K_BLUE;
    for (w, fill) in [(&mut v.widgets.inactive, K_PANEL2), (&mut v.widgets.hovered, K_LINE), (&mut v.widgets.active, K_SEL), (&mut v.widgets.noninteractive, K_PANEL)] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.rounding = egui::Rounding::same(2.0);
        w.fg_stroke = Stroke::new(1.0_f32, K_TEXT);
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, K_DIM);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, K_LINE);
    ctx.set_visuals(v);
    let mut s = (*ctx.style()).clone();
    s.text_styles.iter_mut().for_each(|(_, f)| f.size *= 1.12);
    s.spacing.button_padding = vec2(12.0, 6.0);
    ctx.set_style(s);
}

/// Músicas ou pastas sendo arrastadas.
#[derive(Clone)]
struct Arrasto(Vec<PathBuf>);

/// Trabalho em segundo plano (copiar e analisar), mostrado embaixo.
#[derive(Default)]
struct Trabalho {
    ativo: bool,
    fase: String,
    feito: usize,
    total: usize,
    atual: String,
    erros: Vec<String>,
    /// Mensagem do fim (fica até o próximo trabalho).
    fim: Option<String>,
    /// Pede para a tela reler a pasta (algo foi copiado).
    mudou: bool,
}

/// Janela de pergunta aberta.
enum Dialogo {
    NovaPasta { pai: PathBuf, texto: String },
    Renomear { caminho: PathBuf, texto: String },
    Apagar { itens: Vec<PathBuf> },
}

struct App {
    pc: Vec<PathBuf>,
    pendrives: Vec<PathBuf>,
    pendrive: Option<PathBuf>,
    arvore: Arvore,
    /// Pasta aberta na lista do meio.
    pasta: Option<PathBuf>,
    /// Pasta do pendrive para onde vão as cópias.
    destino: Option<PathBuf>,
    lista: Vec<Musica>,
    sel: BTreeSet<usize>,
    ancora: usize,
    busca: String,
    trabalho: Arc<Mutex<Trabalho>>,
    parar: Arc<AtomicBool>,
    dialogo: Option<Dialogo>,
    aviso: Option<(String, std::time::Instant)>,
    ultima_busca: std::time::Instant,
    frame: u32,
    foto_pedida: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        install_fonts(&cc.egui_ctx);
        estilo(&cc.egui_ctx);
        let pendrives = drives();
        let mut app = Self {
            pc: pastas_pc(),
            pendrive: pendrives.first().cloned(),
            pendrives,
            arvore: Arvore::default(),
            pasta: None,
            destino: None,
            lista: Vec::new(),
            sel: BTreeSet::new(),
            ancora: 0,
            busca: String::new(),
            trabalho: Arc::default(),
            parar: Arc::default(),
            dialogo: None,
            aviso: None,
            ultima_busca: std::time::Instant::now(),
            frame: 0,
            foto_pedida: false,
        };
        // Testes e prints: DJOS_PREPARAR_PC / DJOS_PREPARAR_PENDRIVE trocam as pastas.
        if let Some(p) = std::env::var_os("DJOS_PREPARAR_PC") {
            app.pc = vec![PathBuf::from(p)];
        }
        if let Some(p) = std::env::var_os("DJOS_PREPARAR_PENDRIVE") {
            app.pendrives = vec![PathBuf::from(p)];
            app.pendrive = app.pendrives.first().cloned();
        }
        app.destino = app.pendrive.clone();
        app.remontar();
        app
    }

    fn avisar(&mut self, msg: impl Into<String>) {
        self.aviso = Some((msg.into(), std::time::Instant::now()));
    }

    fn ocupado(&self) -> bool {
        self.trabalho.lock().unwrap().ativo
    }

    /// Relê a árvore e a pasta aberta (depois de copiar, mover, renomear...).
    fn remontar(&mut self) {
        self.arvore.montar(&self.pc, self.pendrive.as_deref());
        if self.pasta.as_ref().is_some_and(|p| !p.is_dir()) {
            self.pasta = None;
        }
        if self.destino.as_ref().is_some_and(|p| !p.is_dir() || self.pendrive.as_ref().is_none_or(|r| !p.starts_with(r))) {
            self.destino = self.pendrive.clone();
        }
        self.recarregar_lista();
    }

    fn recarregar_lista(&mut self) {
        let raiz = self.pasta.as_ref().and_then(|p| self.pendrive.as_ref().filter(|r| p.starts_with(r)));
        self.lista = self.pasta.as_ref().map_or(Vec::new(), |p| biblioteca::musicas(p, raiz.map(PathBuf::as_path)));
        self.sel.retain(|&i| i < self.lista.len());
    }

    fn abrir_pasta(&mut self, p: PathBuf) {
        if self.pendrive.as_ref().is_some_and(|r| p.starts_with(r)) {
            self.destino = Some(p.clone());
        }
        if self.pasta.as_ref() != Some(&p) {
            self.pasta = Some(p);
            self.sel.clear();
            self.recarregar_lista();
        }
    }

    fn no_pendrive(&self, p: &Path) -> bool {
        self.pendrive.as_ref().is_some_and(|r| p.starts_with(r))
    }

    /// Músicas selecionadas (as que passam na busca).
    fn selecionadas(&self) -> Vec<PathBuf> {
        self.sel.iter().filter_map(|&i| self.lista.get(i)).map(|m| m.caminho.clone()).collect()
    }

    /// Leva músicas ou pastas para uma pasta do pendrive: do computador copia (e analisa); de
    /// dentro do mesmo pendrive, só muda de lugar.
    fn levar(&mut self, itens: Vec<PathBuf>, destino: PathBuf, ctx: &egui::Context) {
        let Some(raiz) = self.pendrive.clone().filter(|r| destino.starts_with(r)) else { return };
        if itens.iter().all(|i| i.starts_with(&raiz)) {
            match biblioteca::mover(&itens, &destino) {
                Ok(n) => self.avisar(format!("{n} movida(s) para {}", nome(&destino))),
                Err(e) => self.avisar(format!("Não deu para mover: {e}")),
            }
            self.remontar();
            return;
        }
        if self.ocupado() {
            self.avisar("Espere a cópia de agora terminar");
            return;
        }
        let plano = biblioteca::planejar(&itens, &destino);
        if plano.is_empty() {
            self.avisar("Nada para copiar: essas músicas já estão lá");
            return;
        }
        self.copiar(raiz, plano, ctx);
    }

    /// Copia (com segurança) e depois analisa o que foi copiado, em segundo plano.
    fn copiar(&mut self, raiz: PathBuf, plano: Vec<Copia>, ctx: &egui::Context) {
        self.parar.store(false, Ordering::Relaxed);
        *self.trabalho.lock().unwrap() = Trabalho { ativo: true, fase: "Copiando".into(), total: plano.len(), ..Default::default() };
        let (t, parar, ctx) = (self.trabalho.clone(), self.parar.clone(), ctx.clone());
        std::thread::spawn(move || {
            let mut copiadas = Vec::new();
            for c in &plano {
                if parar.load(Ordering::Relaxed) {
                    break;
                }
                t.lock().unwrap().atual = nome(&c.de);
                ctx.request_repaint();
                match biblioteca::copiar(c) {
                    Ok(p) => copiadas.push(p),
                    Err(e) => t.lock().unwrap().erros.push(format!("{}: {e}", nome(&c.de))),
                }
                let mut s = t.lock().unwrap();
                s.feito += 1;
                s.mudou = true;
            }
            {
                let mut s = t.lock().unwrap();
                *s = Trabalho { ativo: true, fase: "Analisando".into(), total: copiadas.len(), erros: std::mem::take(&mut s.erros), mudou: true, ..Default::default() };
            }
            ctx.request_repaint();
            let totais = prepare::prepare_files(&raiz, &copiadas, prepare::default_threads(), &parar, |f, out| {
                let mut s = t.lock().unwrap();
                s.feito += 1;
                s.atual = nome(f);
                s.mudou = true;
                if let Outcome::Failed(e) = out {
                    s.erros.push(format!("{}: {e}", nome(f)));
                }
                ctx.request_repaint();
            });
            let mut s = t.lock().unwrap();
            let parado = parar.load(Ordering::Relaxed);
            s.fim = Some(format!(
                "{}{} copiada(s) · {} analisada(s){}",
                if parado { "Parado: " } else { "Pronto: " },
                copiadas.len(),
                totais.done + totais.had,
                if s.erros.is_empty() { String::new() } else { format!(" · {} com erro", s.erros.len()) }
            ));
            s.ativo = false;
            s.mudou = true;
            ctx.request_repaint();
        });
    }

    /// Analisa o pendrive inteiro (o que faltar).
    fn analisar_tudo(&mut self, ctx: &egui::Context) {
        let Some(raiz) = self.pendrive.clone() else { return };
        if self.ocupado() {
            return;
        }
        self.parar.store(false, Ordering::Relaxed);
        *self.trabalho.lock().unwrap() = Trabalho { ativo: true, fase: "Procurando as músicas".into(), ..Default::default() };
        let (t, parar, ctx) = (self.trabalho.clone(), self.parar.clone(), ctx.clone());
        std::thread::spawn(move || {
            let files = prepare::list_tracks(&raiz);
            {
                let mut s = t.lock().unwrap();
                s.fase = "Analisando".into();
                s.total = files.len();
            }
            let totais = prepare::prepare_files(&raiz, &files, prepare::default_threads(), &parar, |f, out| {
                let mut s = t.lock().unwrap();
                s.feito += 1;
                s.atual = nome(f);
                s.mudou = true;
                if let Outcome::Failed(e) = out {
                    s.erros.push(format!("{}: {e}", nome(f)));
                }
                ctx.request_repaint();
            });
            let mut s = t.lock().unwrap();
            s.fim = Some(format!(
                "{}{} analisadas agora · {} já estavam{}",
                if parar.load(Ordering::Relaxed) { "Parado: " } else { "Pronto: " },
                totais.done,
                totais.had,
                if totais.failed > 0 { format!(" · {} com erro", totais.failed) } else { String::new() }
            ));
            s.ativo = false;
            s.mudou = true;
            ctx.request_repaint();
        });
    }
}

/// ✓ desenhado.
fn check(p: &egui::Painter, c: egui::Pos2, s: f32, cor: Color32) {
    let st = Stroke::new(2.0_f32, cor);
    p.line_segment([c + vec2(-s, 0.0), c + vec2(-s * 0.3, s * 0.7)], st);
    p.line_segment([c + vec2(-s * 0.3, s * 0.7), c + vec2(s, -s * 0.8)], st);
}

fn nome(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string())
}

/// Texto que cabe na largura (corta com "…").
fn caber(ui: &egui::Ui, s: &str, font: &FontId, w: f32) -> String {
    let largura = |t: &str| ui.fonts(|f| f.layout_no_wrap(t.to_string(), font.clone(), K_TEXT).size().x);
    if largura(s) <= w {
        return s.to_string();
    }
    let mut chars: Vec<char> = s.chars().collect();
    while !chars.is_empty() && largura(&format!("{}…", chars.iter().collect::<String>())) > w {
        chars.pop();
    }
    format!("{}…", chars.iter().collect::<String>())
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.diagnostics(ctx);
        // Pendrives colocados ou tirados (a cada 2 s, fora de uma cópia).
        if !self.ocupado() && self.ultima_busca.elapsed().as_secs() >= 2 && std::env::var_os("DJOS_PREPARAR_PENDRIVE").is_none() {
            self.ultima_busca = std::time::Instant::now();
            let agora = drives();
            if agora != self.pendrives {
                self.pendrives = agora;
                if self.pendrive.as_ref().is_none_or(|p| !self.pendrives.contains(p)) {
                    self.pendrive = self.pendrives.first().cloned();
                    self.destino = self.pendrive.clone();
                }
                self.remontar();
            }
        }
        // A cópia avançou: relê a pasta aberta (as músicas novas aparecem com o ✓).
        let mudou = std::mem::take(&mut self.trabalho.lock().unwrap().mudou);
        if mudou && self.frame % 10 == 0 || mudou && !self.ocupado() {
            self.remontar();
        } else if mudou {
            self.trabalho.lock().unwrap().mudou = true;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        self.barra_de_cima(ctx);
        self.barra_de_baixo(ctx);
        self.pastas(ctx);
        self.lista_de_musicas(ctx);
        self.dialogos(ctx);
        self.arrasto_visivel(ctx);
    }
}

impl App {
    fn barra_de_cima(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("cima").exact_height(52.0).frame(egui::Frame::none().fill(K_PANEL).inner_margin(egui::Margin::symmetric(14.0, 0.0))).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new("DJ OS").font(cond_bold(22.0)).color(K_TEXT));
                ui.label(egui::RichText::new("PREPARAR").font(cond_bold(22.0)).color(K_BLUE));
                ui.add_space(24.0);
                ui.label(egui::RichText::new("PENDRIVE").font(cond(12.0)).color(K_DIM));
                if self.pendrives.is_empty() {
                    ui.label(egui::RichText::new("nenhum ligado").color(K_DIM));
                }
                let mut trocar = None;
                for d in &self.pendrives {
                    let on = self.pendrive.as_ref() == Some(d);
                    let b = egui::Button::new(egui::RichText::new(format!("💾 {}", nome(d))).font(cond_bold(15.0)).color(K_TEXT)).fill(if on { K_SEL } else { K_PANEL2 }).min_size(vec2(0.0, 32.0));
                    if ui.add(b).clicked() && !on {
                        trocar = Some(d.clone());
                    }
                }
                if let Some(d) = trocar {
                    self.pendrive = Some(d.clone());
                    self.destino = Some(d);
                    self.remontar();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ocupado = self.ocupado();
                    let b = egui::Button::new(egui::RichText::new("ANALISAR O PENDRIVE").font(cond_bold(15.0)).color(K_TEXT)).fill(K_PANEL2).min_size(vec2(0.0, 32.0));
                    if ui.add_enabled(!ocupado && self.pendrive.is_some(), b).on_hover_text("Analisa as músicas do pendrive que ainda não foram analisadas").clicked() {
                        self.analisar_tudo(ctx);
                    }
                });
            });
        });
    }

    fn barra_de_baixo(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("baixo").exact_height(44.0).frame(egui::Frame::none().fill(K_PANEL).inner_margin(egui::Margin::symmetric(14.0, 0.0))).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                let t = self.trabalho.lock().unwrap();
                if t.ativo {
                    let frac = if t.total > 0 { t.feito as f32 / t.total as f32 } else { 0.0 };
                    ui.label(egui::RichText::new(format!("{} {} de {}", t.fase.to_uppercase(), t.feito.min(t.total), t.total)).font(cond_bold(14.0)).color(K_TEXT));
                    ui.add(egui::ProgressBar::new(frac).desired_width(260.0).desired_height(10.0).fill(K_BLUE));
                    ui.label(egui::RichText::new(&t.atual).color(K_DIM));
                    drop(t);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("PARAR").clicked() {
                            self.parar.store(true, Ordering::Relaxed);
                        }
                    });
                    return;
                }
                if let Some(fim) = &t.fim {
                    ui.label(egui::RichText::new(fim).font(cond_bold(14.0)).color(if t.erros.is_empty() { K_GREEN } else { K_ORANGE }));
                    if !t.erros.is_empty() {
                        ui.label(egui::RichText::new("(passe o mouse para ver)").color(K_DIM)).on_hover_text(t.erros.join("\n"));
                    }
                    ui.label(egui::RichText::new("·  antes de tirar o pendrive, use \"Ejetar\"").color(K_DIM));
                } else {
                    ui.label(egui::RichText::new("Arraste músicas ou pastas do computador para uma pasta do pendrive: elas são copiadas e analisadas.").color(K_DIM));
                }
                drop(t);
                if let Some((msg, quando)) = &self.aviso {
                    if quando.elapsed().as_secs() < 5 {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(msg).font(cond_bold(14.0)).color(K_ORANGE));
                        });
                    }
                }
            });
        });
    }

    /// Coluna da esquerda: pastas do computador e do pendrive.
    fn pastas(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("pastas").resizable(true).default_width(330.0).width_range(240.0..=520.0).frame(egui::Frame::none().fill(K_PANEL)).show(ctx, |ui| {
            // Cabeçalho com os botões das pastas do computador.
            let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
            ui.painter().rect_filled(head, 0.0, K_PANEL2);
            ui.painter().text(head.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, "Pastas", cond(13.0), K_DIM);
            let mut acao: Option<Box<dyn FnOnce(&mut App)>> = None;
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                let mut lado_antes = None;
                for i in 0..self.arvore.linhas.len() {
                    let l = self.arvore.linhas[i].clone();
                    // Título de cada lado.
                    if lado_antes != Some(l.lado) && l.nivel == 0 {
                        lado_antes = Some(l.lado);
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.add_space(10.0);
                            let titulo = match l.lado {
                                Lado::Pc => "COMPUTADOR".to_string(),
                                Lado::Pendrive => format!("PENDRIVE  ·  {}", nome(&l.caminho)),
                            };
                            ui.label(egui::RichText::new(titulo).font(cond_bold(13.0)).color(if l.lado == Lado::Pendrive { K_BLUE } else { K_DIM }));
                            if l.lado == Lado::Pc {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("+ pasta").on_hover_text("Mostrar outra pasta do computador aqui").clicked() {
                                        acao = Some(Box::new(|a: &mut App| {
                                            if let Some(p) = rfd::FileDialog::new().set_title("Pasta com músicas").pick_folder() {
                                                if !a.pc.contains(&p) {
                                                    a.pc.push(p);
                                                    salvar_pastas_pc(&a.pc);
                                                    a.remontar();
                                                }
                                            }
                                        }));
                                    }
                                });
                            }
                        });
                    }
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H - 4.0), Sense::click_and_drag());
                    let aberta = self.pasta.as_ref() == Some(&l.caminho);
                    let destino = l.lado == Lado::Pendrive && self.destino.as_ref() == Some(&l.caminho);
                    // Algo sendo arrastado por cima de uma pasta do pendrive: acende.
                    let alvo = l.lado == Lado::Pendrive && resp.dnd_hover_payload::<Arrasto>().is_some();
                    let bg = if alvo {
                        K_GREEN.gamma_multiply(0.35)
                    } else if aberta {
                        K_SEL
                    } else if resp.hovered() {
                        K_PANEL2
                    } else {
                        K_PANEL
                    };
                    let p = ui.painter();
                    p.rect_filled(r, 0.0, bg);
                    let x = r.left() + 10.0 + l.nivel as f32 * 16.0;
                    if l.tem_sub {
                        let c = pos2(x + 5.0, r.center().y);
                        let tri = if l.aberta { vec![c + vec2(-5.0, -3.0), c + vec2(5.0, -3.0), c + vec2(0.0, 4.0)] } else { vec![c + vec2(-3.0, -5.0), c + vec2(4.0, 0.0), c + vec2(-3.0, 5.0)] };
                        p.add(egui::Shape::convex_polygon(tri, K_DIM, Stroke::NONE));
                    }
                    let icone = match (l.nivel, l.lado) {
                        (0, Lado::Pendrive) => "💾",
                        (0, Lado::Pc) => "🖥",
                        _ => "📁",
                    };
                    p.text(pos2(x + 16.0, r.center().y), Align2::LEFT_CENTER, icone, cond(14.0), K_DIM);
                    let w = r.right() - (x + 38.0) - 8.0;
                    let texto = caber(ui, &l.nome, &cond(16.0), w);
                    p.text(pos2(x + 38.0, r.center().y), Align2::LEFT_CENTER, texto, cond(16.0), K_TEXT);
                    if destino && !aberta {
                        p.rect_filled(Rect::from_min_size(r.min, vec2(3.0, r.height())), 0.0, K_GREEN);
                    }
                    // Clique na seta abre/fecha; no nome, mostra as músicas; dois cliques abre/fecha.
                    if resp.clicked() {
                        let na_seta = resp.interact_pointer_pos().is_some_and(|pt| pt.x < x + 14.0);
                        let caminho = l.caminho.clone();
                        acao = Some(Box::new(move |a: &mut App| {
                            if na_seta {
                                a.arvore.alternar(i);
                            } else {
                                a.abrir_pasta(caminho);
                            }
                        }));
                    }
                    if resp.double_clicked() {
                        acao = Some(Box::new(move |a: &mut App| a.arvore.alternar(i)));
                    }
                    // Pastas também se arrastam (do computador: copia a pasta inteira).
                    if l.nivel > 0 || l.lado == Lado::Pc {
                        resp.dnd_set_drag_payload(Arrasto(vec![l.caminho.clone()]));
                    }
                    if let (Some(itens), Lado::Pendrive) = (resp.dnd_release_payload::<Arrasto>(), l.lado) {
                        let destino = l.caminho.clone();
                        let itens = itens.0.clone();
                        let ctx = ui.ctx().clone();
                        acao = Some(Box::new(move |a: &mut App| a.levar(itens, destino, &ctx)));
                    }
                    // Botão direito: o que dá para fazer com a pasta.
                    let caminho = l.caminho.clone();
                    let lado = l.lado;
                    resp.context_menu(|ui| {
                        if lado == Lado::Pendrive {
                            if ui.button("Nova pasta aqui dentro").clicked() {
                                let c = caminho.clone();
                                acao = Some(Box::new(move |a: &mut App| a.dialogo = Some(Dialogo::NovaPasta { pai: c, texto: String::new() })));
                                ui.close_menu();
                            }
                            if l.nivel > 0 {
                                if ui.button("Renomear").clicked() {
                                    let c = caminho.clone();
                                    acao = Some(Box::new(move |a: &mut App| a.dialogo = Some(Dialogo::Renomear { texto: nome(&c), caminho: c })));
                                    ui.close_menu();
                                }
                                if ui.button("Apagar a pasta").clicked() {
                                    let c = caminho.clone();
                                    acao = Some(Box::new(move |a: &mut App| a.dialogo = Some(Dialogo::Apagar { itens: vec![c] })));
                                    ui.close_menu();
                                }
                            }
                        } else {
                            if ui.button("Copiar a pasta para o pendrive").clicked() {
                                let c = caminho.clone();
                                let ctx = ui.ctx().clone();
                                acao = Some(Box::new(move |a: &mut App| {
                                    if let Some(d) = a.destino.clone() {
                                        a.levar(vec![c], d, &ctx);
                                    }
                                }));
                                ui.close_menu();
                            }
                            if l.nivel == 0 && ui.button("Tirar desta lista").clicked() {
                                let c = caminho.clone();
                                acao = Some(Box::new(move |a: &mut App| {
                                    a.pc.retain(|p| *p != c);
                                    salvar_pastas_pc(&a.pc);
                                    a.remontar();
                                }));
                                ui.close_menu();
                            }
                        }
                    });
                }
                if self.pendrive.is_none() {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("Coloque um pendrive para aparecer aqui").color(K_DIM));
                    });
                }
            });
            if let Some(f) = acao {
                f(self);
            }
        });
    }

    /// Meio: as músicas da pasta aberta.
    fn lista_de_musicas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(egui::Frame::none().fill(K_BG)).show(ctx, |ui| {
            let Some(pasta) = self.pasta.clone() else {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("Escolha uma pasta à esquerda").font(cond(18.0)).color(K_DIM));
                });
                return;
            };
            let no_pd = self.no_pendrive(&pasta);
            // ---- caminho e botões ----
            egui::Frame::none().fill(K_PANEL).inner_margin(egui::Margin::symmetric(12.0, 8.0)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let raiz = self.pc.iter().chain(self.pendrive.iter()).filter(|r| pasta.starts_with(r)).max_by_key(|r| r.as_os_str().len());
                    let caminho = match raiz {
                        Some(r) => std::iter::once(nome(r)).chain(pasta.strip_prefix(r).unwrap_or(&pasta).iter().map(|s| s.to_string_lossy().to_string())).collect::<Vec<_>>().join("  ›  "),
                        None => pasta.display().to_string(),
                    };
                    ui.label(egui::RichText::new("BROWSE").font(cond_bold(18.0)).color(K_TEXT));
                    ui.label(egui::RichText::new(caminho).font(cond(15.0)).color(K_DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.busca).hint_text("🔍 procurar nesta pasta").desired_width(200.0));
                        let n = self.sel.len();
                        if no_pd {
                            if ui.add_enabled(n > 0, egui::Button::new("Apagar")).clicked() {
                                self.dialogo = Some(Dialogo::Apagar { itens: self.selecionadas() });
                            }
                            if ui.add_enabled(n == 1, egui::Button::new("Renomear")).clicked() {
                                if let Some(c) = self.selecionadas().pop() {
                                    self.dialogo = Some(Dialogo::Renomear { texto: c.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), caminho: c });
                                }
                            }
                            if ui.button("Nova pasta").clicked() {
                                self.dialogo = Some(Dialogo::NovaPasta { pai: pasta.clone(), texto: String::new() });
                            }
                        } else if let Some(destino) = self.destino.clone() {
                            let texto = if n > 0 { format!("COPIAR {n} PARA O PENDRIVE  ·  {}", nome(&destino)) } else { format!("COPIAR A PASTA PARA O PENDRIVE  ·  {}", nome(&destino)) };
                            let b = egui::Button::new(egui::RichText::new(texto).font(cond_bold(14.0)).color(K_TEXT)).fill(K_SEL);
                            if ui.add_enabled(!self.ocupado(), b).on_hover_text("Copia para a pasta do pendrive marcada em verde (clique numa pasta do pendrive para trocar)").clicked() {
                                let itens = if n > 0 { self.selecionadas() } else { vec![pasta.clone()] };
                                self.levar(itens, destino, ctx);
                            }
                        }
                    });
                });
            });
            // ---- cabeçalho das colunas ----
            let w = ui.available_width();
            let (head, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(head, 0.0, K_PANEL2);
            let (x_ok, x_bpm, x_tom) = (head.right() - 40.0, head.right() - 180.0, head.right() - 110.0);
            p.text(head.left_center() + vec2(40.0, 0.0), Align2::LEFT_CENTER, format!("Música  [{:04}]", self.lista.len()), cond(13.0), K_BLUE);
            p.text(pos2(x_tom - 12.0, head.center().y), Align2::RIGHT_CENTER, "BPM", cond(13.0), K_DIM);
            p.text(pos2(x_tom + 10.0, head.center().y), Align2::LEFT_CENTER, "Tom", cond(13.0), K_DIM);
            check(p, pos2(x_ok + 20.0, head.center().y), 5.0, K_DIM);
            let busca = self.busca.to_lowercase();
            let mostrar: Vec<usize> = (0..self.lista.len()).filter(|&i| busca.is_empty() || format!("{} {}", self.lista[i].titulo, self.lista[i].artista).to_lowercase().contains(&busca)).collect();
            let mut clique = None;
            let mut duplo = None;
            egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, ROW_H, mostrar.len(), |ui, faixa| {
                for k in faixa {
                    let i = mostrar[k];
                    let m = &self.lista[i];
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
                    let sel = self.sel.contains(&i);
                    let p = ui.painter();
                    p.rect_filled(r, 0.0, if sel { K_SEL } else if k % 2 == 0 { K_ROW } else { K_BG });
                    p.text(r.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, "♪", cond(15.0), K_DIM);
                    let nome_m = if m.artista.is_empty() { m.titulo.clone() } else { format!("{}  ·  {}", m.titulo, m.artista) };
                    let texto = caber(ui, &nome_m, &cond(17.0), x_bpm - r.left() - 50.0);
                    p.text(r.left_center() + vec2(40.0, 0.0), Align2::LEFT_CENTER, texto, cond(17.0), K_TEXT);
                    if let Some(b) = m.bpm {
                        p.text(pos2(x_tom - 12.0, r.center().y), Align2::RIGHT_CENTER, format!("{b:.1}"), cond(17.0), K_TEXT);
                    }
                    if let Some(t) = m.tom {
                        p.text(pos2(x_tom + 10.0, r.center().y), Align2::LEFT_CENTER, dj_os_analise::key::label(t), cond(17.0), K_TEXT);
                    }
                    // Analisada: ✓ verde (desenhado: a fonte não tem o símbolo); senão, um ponto.
                    if m.analisada {
                        check(p, pos2(x_ok + 20.0, r.center().y), 6.0, K_GREEN);
                    } else {
                        p.circle_filled(pos2(x_ok + 20.0, r.center().y), 2.0, K_DIM);
                    }
                    if resp.clicked() {
                        clique = Some(i);
                    }
                    if resp.double_clicked() {
                        duplo = Some(i);
                    }
                    if resp.drag_started() && !sel {
                        clique = Some(i);
                    }
                    let itens = if sel { self.selecionadas() } else { vec![m.caminho.clone()] };
                    resp.dnd_set_drag_payload(Arrasto(itens));
                }
            });
            if mostrar.is_empty() {
                ui.label(egui::RichText::new(if self.lista.is_empty() { "Nenhuma música nesta pasta" } else { "Nada encontrado" }).color(K_DIM));
            }
            // Clique: seleciona (Ctrl soma, Shift pega o intervalo).
            if let Some(i) = clique {
                let m = ctx.input(|inp| inp.modifiers);
                if m.shift {
                    let (a, b) = (self.ancora.min(i), self.ancora.max(i));
                    self.sel.extend(mostrar.iter().copied().filter(|&j| j >= a && j <= b));
                } else if m.command {
                    if !self.sel.remove(&i) {
                        self.sel.insert(i);
                    }
                    self.ancora = i;
                } else {
                    self.sel = BTreeSet::from([i]);
                    self.ancora = i;
                }
            }
            // Dois cliques numa música do computador: copia para o pendrive.
            if let (Some(i), false) = (duplo, no_pd) {
                if let (Some(d), Some(m)) = (self.destino.clone(), self.lista.get(i)) {
                    let c = m.caminho.clone();
                    self.levar(vec![c], d, ctx);
                }
            }
            // Teclado: Ctrl+A seleciona tudo; Delete apaga (só no pendrive); F2 renomeia.
            if self.dialogo.is_none() && !ctx.wants_keyboard_input() {
                let (todos, del, f2) = ctx.input(|i| (i.modifiers.command && i.key_pressed(egui::Key::A), i.key_pressed(egui::Key::Delete), i.key_pressed(egui::Key::F2)));
                if todos {
                    self.sel = mostrar.iter().copied().collect();
                }
                if del && no_pd && !self.sel.is_empty() {
                    self.dialogo = Some(Dialogo::Apagar { itens: self.selecionadas() });
                }
                if f2 && no_pd && self.sel.len() == 1 {
                    if let Some(c) = self.selecionadas().pop() {
                        self.dialogo = Some(Dialogo::Renomear { texto: c.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), caminho: c });
                    }
                }
            }
        });
    }

    fn dialogos(&mut self, ctx: &egui::Context) {
        let Some(d) = self.dialogo.as_mut() else { return };
        let mut fechar = false;
        let mut feito: Option<Result<String, String>> = None;
        let titulo = match d {
            Dialogo::NovaPasta { .. } => "Nova pasta",
            Dialogo::Renomear { .. } => "Renomear",
            Dialogo::Apagar { .. } => "Apagar do pendrive",
        };
        egui::Window::new(titulo).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_min_width(380.0);
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            match d {
                Dialogo::NovaPasta { pai, texto } => {
                    ui.label(format!("Dentro de \"{}\":", nome(pai)));
                    ui.text_edit_singleline(texto).request_focus();
                    ui.horizontal(|ui| {
                        if ui.button("Criar").clicked() || enter {
                            feito = Some(biblioteca::criar_pasta(pai, texto).map(|p| format!("Pasta \"{}\" criada", nome(&p))).map_err(|e| e.to_string()));
                        }
                        fechar |= ui.button("Cancelar").clicked();
                    });
                }
                Dialogo::Renomear { caminho, texto } => {
                    ui.text_edit_singleline(texto).request_focus();
                    ui.horizontal(|ui| {
                        if ui.button("Renomear").clicked() || enter {
                            feito = Some(biblioteca::renomear(caminho, texto).map(|p| format!("Agora é \"{}\"", nome(&p))).map_err(|e| e.to_string()));
                        }
                        fechar |= ui.button("Cancelar").clicked();
                    });
                }
                Dialogo::Apagar { itens } => {
                    let pastas = itens.iter().filter(|p| p.is_dir()).count();
                    let texto = match (itens.len(), pastas) {
                        (1, 1) => format!("Apagar a pasta \"{}\" e tudo o que tem dentro?", nome(&itens[0])),
                        (1, _) => format!("Apagar \"{}\" do pendrive?", nome(&itens[0])),
                        (n, _) => format!("Apagar {n} itens do pendrive?"),
                    };
                    ui.label(texto);
                    ui.label(egui::RichText::new("Só sai do pendrive: o que está no computador não é mexido.").color(K_DIM));
                    ui.horizontal(|ui| {
                        let b = egui::Button::new(egui::RichText::new("Apagar").color(K_TEXT)).fill(K_RED);
                        if ui.add(b).clicked() {
                            feito = Some(biblioteca::apagar(itens).map(|_| "Apagado".to_string()).map_err(|e| e.to_string()));
                        }
                        fechar |= ui.button("Cancelar").clicked();
                    });
                }
            }
            fechar |= ui.input(|i| i.key_pressed(egui::Key::Escape));
        });
        if let Some(r) = feito {
            match r {
                Ok(msg) => self.avisar(msg),
                Err(e) => self.avisar(format!("Não deu: {e}")),
            }
            self.sel.clear();
            fechar = true;
            self.remontar();
        }
        if fechar {
            self.dialogo = None;
        }
    }

    /// Enquanto arrasta: o nome do que vai junto, ao lado do ponteiro.
    fn arrasto_visivel(&self, ctx: &egui::Context) {
        let Some(a) = egui::DragAndDrop::payload::<Arrasto>(ctx) else { return };
        let Some(pt) = ctx.pointer_interact_pos() else { return };
        let texto = match a.0.as_slice() {
            [um] => nome(um),
            v => format!("{} itens", v.len()),
        };
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("arrasto")));
        let g = p.layout_no_wrap(texto, cond_bold(15.0), K_TEXT);
        let r = Rect::from_min_size(pt + vec2(16.0, 10.0), g.size() + vec2(20.0, 10.0));
        p.rect_filled(r, 3.0, K_SEL);
        p.galley(r.min + vec2(10.0, 5.0), g, K_TEXT);
    }

    /// Para o README e testes: DJOS_PREPARAR_PASTA=<pasta> já abre essa pasta (com duas músicas
    /// selecionadas); DJOS_PREPARAR_COPIAR=<pasta do pendrive> copia essas duas para lá;
    /// DJOS_SCREENSHOT=<arquivo> salva a tela (RGBA cru com cabeçalho "largura altura\n") depois
    /// que a cópia termina.
    fn diagnostics(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        if self.frame == 2 {
            if let Some(p) = std::env::var_os("DJOS_PREPARAR_PASTA").map(PathBuf::from) {
                self.arvore.revelar(&p);
                self.abrir_pasta(p);
                self.sel = BTreeSet::from([1, 2]);
            }
        }
        if self.frame == 5 {
            if let Some(d) = std::env::var_os("DJOS_PREPARAR_COPIAR").map(PathBuf::from) {
                let itens = self.selecionadas();
                self.levar(itens, d, ctx);
            }
        }
        let Some(path) = std::env::var_os("DJOS_SCREENSHOT") else { return };
        ctx.request_repaint();
        if self.frame >= 90 && !self.ocupado() && !self.foto_pedida {
            self.foto_pedida = true;
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

// ------------------------------------------------------------------ pastas do computador

/// Onde fica a lista das pastas do computador mostradas.
fn arquivo_pastas() -> Option<PathBuf> {
    let base = if cfg!(windows) { std::env::var_os("APPDATA").map(PathBuf::from) } else { std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))) };
    base.map(|b| b.join("dj-os-preparar").join("pastas.txt"))
}

/// As pastas do computador escolhidas (na primeira vez, a pasta de músicas do usuário).
fn pastas_pc() -> Vec<PathBuf> {
    if let Some(t) = arquivo_pastas().and_then(|f| std::fs::read_to_string(f).ok()) {
        let v: Vec<PathBuf> = t.lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from).filter(|p| p.is_dir()).collect();
        if !v.is_empty() {
            return v;
        }
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    home.map(|h| ["Música", "Music", "Músicas"].iter().map(|n| h.join(n)).filter(|p| p.is_dir()).take(1).collect()).unwrap_or_default()
}

fn salvar_pastas_pc(v: &[PathBuf]) {
    if let Some(f) = arquivo_pastas() {
        let _ = std::fs::create_dir_all(f.parent().unwrap_or(Path::new(".")));
        let _ = std::fs::write(f, v.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n"));
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
