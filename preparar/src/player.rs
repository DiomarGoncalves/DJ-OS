//! Player do Preparar: ouve a música, pula para qualquer ponto e marca os hot cues A–H e o CUE.
//! Os cues vão para a análise da música: a do pendrive (o aparelho lê dela) ou, para música do
//! computador, a do computador (vai junto quando a música for copiada para o pendrive).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use dj_os_analise::store::{self, Saved};
use dj_os_analise::track::Track;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use eframe::egui;

/// Música aberta no player.
pub struct Aberta {
    pub caminho: PathBuf,
    pub titulo: String,
    pub track: Arc<Track>,
    /// Análise com os cues (o que é gravado).
    pub salvo: Saved,
    /// Pendrive onde ela está (grava a análise lá) ou `None` (computador).
    pub pendrive: Option<PathBuf>,
}

/// O que o áudio lê e escreve (posição em frames da música, tocando ou não).
#[derive(Default)]
struct Compartilhado {
    pos: AtomicU64,
    tocando: AtomicBool,
    track: Mutex<Option<Arc<Track>>>,
}

impl Compartilhado {
    fn pos(&self) -> f64 {
        f64::from_bits(self.pos.load(Ordering::Relaxed))
    }

    fn set_pos(&self, p: f64) {
        self.pos.store(p.to_bits(), Ordering::Relaxed);
    }
}

pub struct Player {
    pub aberta: Option<Aberta>,
    /// Carregando: nome e de onde vem o resultado.
    pub carregando: Option<(String, Receiver<Result<Aberta, String>>)>,
    pub erro: Option<String>,
    c: Arc<Compartilhado>,
    saida: Option<cpal::Stream>,
}

impl Default for Player {
    fn default() -> Self {
        Self { aberta: None, carregando: None, erro: None, c: Arc::default(), saida: None }
    }
}

/// Abre a música (decodifica inteira) com a análise dela; sem análise, analisa agora (e, no
/// pendrive, já grava).
fn abrir(caminho: &Path, pendrive: Option<&Path>) -> Result<Aberta, String> {
    let guardado = match pendrive {
        Some(r) => store::load_portable(r, caminho),
        None => store::load(caminho),
    };
    let (track, salvo) = match guardado {
        Some(s) => (dj_os_analise::track::load(caminho, false)?, s),
        None => {
            let (t, s) = dj_os_analise::rekordbox::first_analysis(caminho)?;
            if let Some(r) = pendrive {
                let _ = store::save_portable(r, caminho, &s);
            }
            (t, s)
        }
    };
    let titulo = if track.artist.is_empty() { track.title.clone() } else { format!("{}  ·  {}", track.title, track.artist) };
    Ok(Aberta { caminho: caminho.to_path_buf(), titulo, track: Arc::new(track), salvo, pendrive: pendrive.map(Path::to_path_buf) })
}

