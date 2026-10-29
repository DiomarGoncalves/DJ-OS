//! Músicas internas: copiar pastas do pendrive/SSD para o disco do sistema, apagar o que está lá
//! dentro, e a análise automática em segundo plano (BPM, grade e waveform ficam salvos).

use crate::app::{data_dir, SharedApp};
use crate::library::Index;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Pasta das músicas internas.
pub fn internal_dir() -> PathBuf {
    data_dir().join("musicas")
}

/// Andamento da importação, mostrado na tela.
#[derive(Clone, Debug, Default)]
pub struct ImportProgress {
    pub name: String,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub finished: bool,
}

/// Espaço livre (em bytes) no disco onde está `path`.
pub fn free_space(path: &Path) -> Option<u64> {
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some(st.f_bavail as u64 * st.f_frsize as u64)
}

/// Baixa a prioridade da thread atual (para cópia e análise não atrapalharem o áudio).
fn low_priority() {
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, libc::gettid() as libc::id_t, 19);
    }
}

fn audio_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if crate::library::is_audio(&p) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Plano da cópia: (origem, destino) das músicas que ainda não existem lá, e quantas já existiam.
fn plan(src: &Path, dest_root: &Path) -> (Vec<(PathBuf, PathBuf, u64)>, usize) {
    let name = src.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    let mut todo = Vec::new();
    let mut skipped = 0;
    for f in audio_files(src) {
        let rel = f.strip_prefix(src).unwrap_or(&f);
        let dst = dest_root.join(&name).join(rel);
        let size = std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0);
        if std::fs::metadata(&dst).is_ok_and(|m| m.len() == size) {
            skipped += 1;
        } else {
            todo.push((f, dst, size));
        }
    }
    (todo, skipped)
}

/// Copia a pasta `src` para as músicas internas, em segundo plano. Recusa se faltar espaço
/// ou se outra cópia estiver em andamento. Retorna a mensagem para mostrar na tela.
pub fn start(app: &SharedApp, src: PathBuf) -> String {
    {
        let a = app.lock();
        if a.import.as_ref().is_some_and(|p| !p.finished) {
            return "Já tem uma cópia em andamento".into();
        }
    }
    let dest_root = internal_dir();
    if src.starts_with(&dest_root) {
        return "Essa pasta já está nas músicas internas".into();
    }
    let _ = std::fs::create_dir_all(&dest_root);
    let (todo, skipped) = plan(&src, &dest_root);
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if todo.is_empty() {
        return if skipped > 0 { format!("\"{name}\" já está no sistema ({skipped} músicas)") } else { format!("\"{name}\" não tem músicas") };
    }
    let total: u64 = todo.iter().map(|t| t.2).sum();
    let free = free_space(&dest_root).unwrap_or(u64::MAX);
    // Deixa 1 GB livre de folga para o sistema.
    if total + (1 << 30) > free {
        return format!("Sem espaço: precisa de {} MB, tem {} MB livres", total >> 20, free >> 20);
    }
    let files_total = todo.len();
    app.lock().import = Some(ImportProgress {
        name: name.clone(),
        files_total,
        bytes_total: total,
        ..Default::default()
    });
    let app2 = app.clone();
    let msg = format!("Copiando \"{name}\" para o sistema ({files_total} músicas)…");
    std::thread::Builder::new()
        .name("importar".into())
        .spawn(move || {
            low_priority();
            let mut failed = 0;
            for (from, to, size) in todo {
                let ok = to.parent().is_some_and(|p| std::fs::create_dir_all(p).is_ok()) && copy_file(&from, &to).is_ok();
                if !ok {
                    failed += 1;
                    let _ = std::fs::remove_file(&to);
                }
                let mut a = app2.lock();
                if let Some(p) = a.import.as_mut() {
                    p.files_done += 1;
                    p.bytes_done += size;
                }
            }
            let mut a = app2.lock();
            if let Some(p) = a.import.as_mut() {
                p.finished = true;
            }
            a.lib.force_reload();
            // Mostra a pasta copiada: abre "Músicas internas" e seleciona a pasta nova.
            a.lib.reveal(&dest_root.join(&name));
            let ja = if skipped > 0 { format!("; {skipped} já estavam") } else { String::new() };
            let msg = if failed == 0 {
                format!("\"{name}\" copiada para o sistema ({files_total} músicas{ja})")
            } else {
                format!("\"{name}\": {failed} música(s) não copiaram")
            };
            a.toast(msg);
        })
        .expect("thread de importação");
    msg
}

