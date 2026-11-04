//! DJ OS Preparar: o "rekordbox" do DJ OS. Roda no computador de casa (Windows ou Linux), dividido
//! no meio: à esquerda o computador (escolhe o disco e anda pelas pastas), à direita o pendrive.
//! Arrastar de um lado para o outro copia (com segurança) e já analisa; dentro do pendrive dá para
//! criar pasta, renomear, mover e apagar. Embaixo, o player: ouve a música, pula para qualquer
//! ponto e marca os hot cues A–H e o CUE. A análise vai para dentro do pendrive
//! (`DJOS/analise/`): no aparelho as músicas já entram no deck analisadas, com os cues.

#![cfg_attr(windows, windows_subsystem = "windows")] // sem a janela preta do terminal no Windows

mod biblioteca;
mod player;

use biblioteca::{Copia, Item, Lado};
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
            .with_inner_size([1280.0, 780.0])
            .with_min_inner_size([900.0, 560.0]),
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
const K_SEL_OFF: Color32 = Color32::from_rgb(38, 58, 92);
const K_GREEN: Color32 = Color32::from_rgb(60, 210, 90);
const K_RED: Color32 = Color32::from_rgb(230, 30, 40);
const K_ORANGE: Color32 = Color32::from_rgb(255, 140, 20);
/// Waveform como no aparelho: grave azul, médio âmbar, agudo creme.
const BANDAS: [Color32; 3] = [Color32::from_rgb(25, 100, 240), Color32::from_rgb(235, 140, 30), Color32::from_rgb(250, 238, 218)];
/// Cores dos hot cues A–H (as do aparelho).
const CUE_CORES: [Color32; 8] = [
    Color32::from_rgb(235, 60, 60),
    Color32::from_rgb(40, 190, 230),
    Color32::from_rgb(70, 205, 80),
    Color32::from_rgb(140, 80, 245),
    Color32::from_rgb(130, 220, 60),
    Color32::from_rgb(255, 130, 30),
    Color32::from_rgb(50, 95, 255),
    Color32::from_rgb(240, 205, 40),
];
const CUE_LETRAS: [&str; 8] = ["A", "B", "C", "D", "E", "F", "G", "H"];

const ROW_H: f32 = 32.0;

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
    /// Pede para a tela reler as pastas (algo foi copiado).
    mudou: bool,
}

/// Janela de pergunta aberta.
enum Dialogo {
    NovaPasta { pai: PathBuf, texto: String },
    Renomear { caminho: PathBuf, texto: String },
    Apagar { itens: Vec<PathBuf> },
}

/// Um lado da tela: o computador ou o pendrive.
struct Painel {
    lado: Lado,
    /// Começo do disco (ou do pendrive): não se volta além dele.
    raiz: Option<PathBuf>,
    pasta: Option<PathBuf>,
    itens: Vec<Item>,
    sel: BTreeSet<usize>,
    ancora: usize,
    busca: String,
}

impl Painel {
    fn novo(lado: Lado) -> Self {
        Self { lado, raiz: None, pasta: None, itens: Vec::new(), sel: BTreeSet::new(), ancora: 0, busca: String::new() }
    }

    /// Pendrive deste lado (para saber se a música já está analisada nele).
    fn pendrive(&self) -> Option<&Path> {
        (self.lado == Lado::Pendrive).then_some(self.raiz.as_deref()).flatten()
    }

    fn entrar(&mut self, raiz: Option<PathBuf>, pasta: Option<PathBuf>) {
        self.raiz = raiz;
        self.pasta = pasta.or_else(|| self.raiz.clone());
        self.sel.clear();
        self.busca.clear();
        self.recarregar();
    }

    fn abrir(&mut self, pasta: PathBuf) {
        self.pasta = Some(pasta);
        self.sel.clear();
        self.busca.clear();
        self.recarregar();
    }

    fn recarregar(&mut self) {
        if self.pasta.as_ref().is_some_and(|p| !p.is_dir()) {
            // A pasta sumiu (apagada, pendrive tirado): sobe até uma que existe.
            self.pasta = self.pasta.as_ref().and_then(|p| p.ancestors().find(|a| a.is_dir() && self.raiz.as_ref().is_some_and(|r| a.starts_with(r))).map(Path::to_path_buf));
        }
        self.itens = match (&self.pasta, &self.raiz) {
            (Some(p), Some(r)) => biblioteca::conteudo(p, r, self.pendrive()),
            _ => Vec::new(),
        };
        self.sel.retain(|&i| i < self.itens.len());
    }

    fn voltar(&mut self) {
        let (Some(p), Some(r)) = (&self.pasta, &self.raiz) else { return };
        if p != r {
            if let Some(pai) = p.parent().map(Path::to_path_buf) {
                let veio = p.clone();
                self.abrir(pai);
                // Deixa selecionada a pasta de onde voltou.
                if let Some(i) = self.itens.iter().position(|it| it.caminho() == Some(veio.as_path())) {
                    self.sel.insert(i);
                    self.ancora = i;
                }
            }
        }
    }

    fn selecionados(&self) -> Vec<PathBuf> {
        self.sel.iter().filter_map(|&i| self.itens.get(i)).filter_map(|it| it.caminho().map(Path::to_path_buf)).collect()
    }
}