impl Player {
    /// Começa a abrir uma música (em segundo plano: decodificar leva um tempinho).
    pub fn carregar(&mut self, caminho: PathBuf, pendrive: Option<PathBuf>, ctx: &egui::Context) {
        self.pausar();
        let (tx, rx) = channel();
        let nome = caminho.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(abrir(&caminho, pendrive.as_deref()));
            ctx.request_repaint();
        });
        self.carregando = Some((nome, rx));
        self.erro = None;
    }

    /// Chamado a cada quadro: pega a música que terminou de abrir.
    pub fn tick(&mut self) {
        let Some((_, rx)) = &self.carregando else { return };
        let Ok(r) = rx.try_recv() else { return };
        self.carregando = None;
        match r {
            Ok(a) => {
                *self.c.track.lock().unwrap() = Some(a.track.clone());
                // Começa no CUE, se tiver.
                self.c.set_pos(a.salvo.cue.unwrap_or(0.0));
                self.aberta = Some(a);
            }
            Err(e) => self.erro = Some(format!("Não deu para abrir: {e}")),
        }
    }

    pub fn tocando(&self) -> bool {
        self.c.tocando.load(Ordering::Relaxed)
    }

    pub fn pos(&self) -> f64 {
        self.c.pos()
    }

    pub fn pausar(&mut self) {
        self.c.tocando.store(false, Ordering::Relaxed);
    }

    pub fn play_pause(&mut self) {
        if self.aberta.is_none() {
            return;
        }
        if self.saida.is_none() {
            match abrir_saida(self.c.clone()) {
                Ok(s) => self.saida = Some(s),
                Err(e) => {
                    self.erro = Some(format!("Sem saída de som: {e}"));
                    return;
                }
            }
        }
        let on = !self.tocando();
        self.c.tocando.store(on, Ordering::Relaxed);
    }

    /// Pula para uma fração da música (0 a 1).
    pub fn pular(&mut self, frac: f64) {
        if let Some(a) = &self.aberta {
            self.c.set_pos(frac.clamp(0.0, 1.0) * a.track.frames as f64);
        }
    }

    /// Hot cue: vazio, marca onde está; marcado, vai para ele.
    pub fn hotcue(&mut self, i: usize) -> Option<String> {
        let pos = self.pos();
        let a = self.aberta.as_mut()?;
        match a.salvo.hotcues[i] {
            Some(p) => {
                self.c.set_pos(p);
                None
            }
            None => {
                a.salvo.hotcues[i] = Some(pos);
                Some(self.gravar())
            }
        }
    }

    pub fn apagar_hotcue(&mut self, i: usize) -> Option<String> {
        self.aberta.as_mut()?.salvo.hotcues[i] = None;
        Some(self.gravar())
    }

    /// CUE: marca onde está (parado) ou volta para ele (tocando).
    pub fn cue(&mut self) -> Option<String> {
        let pos = self.pos();
        let tocando = self.tocando();
        let a = self.aberta.as_mut()?;
        match (tocando, a.salvo.cue) {
            (true, Some(c)) => {
                self.c.set_pos(c);
                self.pausar();
                None
            }
            _ => {
                a.salvo.cue = Some(pos);
                Some(self.gravar())
            }
        }
    }

    /// Grava os cues na análise da música; devolve a mensagem para mostrar.
    fn gravar(&self) -> String {
        let Some(a) = &self.aberta else { return String::new() };
        match &a.pendrive {
            Some(r) => match store::save_portable(r, &a.caminho, &a.salvo) {
                Ok(()) => "Cues gravados no pendrive".into(),
                Err(e) => format!("Não gravou no pendrive: {e}"),
            },
            None => {
                store::save(&a.caminho, &a.salvo);
                "Cues guardados (vão junto quando copiar para o pendrive)".into()
            }
        }
    }
}

/// Abre a saída de som padrão do computador e toca a música da posição atual, ajustando a taxa
/// de amostragem (a música e a placa podem ser diferentes).
fn abrir_saida(c: Arc<Compartilhado>) -> Result<cpal::Stream, String> {
    let dev = cpal::default_host().default_output_device().ok_or("nenhuma saída de som")?;
    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
    let canais = cfg.channels() as usize;
    let taxa = cfg.sample_rate().0 as f64;
    let erro = |e| eprintln!("[preparar] som: {e}");
    let preencher = move |out: &mut [f32]| {
        // Nunca espera trava no áudio: se a tela está trocando a música, toca silêncio.
        let t = c.track.try_lock().ok().and_then(|g| g.clone());
        let tocando = c.tocando.load(Ordering::Relaxed);
        let (Some(t), true) = (t, tocando) else {
            out.fill(0.0);
            return;
        };
        let passo = t.sr / taxa;
        let mut pos = c.pos();
        for quadro in out.chunks_mut(canais) {
            let i = pos as usize;
            let (l, r) = if i + 1 < t.frames {
                let f = (pos - i as f64) as f32;
                let at = |k: usize, ch: usize| t.data[k * 2 + ch];
                (at(i, 0) * (1.0 - f) + at(i + 1, 0) * f, at(i, 1) * (1.0 - f) + at(i + 1, 1) * f)
            } else {
                (0.0, 0.0)
            };
            for (k, s) in quadro.iter_mut().enumerate() {
                *s = 0.8 * if k % 2 == 0 { l } else { r };
            }
            pos += passo;
        }
        if pos >= t.frames as f64 {
            pos = t.frames as f64;
            c.tocando.store(false, Ordering::Relaxed);
        }
        c.set_pos(pos);
    };
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => dev.build_output_stream(&cfg.into(), move |out: &mut [f32], _| preencher(out), erro, None),
        cpal::SampleFormat::I16 => {
            let mut buf = Vec::new();
            dev.build_output_stream(
                &cfg.into(),
                move |out: &mut [i16], _| {
                    buf.resize(out.len(), 0.0);
                    preencher(&mut buf);
                    for (o, s) in out.iter_mut().zip(&buf) {
                        *o = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                    }
                },
                erro,
                None,
            )
        }
        f => return Err(format!("formato de som {f:?} não suportado")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}