/// Copia para um temporário e renomeia no fim: uma cópia interrompida não deixa arquivo pela metade.
fn copy_file(from: &Path, to: &Path) -> std::io::Result<()> {
    let tmp = to.with_extension("parcial");
    std::fs::copy(from, &tmp)?;
    std::fs::rename(&tmp, to)
}

/// Apaga uma música ou pasta, só se estiver dentro das músicas internas.
pub fn delete_internal(path: &Path) -> Result<(), String> {
    let root = internal_dir().canonicalize().map_err(|e| e.to_string())?;
    let target = path.canonicalize().map_err(|e| e.to_string())?;
    if !target.starts_with(&root) || target == root {
        return Err("só dá para apagar o que está nas músicas internas".into());
    }
    if target.is_dir() {
        std::fs::remove_dir_all(&target).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(&target).map_err(|e| e.to_string())
    }
}

/// Analisa sozinho, com prioridade baixa, toda música do índice que ainda não tem análise salva.
/// `counter` sobe a cada música analisada (a lista usa para mostrar o BPM novo).
pub fn start_analyzer(index: Arc<Mutex<Index>>, counter: Arc<AtomicU64>) {
    std::thread::Builder::new()
        .name("analise".into())
        .spawn(move || {
            low_priority();
            loop {
                let paths: Vec<PathBuf> = index.lock().entries.iter().map(|(_, t)| t.path.clone()).collect();
                let mut did = 0;
                for p in paths {
                    if crate::store::peek_bpm(&p).is_some() {
                        continue;
                    }
                    if let Ok(t) = crate::track::load(&p, true) {
                        crate::store::save(&p, &crate::store::Saved::from_track(&t));
                        counter.fetch_add(1, Ordering::Relaxed);
                        did += 1;
                    }
                    // Folga entre músicas para nunca ocupar o processador de vez.
                    std::thread::sleep(Duration::from_millis(300));
                }
                if did > 0 {
                    eprintln!("[djos] análise automática: {did} música(s) analisadas");
                }
                std::thread::sleep(Duration::from_secs(10));
            }
        })
        .expect("thread de análise");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plano_copia_so_o_que_falta_e_mantem_subpastas() {
        let base = std::env::temp_dir().join(format!("dj-os-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("pendrive").join("Funk");
        std::fs::create_dir_all(src.join("Antigas")).unwrap();
        std::fs::write(src.join("a.mp3"), b"123").unwrap();
        std::fs::write(src.join("Antigas").join("b.wav"), b"45").unwrap();
        std::fs::write(src.join("capa.jpg"), b"x").unwrap();
        let dest = base.join("internas");
        let (todo, skipped) = plan(&src, &dest);
        assert_eq!((todo.len(), skipped), (2, 0), "só as músicas, sem a capa");
        assert!(todo.iter().any(|(_, d, _)| d.ends_with("Funk/Antigas/b.wav")));
        for (f, t, _) in &todo {
            std::fs::create_dir_all(t.parent().unwrap()).unwrap();
            copy_file(f, t).unwrap();
        }
        let (todo, skipped) = plan(&src, &dest);
        assert_eq!((todo.len(), skipped), (0, 2), "não duplica");
        assert!(free_space(&base).unwrap() > 0);
        let _ = std::fs::remove_dir_all(base);
    }
}