struct App {
    pc: Painel,
    pd: Painel,
    discos: Vec<(String, PathBuf)>,
    pendrives: Vec<PathBuf>,
    /// Lado em que o teclado age (o último clicado).
    ativo: Lado,
    player: player::Player,
    trabalho: Arc<Mutex<Trabalho>>,
    parar: Arc<AtomicBool>,
    dialogo: Option<Dialogo>,
    aviso: Option<(String, std::time::Instant)>,
    ultima_busca: std::time::Instant,
    /// Onde dá para soltar o que se arrasta neste quadro: as pastas do pendrive e o lado dele.
    alvos: Vec<(Rect, PathBuf)>,
    frame: u32,
    foto_pedida: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        install_fonts(&cc.egui_ctx);
        estilo(&cc.egui_ctx);
        let mut app = Self {
            pc: Painel::novo(Lado::Pc),
            pd: Painel::novo(Lado::Pendrive),
            discos: biblioteca::discos(),
            pendrives: drives(),
            ativo: Lado::Pc,
            player: player::Player::default(),
            trabalho: Arc::default(),
            parar: Arc::default(),
            dialogo: None,
            aviso: None,
            ultima_busca: std::time::Instant::now(),
            alvos: Vec::new(),
            frame: 0,
            foto_pedida: false,
        };
        // Testes e prints: DJOS_PREPARAR_PC / DJOS_PREPARAR_PENDRIVE trocam as pastas.
        if let Some(p) = std::env::var_os("DJOS_PREPARAR_PC").map(PathBuf::from) {
            app.discos = vec![("Teste".into(), p)];
        }
        if let Some(p) = std::env::var_os("DJOS_PREPARAR_PENDRIVE").map(PathBuf::from) {
            app.pendrives = vec![p];
        }
        // O computador abre onde parou (ou nas Músicas).
        let ultima = estado_pc().filter(|p| p.is_dir());
        let raiz = ultima.as_ref().and_then(|u| app.discos.iter().filter(|(_, d)| u.starts_with(d)).max_by_key(|(_, d)| d.as_os_str().len())).map(|(_, d)| d.clone());
        match raiz {
            Some(r) => app.pc.entrar(Some(r), ultima),
            None => app.pc.entrar(app.discos.first().map(|(_, d)| d.clone()), None),
        }
        app.pd.entrar(app.pendrives.first().cloned(), None);
        app
    }

    fn avisar(&mut self, msg: impl Into<String>) {
        self.aviso = Some((msg.into(), std::time::Instant::now()));
    }

    fn ocupado(&self) -> bool {
        self.trabalho.lock().unwrap().ativo
    }

    fn painel(&mut self, lado: Lado) -> &mut Painel {
        match lado {
            Lado::Pc => &mut self.pc,
            Lado::Pendrive => &mut self.pd,
        }
    }

    fn recarregar(&mut self) {
        self.pc.recarregar();
        self.pd.recarregar();
    }

    /// Leva músicas ou pastas para uma pasta do pendrive: do computador copia (e analisa); de
    /// dentro do mesmo pendrive, só muda de lugar.
    fn levar(&mut self, itens: Vec<PathBuf>, destino: PathBuf, ctx: &egui::Context) {
        let Some(raiz) = self.pd.raiz.clone().filter(|r| destino.starts_with(r)) else { return };
        if itens.is_empty() {
            return;
        }
        if itens.iter().all(|i| i.starts_with(&raiz)) {
            match biblioteca::mover(&itens, &destino) {
                Ok(0) => {}
                Ok(n) => self.avisar(format!("{n} movida(s) para {}", nome(&destino))),
                Err(e) => self.avisar(format!("Não deu para mover: {e}")),
            }
            self.recarregar();
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
                let erros = std::mem::take(&mut s.erros);
                *s = Trabalho { ativo: true, fase: "Analisando".into(), total: copiadas.len(), erros, mudou: true, ..Default::default() };
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
        let Some(raiz) = self.pd.raiz.clone() else { return };
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

    fn ejetar(&mut self) {
        let Some(r) = self.pd.raiz.clone() else { return };
        if self.ocupado() {
            self.avisar("Espere a cópia terminar antes de ejetar");
            return;
        }
        if self.player.aberta.as_ref().is_some_and(|a| a.caminho.starts_with(&r)) {
            self.player.pausar();
            self.player.aberta = None;
        }
        match biblioteca::ejetar(&r) {
            Ok(()) => {
                self.avisar(format!("{} pode ser tirado", nome(&r)));
                self.pendrives.retain(|p| *p != r);
                self.pd.entrar(self.pendrives.first().cloned(), None);
            }
            Err(e) => self.avisar(format!("Não deu para ejetar: {e}")),
        }
    }
}

fn nome(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string())
}

/// ✓ desenhado (a fonte não tem o símbolo).
fn check(p: &egui::Painter, c: egui::Pos2, s: f32, cor: Color32) {
    let st = Stroke::new(2.0_f32, cor);
    p.line_segment([c + vec2(-s, 0.0), c + vec2(-s * 0.3, s * 0.7)], st);
    p.line_segment([c + vec2(-s * 0.3, s * 0.7), c + vec2(s, -s * 0.8)], st);
}

