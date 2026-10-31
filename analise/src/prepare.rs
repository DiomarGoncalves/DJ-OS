//! Prepara um pendrive no PC, como o rekordbox: analisa cada música (BPM, grade, tom, volume e
//! waveform) e grava em `DJOS/analise/` dentro do próprio pendrive. No aparelho a música já entra
//! analisada, sem gastar processador. Músicas já preparadas são puladas; se o PC já tinha a análise
//! (com cues marcados no PC), ela vai junto.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

/// Extensões de áudio que o DJ OS toca.
pub const AUDIO_EXT: [&str; 9] = ["mp3", "flac", "wav", "m4a", "aac", "ogg", "aiff", "aif", "mp4"];

pub fn is_audio(p: &Path) -> bool {
    p.is_file() && p.extension().is_some_and(|e| AUDIO_EXT.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

/// Todas as músicas dentro de `root` (subpastas incluídas; pastas ocultas e a do DJ OS ficam de
/// fora), em ordem.
pub fn list_tracks(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(d) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name.starts_with('$') || name == "System Volume Information" || p == root.join("DJOS") {
                continue;
            }
            if p.is_dir() {
                dirs.push(p);
            } else if is_audio(&p) {
                files.push(p);
            }
        }
    }
    files.sort();
    files
}

/// Resultado de uma música.
pub enum Outcome {
    /// Analisada agora (BPM).
    Done(f64),
    /// Já estava preparada.
    Had,
    Failed(String),
}

/// Contagem final: (analisadas agora, já estavam, com erro).
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Totals {
    pub done: usize,
    pub had: usize,
    pub failed: usize,
}

/// Prepara as `files` do pendrive em `root` usando `threads` análises ao mesmo tempo. `progress`
/// recebe cada música terminada (uma por vez). `stop` interrompe entre uma música e outra.
pub fn prepare_files(root: &Path, files: &[PathBuf], threads: usize, stop: &AtomicBool, progress: impl Fn(&Path, &Outcome) + Sync) -> Totals {
    let next = AtomicUsize::new(0);
    let totals = Mutex::new(Totals::default());
    let report = Mutex::new(());
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(f) = files.get(i) else { break };
                let out = prepare_one(root, f);
                {
                    let mut t = totals.lock().unwrap();
                    match out {
                        Outcome::Done(_) => t.done += 1,
                        Outcome::Had => t.had += 1,
                        Outcome::Failed(_) => t.failed += 1,
                    }
                }
                let _g = report.lock().unwrap();
                progress(f, &out);
            });
        }
    });
    totals.into_inner().unwrap()
}

/// Atalho: prepara o pendrive inteiro.
pub fn prepare_drive(root: &Path, threads: usize, progress: impl Fn(&Path, &Outcome) + Sync) -> Totals {
    prepare_files(root, &list_tracks(root), threads, &AtomicBool::new(false), progress)
}

fn prepare_one(root: &Path, f: &Path) -> Outcome {
    if crate::store::has_portable(root, f) {
        return Outcome::Had;
    }
    let saved = match crate::store::load(f) {
        Some(s) if s.key != crate::key::RAW_UNKNOWN && !s.loudness.is_nan() => Ok(s),
        _ => crate::rekordbox::first_analysis(f).map(|(_, s)| s),
    };
    match saved.map_err(|e| e.to_string()).and_then(|s| crate::store::save_portable(root, f, &s).map(|_| s.bpm).map_err(|e| e.to_string())) {
        Ok(bpm) => Outcome::Done(bpm),
        Err(e) => Outcome::Failed(e),
    }
}

/// Quantas análises ao mesmo tempo: metade dos núcleos (cada uma decodifica a música inteira na
/// memória, ~100 MB por música de 5 min), entre 1 e 4.
pub fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get() / 2).clamp(1, 4)
}