/// Seta para a esquerda desenhada.
fn seta_esq(p: &egui::Painter, c: egui::Pos2, s: f32, cor: Color32) {
    let st = Stroke::new(2.0_f32, cor);
    p.line_segment([c + vec2(-s, 0.0), c + vec2(s, 0.0)], st);
    p.line_segment([c + vec2(-s, 0.0), c + vec2(-s * 0.4, -s * 0.6)], st);
    p.line_segment([c + vec2(-s, 0.0), c + vec2(-s * 0.4, s * 0.6)], st);
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

/// Tempo "m:ss".
fn tempo(s: f64) -> String {
    let s = s.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Altura (0 a 1) de cada camada da waveform, empilhadas como no aparelho: agudo no centro,
/// médio em volta, grave por fora.
fn camadas(l: f32, m: f32, h: f32) -> [f32; 3] {
    let f = |v: f32| v.clamp(0.0, 1.0).powf(0.75);
    let hi = f(h) * 0.30;
    let mi = hi + f(m) * 0.30;
    let lo = mi + f(l) * 0.55;
    [lo.min(1.0), mi.min(1.0), hi.min(1.0)]
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.diagnostics(ctx);
        self.player.tick();
        // Pendrives colocados ou tirados (a cada 2 s, fora de uma cópia).
        if !self.ocupado() && self.ultima_busca.elapsed().as_secs() >= 2 && std::env::var_os("DJOS_PREPARAR_PENDRIVE").is_none() {
            self.ultima_busca = std::time::Instant::now();
            let agora = drives();
            if agora != self.pendrives {
                self.pendrives = agora;
                if self.pd.raiz.as_ref().is_none_or(|p| !self.pendrives.contains(p)) {
                    self.pd.entrar(self.pendrives.first().cloned(), None);
                }
                self.discos = biblioteca::discos();
            }
        }
        // A cópia avançou: relê as pastas (as músicas novas aparecem com o ✓).
        let mudou = std::mem::take(&mut self.trabalho.lock().unwrap().mudou);
        if mudou {
            if self.frame % 10 == 0 || !self.ocupado() {
                self.recarregar();
            } else {
                self.trabalho.lock().unwrap().mudou = true;
            }
        }
        if self.player.tocando() {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        self.alvos.clear();
        self.barra_de_cima(ctx);
        self.barra_de_baixo(ctx);
        self.player_ui(ctx);
        egui::CentralPanel::default().frame(egui::Frame::none().fill(K_BG).inner_margin(egui::Margin::same(6.0))).show(ctx, |ui| {
            ui.columns(2, |c| {
                self.lado_ui(&mut c[0], Lado::Pc, ctx);
                self.lado_ui(&mut c[1], Lado::Pendrive, ctx);
            });
        });
        self.soltar(ctx);
        self.teclado(ctx);
        self.dialogos(ctx);
        self.arrasto_visivel(ctx);
    }
}

impl App {
    fn barra_de_cima(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("cima").exact_height(48.0).frame(egui::Frame::none().fill(K_PANEL).inner_margin(egui::Margin::symmetric(14.0, 0.0))).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new("DJ OS").font(cond_bold(22.0)).color(K_TEXT));
                ui.label(egui::RichText::new("PREPARAR").font(cond_bold(22.0)).color(K_BLUE));
                ui.add_space(16.0);
                ui.label(egui::RichText::new("Arraste do computador para o pendrive: copia e já analisa. Dois cliques numa música: ouvir e marcar os cues.").color(K_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let b = egui::Button::new(egui::RichText::new("ANALISAR O PENDRIVE").font(cond_bold(14.0)).color(K_TEXT)).fill(K_PANEL2).min_size(vec2(0.0, 30.0));
                    if ui.add_enabled(!self.ocupado() && self.pd.raiz.is_some(), b).on_hover_text("Analisa as músicas do pendrive que ainda não foram analisadas").clicked() {
                        self.analisar_tudo(ctx);
                    }
                });
            });
        });
    }

    fn barra_de_baixo(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("baixo").exact_height(36.0).frame(egui::Frame::none().fill(K_PANEL).inner_margin(egui::Margin::symmetric(14.0, 0.0))).show(ctx, |ui| {
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
                    ui.label(egui::RichText::new("·  antes de tirar o pendrive, aperte EJETAR").color(K_DIM));
                }
                drop(t);
                if let Some((msg, quando)) = &self.aviso {
                    if quando.elapsed().as_secs() < 6 {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(msg).font(cond_bold(14.0)).color(K_ORANGE));
                        });
                    }
                }
            });
        });
    }

    /// Um lado: o computador (escolhe o disco) ou o pendrive (escolhe o pendrive, ejeta).
    fn lado_ui(&mut self, ui: &mut egui::Ui, lado: Lado, ctx: &egui::Context) {
        let area = ui.max_rect();
        ui.painter().rect_filled(area, 0.0, K_BG);
        // ---- cabeçalho: de onde e onde ----
        egui::Frame::none().fill(K_PANEL).inner_margin(egui::Margin::symmetric(10.0, 6.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                match lado {
                    Lado::Pc => {
                        ui.label(egui::RichText::new("COMPUTADOR").font(cond_bold(16.0)).color(K_TEXT));
                        let atual = self.pc.raiz.as_ref().and_then(|r| self.discos.iter().find(|(_, d)| d == r)).map_or("escolha".to_string(), |(n, _)| n.clone());
                        let mut escolher = None;
                        egui::ComboBox::from_id_salt("disco").selected_text(atual).width(220.0).show_ui(ui, |ui| {
                            for (n, d) in &self.discos {
                                if ui.selectable_label(self.pc.raiz.as_ref() == Some(d), n).clicked() {
                                    escolher = Some(d.clone());
                                }
                            }
                        });
                        if let Some(d) = escolher {
                            self.pc.entrar(Some(d), None);
                            salvar_estado_pc(self.pc.pasta.as_deref());
                        }
                    }
                    Lado::Pendrive => {
                        ui.label(egui::RichText::new("PENDRIVE").font(cond_bold(16.0)).color(K_BLUE));
                        if self.pendrives.is_empty() {
                            ui.label(egui::RichText::new("coloque um pendrive").color(K_DIM));
                        }
                        let mut trocar = None;
                        for d in &self.pendrives {
                            let on = self.pd.raiz.as_ref() == Some(d);
                            let b = egui::Button::new(egui::RichText::new(nome(d)).font(cond_bold(14.0)).color(K_TEXT)).fill(if on { K_SEL } else { K_PANEL2 });
                            if ui.add(b).clicked() && !on {
                                trocar = Some(d.clone());
                            }
                        }
                        if let Some(d) = trocar {
                            self.pd.entrar(Some(d), None);
                        }
                        if self.pd.raiz.is_some() {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let b = egui::Button::new(egui::RichText::new("EJETAR").font(cond_bold(14.0)).color(K_TEXT)).fill(K_PANEL2);
                                if ui.add_enabled(!self.ocupado(), b).on_hover_text("Tira o pendrive com segurança").clicked() {
                                    self.ejetar();
                                }
                            });
                        }
                    }
                }
            });
            // Caminho e o que dá para fazer aqui.
            ui.horizontal(|ui| {
                let p = self.painel(lado);
                let caminho = match (&p.pasta, &p.raiz) {
                    (Some(pasta), Some(r)) => std::iter::once(nome(r)).chain(pasta.strip_prefix(r).unwrap_or(pasta).iter().map(|s| s.to_string_lossy().to_string())).collect::<Vec<_>>().join("  ›  "),
                    _ => String::new(),
                };
                ui.label(egui::RichText::new("BROWSE").font(cond_bold(15.0)).color(K_TEXT));
                let w = (ui.available_width() - 330.0).max(60.0);
                ui.label(egui::RichText::new(caber(ui, &caminho, &cond(14.0), w)).font(cond(14.0)).color(K_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.painel(lado).busca).hint_text("procurar aqui").desired_width(110.0));
                    let n = self.painel(lado).sel.len();
                    match lado {
                        Lado::Pendrive if self.pd.pasta.is_some() => {
                            if ui.add_enabled(n > 0, egui::Button::new("Apagar")).clicked() {
                                self.dialogo = Some(Dialogo::Apagar { itens: self.pd.selecionados() });
                            }
                            if ui.add_enabled(n == 1, egui::Button::new("Renomear")).clicked() {
                                self.pedir_renomear();
                            }
                            if ui.button("Nova pasta").clicked() {
                                self.dialogo = Some(Dialogo::NovaPasta { pai: self.pd.pasta.clone().unwrap_or_default(), texto: String::new() });
                            }
                        }
                        Lado::Pc if self.pd.pasta.is_some() => {
                            let b = egui::Button::new(egui::RichText::new(if n > 0 { format!("COPIAR {n} PARA O PENDRIVE") } else { "COPIAR PARA O PENDRIVE".into() }).font(cond_bold(13.0)).color(K_TEXT)).fill(K_SEL);
                            if ui.add_enabled(n > 0 && !self.ocupado(), b).on_hover_text("Copia o que está selecionado para a pasta aberta no pendrive (ou arraste)").clicked() {
                                let itens = self.pc.selecionados();
                                if let Some(d) = self.pd.pasta.clone() {
                                    self.levar(itens, d, ctx);
                                }
                            }
                        }
                        _ => {}
                    }
                });
            });
        });

        // ---- lista ----
        let w = ui.available_width();
        let (head, _) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
        let x_ok = head.right() - 26.0;
        let x_tom = head.right() - 90.0;
        let x_bpm = head.right() - 104.0;
        {
            let p = ui.painter();
            p.rect_filled(head, 0.0, K_PANEL2);
            let musicas = self.painel(lado).itens.iter().filter(|i| matches!(i, Item::Musica(_))).count();
            p.text(head.left_center() + vec2(36.0, 0.0), Align2::LEFT_CENTER, format!("Música  [{musicas:04}]"), cond(13.0), K_BLUE);
            p.text(pos2(x_bpm, head.center().y), Align2::RIGHT_CENTER, "BPM", cond(13.0), K_DIM);
            p.text(pos2(x_tom, head.center().y), Align2::LEFT_CENTER, "Tom", cond(13.0), K_DIM);
            if lado == Lado::Pendrive {
                check(p, pos2(x_ok, head.center().y), 5.0, K_DIM);
            }
        }
        let lista = Rect::from_min_max(pos2(head.left(), head.bottom()), pos2(head.right(), area.bottom()));
        let arrastando = egui::DragAndDrop::has_payload_of_type::<Arrasto>(ctx);
        let ponteiro = ctx.pointer_hover_pos();
        // O lado do pendrive inteiro recebe (na pasta aberta).
        let mut alvo_lado = None;
        if lado == Lado::Pendrive {
            alvo_lado = self.pd.pasta.clone().map(|p| (lista, p));
            if arrastando && ponteiro.is_some_and(|pt| lista.contains(pt)) {
                ui.painter().rect_stroke(lista.shrink(1.0), 0.0, Stroke::new(2.0_f32, K_GREEN));
            }
        }
        let ativo = self.ativo == lado;
        let busca = self.painel(lado).busca.to_lowercase();
        let mostrar: Vec<usize> = {
            let p = self.painel(lado);
            (0..p.itens.len())
                .filter(|&i| {
                    busca.is_empty()
                        || match &p.itens[i] {
                            Item::Voltar => true,
                            Item::Pasta(c) => nome(c).to_lowercase().contains(&busca),
                            Item::Musica(m) => format!("{} {}", m.titulo, m.artista).to_lowercase().contains(&busca),
                        }
                })
                .collect()
        };
        let tocando = self.player.aberta.as_ref().map(|a| a.caminho.clone());
        let mut clique = None;
        let mut duplo = None;
        let mut alvos_pastas = Vec::new();
        egui::ScrollArea::vertical().id_salt(lado == Lado::Pc).auto_shrink(false).show_rows(ui, ROW_H, mostrar.len(), |ui, faixa| {
            let p = self.painel(lado);
            for k in faixa {
                let i = mostrar[k];
                let item = &p.itens[i];
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
                let sel = p.sel.contains(&i);
                let alvo = lado == Lado::Pendrive && matches!(item, Item::Pasta(_)) && arrastando && ponteiro.is_some_and(|pt| r.contains(pt));
                let pt = ui.painter();
                let bg = if alvo {
                    K_GREEN.gamma_multiply(0.4)
                } else if sel {
                    if ativo {
                        K_SEL
                    } else {
                        K_SEL_OFF
                    }
                } else if k % 2 == 0 {
                    K_ROW
                } else {
                    K_BG
                };
                pt.rect_filled(r, 0.0, bg);
                match item {
                    Item::Voltar => {
                        seta_esq(pt, r.left_center() + vec2(18.0, 0.0), 7.0, K_DIM);
                        pt.text(r.left_center() + vec2(36.0, 0.0), Align2::LEFT_CENTER, "Voltar", cond(16.0), K_DIM);
                    }
                    Item::Pasta(c) => {
                        pt.text(r.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, "📁", cond(14.0), K_DIM);
                        let t = caber(ui, &nome(c), &cond(16.0), r.width() - 50.0);
                        pt.text(r.left_center() + vec2(36.0, 0.0), Align2::LEFT_CENTER, t, cond(16.0), K_TEXT);
                        if lado == Lado::Pendrive {
                            alvos_pastas.push((r, c.clone()));
                        }
                    }
                    Item::Musica(m) => {
                        let no_player = tocando.as_ref() == Some(&m.caminho);
                        pt.text(r.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, if no_player { "▶" } else { "♪" }, cond(14.0), if no_player { K_GREEN } else { K_DIM });
                        let nome_m = if m.artista.is_empty() { m.titulo.clone() } else { format!("{}  ·  {}", m.titulo, m.artista) };
                        let t = caber(ui, &nome_m, &cond(16.0), x_bpm - r.left() - 90.0);
                        pt.text(r.left_center() + vec2(36.0, 0.0), Align2::LEFT_CENTER, t, cond(16.0), if no_player { K_GREEN } else { K_TEXT });
                        if let Some(b) = m.bpm {
                            pt.text(pos2(x_bpm, r.center().y), Align2::RIGHT_CENTER, format!("{b:.1}"), cond(16.0), K_TEXT);
                        }
                        if let Some(t) = m.tom {
                            pt.text(pos2(x_tom, r.center().y), Align2::LEFT_CENTER, dj_os_analise::key::label(t), cond(16.0), K_TEXT);
                        }
                        if lado == Lado::Pendrive {
                            if m.analisada {
                                check(pt, pos2(x_ok, r.center().y), 6.0, K_GREEN);
                            } else {
                                pt.circle_filled(pos2(x_ok, r.center().y), 2.0, K_DIM);
                            }
                        }
                    }
                }
                if resp.clicked() || (resp.drag_started() && !sel) {
                    clique = Some(i);
                }
                if resp.double_clicked() {
                    duplo = Some(i);
                }
                if !matches!(item, Item::Voltar) {
                    let itens = if sel { p.selecionados() } else { item.caminho().map(|c| vec![c.to_path_buf()]).unwrap_or_default() };
                    resp.dnd_set_drag_payload(Arrasto(itens));
                }
            }
        });
        // Pastas antes do lado inteiro (soltar em cima de uma pasta vai para dentro dela).
        self.alvos.extend(alvos_pastas);
        self.alvos.extend(alvo_lado);
        if mostrar.is_empty() && self.painel(lado).raiz.is_some() {
            ui.label(egui::RichText::new("Nada aqui").color(K_DIM));
        }
        // Clique: seleciona (Ctrl soma, Shift pega o intervalo).
        if let Some(i) = clique {
            self.ativo = lado;
            let m = ctx.input(|inp| inp.modifiers);
            let p = self.painel(lado);
            if m.shift {
                let (a, b) = (p.ancora.min(i), p.ancora.max(i));
                p.sel.extend(mostrar.iter().copied().filter(|&j| j >= a && j <= b));
            } else if m.command {
                if !p.sel.remove(&i) {
                    p.sel.insert(i);
                }
                p.ancora = i;
            } else {
                p.sel = BTreeSet::from([i]);
                p.ancora = i;
            }
        }
        if let Some(i) = duplo {
            self.abrir_item(lado, i, ctx);
        }
    }

    /// Dois cliques (ou Enter): voltar, entrar na pasta ou ouvir a música.
    fn abrir_item(&mut self, lado: Lado, i: usize, ctx: &egui::Context) {
        let Some(item) = self.painel(lado).itens.get(i).cloned() else { return };
        match item {
            Item::Voltar => self.painel(lado).voltar(),
            Item::Pasta(c) => self.painel(lado).abrir(c),
            Item::Musica(m) => {
                let pd = self.painel(lado).pendrive().map(Path::to_path_buf);
                self.player.carregar(m.caminho, pd, ctx);
            }
        }
        if lado == Lado::Pc {
            salvar_estado_pc(self.pc.pasta.as_deref());
        }
    }

    fn pedir_renomear(&mut self) {
        if let Some(c) = self.pd.selecionados().pop() {
            let texto = if c.is_dir() { nome(&c) } else { c.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default() };
            self.dialogo = Some(Dialogo::Renomear { texto, caminho: c });
        }
    }

    /// Soltou o que estava arrastando: em cima de uma pasta do pendrive (ou do lado dele).
    fn soltar(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.pointer.any_released()) {
            return;
        }
        let Some(a) = egui::DragAndDrop::payload::<Arrasto>(ctx) else { return };
        let Some(pt) = ctx.pointer_interact_pos() else { return };
        if let Some((_, destino)) = self.alvos.iter().find(|(r, _)| r.contains(pt)).cloned() {
            egui::DragAndDrop::clear_payload(ctx);
            self.levar(a.0.clone(), destino, ctx);
        }
    }

    fn teclado(&mut self, ctx: &egui::Context) {
        if self.dialogo.is_some() || ctx.wants_keyboard_input() {
            return;
        }
        let (espaco, voltar, enter, todos, del, f2) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::Backspace),
                i.key_pressed(egui::Key::Enter),
                i.modifiers.command && i.key_pressed(egui::Key::A),
                i.key_pressed(egui::Key::Delete),
                i.key_pressed(egui::Key::F2),
            )
        });
        if espaco {
            self.player.play_pause();
        }
        let lado = self.ativo;
        if voltar {
            self.painel(lado).voltar();
        }
        if enter && self.painel(lado).sel.len() == 1 {
            let i = *self.painel(lado).sel.iter().next().unwrap();
            self.abrir_item(lado, i, ctx);
        }
        if todos {
            let p = self.painel(lado);
            p.sel = (0..p.itens.len()).filter(|&i| !matches!(p.itens[i], Item::Voltar)).collect();
        }
        if lado == Lado::Pendrive && !self.pd.sel.is_empty() {
            if del {
                self.dialogo = Some(Dialogo::Apagar { itens: self.pd.selecionados() });
            }
            if f2 && self.pd.sel.len() == 1 {
                self.pedir_renomear();
            }
        }
    }

    /// Embaixo: a música aberta, a waveform inteira (clica para pular), play, CUE e hot cues.
    fn player_ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("player").exact_height(132.0).frame(egui::Frame::none().fill(K_PANEL).inner_margin(egui::Margin::symmetric(12.0, 8.0))).show(ctx, |ui| {
            let total = ui.available_rect_before_wrap();
            let Some(a) = &self.player.aberta else {
                let msg = match (&self.player.carregando, &self.player.erro) {
                    (Some((n, _)), _) => format!("Abrindo \"{n}\"…"),
                    (None, Some(e)) => e.clone(),
                    _ => "Dois cliques numa música (do computador ou do pendrive) para ouvir e marcar os hot cues e o CUE.".into(),
                };
                ui.painter().text(total.center(), Align2::CENTER_CENTER, msg, cond(16.0), K_DIM);
                return;
            };
            let t = a.track.clone();
            let salvo = a.salvo.clone();
            let titulo = a.titulo.clone();
            let pos = self.player.pos();
            let tocando = self.player.tocando();
            let p = ui.painter().clone();
            // ---- linha de cima: play, CUE, nome, tempo ----
            let topo = Rect::from_min_size(total.min, vec2(total.width(), 34.0));
            let play = Rect::from_min_size(topo.min, vec2(70.0, 34.0));
            let cue = Rect::from_min_size(pos2(play.right() + 6.0, topo.top()), vec2(60.0, 34.0));
            p.rect_filled(play, 3.0, if tocando { K_GREEN.gamma_multiply(0.35) } else { K_PANEL2 });
            if tocando {
                for dx in [-5.0, 5.0] {
                    p.rect_filled(Rect::from_center_size(play.center() + vec2(dx, 0.0), vec2(5.0, 16.0)), 0.0, K_GREEN);
                }
            } else {
                let c = play.center();
                p.add(egui::Shape::convex_polygon(vec![c + vec2(-6.0, -9.0), c + vec2(9.0, 0.0), c + vec2(-6.0, 9.0)], K_GREEN, Stroke::NONE));
            }
            p.rect_filled(cue, 3.0, K_PANEL2);
            p.text(cue.center(), Align2::CENTER_CENTER, "CUE", cond_bold(16.0), if salvo.cue.is_some() { K_ORANGE } else { K_TEXT });
            let dur = t.frames as f64 / t.sr;
            let tt = p.text(pos2(topo.right(), topo.center().y), Align2::RIGHT_CENTER, format!("{} / {}", tempo(pos / t.sr), tempo(dur)), cond_bold(18.0), K_TEXT);
            let nome_w = tt.left() - cue.right() - 30.0;
            p.text(pos2(cue.right() + 14.0, topo.center().y), Align2::LEFT_CENTER, caber(ui, &titulo, &cond_bold(17.0), nome_w), cond_bold(17.0), K_TEXT);
            let r_play = ui.interact(play, ui.id().with("play"), Sense::click()).on_hover_text("Tocar / pausar (espaço)");
            let r_cue = ui.interact(cue, ui.id().with("cue"), Sense::click()).on_hover_text("Parado: marca o CUE aqui. Tocando: volta para o CUE.");
            // ---- waveform inteira ----
            let onda = Rect::from_min_max(pos2(total.left(), topo.bottom() + 8.0), pos2(total.right() - 330.0, total.bottom()));
            p.rect_filled(onda, 0.0, K_BG);
            let n = salvo.low.len().min(salvo.mid.len()).min(salvo.high.len());
            let w = onda.width().max(1.0) as usize;
            let meio = onda.center().y;
            let hh = onda.height() / 2.0 - 2.0;
            let tocado = if t.frames > 0 { (pos / t.frames as f64) as f32 * onda.width() } else { 0.0 };
            if n > 0 {
                let mut mesh = egui::Mesh::default();
                for x in 0..w {
                    let (b0, b1) = (x * n / w, ((x + 1) * n / w).max(x * n / w + 1).min(n));
                    let (mut l, mut m, mut h) = (0u8, 0u8, 0u8);
                    for i in b0..b1 {
                        l = l.max(salvo.low[i]);
                        m = m.max(salvo.mid[i]);
                        h = h.max(salvo.high[i]);
                    }
                    let dim = if (x as f32) < tocado { 0.45 } else { 1.0 };
                    let fx = onda.left() + x as f32;
                    for (v, cor) in camadas(l as f32 / 255.0, m as f32 / 255.0, h as f32 / 255.0).into_iter().zip(BANDAS) {
                        let a = v * hh;
                        if a > 0.5 {
                            mesh.add_colored_rect(Rect::from_min_max(pos2(fx, meio - a), pos2(fx + 1.0, meio + a)), cor.gamma_multiply(dim));
                        }
                    }
                }
                p.add(egui::Shape::mesh(mesh));
            }
            let x_de = |f: f64| onda.left() + (f / t.frames.max(1) as f64) as f32 * onda.width();
            if let Some(c) = salvo.cue {
                let x = x_de(c);
                p.add(egui::Shape::convex_polygon(vec![pos2(x - 6.0, onda.bottom()), pos2(x + 6.0, onda.bottom()), pos2(x, onda.bottom() - 9.0)], K_ORANGE, Stroke::NONE));
            }
            for (i, h) in salvo.hotcues.iter().enumerate() {
                if let Some(h) = h {
                    let x = x_de(*h);
                    p.line_segment([pos2(x, onda.top()), pos2(x, onda.bottom())], Stroke::new(1.0_f32, CUE_CORES[i]));
                    let lab = Rect::from_min_size(pos2(x, onda.top()), vec2(13.0, 13.0));
                    p.rect_filled(lab, 1.0, CUE_CORES[i]);
                    p.text(lab.center(), Align2::CENTER_CENTER, CUE_LETRAS[i], cond_bold(10.0), K_BG);
                }
            }
            let xp = x_de(pos);
            p.line_segment([pos2(xp, onda.top()), pos2(xp, onda.bottom())], Stroke::new(2.0_f32, K_TEXT));
            let r_onda = ui.interact(onda, ui.id().with("onda"), Sense::click_and_drag()).on_hover_text("Clique para pular para esse ponto");
            // ---- hot cues ----
            let grade = Rect::from_min_max(pos2(onda.right() + 12.0, onda.top()), pos2(total.right(), total.bottom()));
            let (bw, bh) = ((grade.width() - 3.0 * 6.0) / 4.0, (grade.height() - 6.0) / 2.0);
            let mut cliques_hc = Vec::new();
            for i in 0..8 {
                let r = Rect::from_min_size(pos2(grade.left() + (i % 4) as f32 * (bw + 6.0), grade.top() + (i / 4) as f32 * (bh + 6.0)), vec2(bw, bh));
                let marcado = salvo.hotcues[i];
                p.rect_filled(r, 3.0, if marcado.is_some() { CUE_CORES[i].gamma_multiply(0.85) } else { K_PANEL2 });
                p.rect_stroke(r, 3.0, Stroke::new(1.0_f32, CUE_CORES[i]));
                let txt = match marcado {
                    Some(f) => format!("{}  {}", CUE_LETRAS[i], tempo(f / t.sr)),
                    None => CUE_LETRAS[i].to_string(),
                };
                p.text(r.center(), Align2::CENTER_CENTER, txt, cond_bold(15.0), if marcado.is_some() { K_BG } else { CUE_CORES[i] });
                let resp = ui.interact(r, ui.id().with(("hc", i)), Sense::click()).on_hover_text("Vazio: marca aqui. Marcado: vai para ele. Botão direito: apaga.");
                if resp.clicked() {
                    cliques_hc.push((i, false));
                }
                if resp.secondary_clicked() {
                    cliques_hc.push((i, true));
                }
            }
            // ---- o que foi clicado ----
            if r_play.clicked() {
                self.player.play_pause();
            }
            if r_cue.clicked() {
                if let Some(m) = self.player.cue() {
                    self.avisar(m);
                }
            }
            if r_onda.clicked() || r_onda.dragged() {
                if let Some(pt) = r_onda.interact_pointer_pos() {
                    self.player.pular(((pt.x - onda.left()) / onda.width()) as f64);
                }
            }
            for (i, apagar) in cliques_hc {
                let msg = if apagar { self.player.apagar_hotcue(i) } else { self.player.hotcue(i) };
                if let Some(m) = msg {
                    self.avisar(m);
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
            self.pd.sel.clear();
            fechar = true;
            self.recarregar();
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

    /// Para o README e testes: DJOS_PREPARAR_PASTA=<pasta do pendrive> abre essa pasta no lado do
    /// pendrive; DJOS_PREPARAR_COPIAR=1 copia duas músicas do computador para lá;
    /// DJOS_PREPARAR_TOCAR=<música> abre no player (com o hot cue A marcado);
    /// DJOS_SCREENSHOT=<arquivo> salva a tela (RGBA cru com cabeçalho "largura altura\n") depois que
    /// a cópia termina.
    fn diagnostics(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        if self.frame == 2 {
            if let Some(p) = std::env::var_os("DJOS_PREPARAR_PASTA").map(PathBuf::from) {
                self.pd.abrir(p);
            }
            let musicas: Vec<usize> = (0..self.pc.itens.len()).filter(|&i| matches!(self.pc.itens[i], Item::Musica(_))).collect();
            self.pc.sel = musicas.iter().skip(1).take(2).copied().collect();
            if let Some(m) = std::env::var_os("DJOS_PREPARAR_TOCAR").map(PathBuf::from) {
                let pd = self.pd.raiz.clone().filter(|r| m.starts_with(r));
                self.player.carregar(m, pd, ctx);
            }
        }
        if self.frame == 5 && std::env::var_os("DJOS_PREPARAR_COPIAR").is_some() {
            let itens = self.pc.selecionados();
            if let Some(d) = self.pd.pasta.clone() {
                self.levar(itens, d, ctx);
            }
        }
        let Some(path) = std::env::var_os("DJOS_SCREENSHOT") else { return };
        ctx.request_repaint();
        if self.frame == 60 && std::env::var_os("DJOS_PREPARAR_TOCAR").is_some() && self.player.aberta.is_some() {
            self.player.pular(0.4);
            self.player.hotcue(0);
            self.player.pular(0.55);
        }
        let pronto = self.player.carregando.is_none() && !self.ocupado();
        if self.frame >= 90 && pronto && !self.foto_pedida {
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

// ------------------------------------------------------------------ onde o computador parou

fn arquivo_estado() -> Option<PathBuf> {
    let base = if cfg!(windows) { std::env::var_os("APPDATA").map(PathBuf::from) } else { std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))) };
    base.map(|b| b.join("dj-os-preparar").join("ultima-pasta.txt"))
}

/// Última pasta aberta no lado do computador.
fn estado_pc() -> Option<PathBuf> {
    if std::env::var_os("DJOS_PREPARAR_PC").is_some() {
        return None;
    }
    std::fs::read_to_string(arquivo_estado()?).ok().map(|t| PathBuf::from(t.trim())).filter(|p| !p.as_os_str().is_empty())
}

fn salvar_estado_pc(p: Option<&Path>) {
    if std::env::var_os("DJOS_PREPARAR_PC").is_some() {
        return;
    }
    if let (Some(f), Some(p)) = (arquivo_estado(), p) {
        let _ = std::fs::create_dir_all(f.parent().unwrap_or(Path::new(".")));
        let _ = std::fs::write(f, p.display().to_string());
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
