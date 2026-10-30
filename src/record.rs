//! Gravação do set: o master (depois do limitador) vira um WAV na pasta `records`, no aparelho
//! (`~/DJOS/records`) ou direto no pendrive (`<pendrive>/records`), conforme CONFIG → ÁUDIO.
//!
//! O cabeçalho do WAV é atualizado a cada 2 s: se faltar luz, o que foi gravado até ali abre
//! normalmente. A gravação para sozinha se o disco estiver acabando ou o pendrive for retirado.
//! No aparelho os pendrives ficam só leitura: a ferramenta `dj-os-pendrive` libera a escrita só
//! enquanto grava (ou copia) e trava de novo no fim.

use crate::app::{data_dir, SharedApp};
use crate::shared::{Cmd, CmdTx};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Para de gravar com menos que isto livre no disco.
const MIN_FREE: u64 = 300 << 20;

#[derive(Default)]
pub struct RecState {
    /// Gravando agora: arquivo, quando começou e o sinal para parar.
    pub active: Option<(PathBuf, Instant, Arc<AtomicBool>)>,
}

/// Onde gravar (CONFIG → ÁUDIO).
pub const TARGETS: [&str; 3] = ["AUTOMÁTICO (pendrive, se houver)", "APARELHO", "PENDRIVE"];

/// Pasta `records` do aparelho.
pub fn dir() -> PathBuf {
    data_dir().join("records")
}

/// Libera (`true`) ou trava a escrita no pendrive. No PC de desenvolvimento o pendrive já aceita
/// escrita e o sudo recusa: tudo bem, quem decide é se a pasta dá para criar.
fn pendrive_write(drive: &Path, on: bool) -> bool {
    std::process::Command::new("sudo")
        .args(["-n", "/usr/local/sbin/dj-os-pendrive", if on { "liberar" } else { "travar" }])
        .arg(drive)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Pasta de destino da gravação; o pendrive (se for nele) para travar de novo no fim.
fn destination(target: usize) -> Result<(PathBuf, Option<PathBuf>), String> {
    let drive = first_pendrive();
    match (target, drive) {
        (2, None) => Err("Coloque um pendrive para gravar nele (ou escolha APARELHO em CONFIG → ÁUDIO)".into()),
        (1, _) | (_, None) => Ok((dir(), None)),
        (_, Some(d)) => {
            let rec = d.join("records");
            if std::fs::create_dir_all(&rec).is_ok() {
                return Ok((rec, None));
            }
            if pendrive_write(&d, true) && std::fs::create_dir_all(&rec).is_ok() {
                return Ok((rec, Some(d)));
            }
            Err("Este pendrive não aceita gravação (ex.: o do Ventoy)".into())
        }
    }
}

/// Cabeçalho WAV PCM 16 bits estéreo com `data_bytes` de áudio.
pub fn wav_header(rate: u32, data_bytes: u32) -> [u8; 44] {
    let mut h = [0u8; 44];
    let mut put = |at: usize, b: &[u8]| h[at..at + b.len()].copy_from_slice(b);
    put(0, b"RIFF");
    put(4, &(36 + data_bytes).to_le_bytes());
    put(8, b"WAVEfmt ");
    put(16, &16u32.to_le_bytes());
    put(20, &1u16.to_le_bytes()); // PCM
    put(22, &2u16.to_le_bytes()); // estéreo
    put(24, &rate.to_le_bytes());
    put(28, &(rate * 4).to_le_bytes());
    put(32, &4u16.to_le_bytes());
    put(34, &16u16.to_le_bytes());
    put(36, b"data");
    put(40, &data_bytes.to_le_bytes());
    h
}

/// Liga ou desliga a gravação (botão REC).
pub fn toggle(app: &SharedApp, cmd: &CmdTx, rate: f64) {
    let active = app.lock().rec.active.is_some();
    if active {
        stop(app, cmd);
    } else {
        start(app, cmd, rate);
    }
}

pub fn stop(app: &SharedApp, cmd: &CmdTx) {
    if let Some((_, _, stop)) = app.lock().rec.active.take() {
        cmd.send(Cmd::RecDetach);
        stop.store(true, Ordering::Relaxed);
    }
}

pub fn start(app: &SharedApp, cmd: &CmdTx, rate: f64) {
    let rate = if rate > 0.0 { rate as u32 } else { 48000 };
    let target = app.lock().settings.rec_target;
    let (dir, relock) = match destination(target) {
        Ok(v) => v,
        Err(e) => {
            app.lock().toast(e);
            return;
        }
    };
    let _ = std::fs::create_dir_all(&dir);
    let unlock_back = |relock: &Option<PathBuf>| {
        if let Some(d) = relock {
            pendrive_write(d, false);
        }
    };
    if crate::import::free_space(&dir).is_some_and(|f| f < MIN_FREE * 4) {
        unlock_back(&relock);
        app.lock().toast("Sem espaço para gravar (precisa de 1,2 GB livres)".to_string());
        return;
    }
    let path = dir.join(format!("set-{}.wav", chrono::Local::now().format("%Y-%m-%d-%H%M%S")));
    let mut file = match std::fs::File::create(&path) {
        Ok(f) => f,
        Err(e) => {
            unlock_back(&relock);
            app.lock().toast(format!("Não deu para gravar: {e}"));
            return;
        }
    };
    let _ = file.write_all(&wav_header(rate, 0));
    eprintln!("[djos] gravando o set em {}", path.display());
    // 5 s de folga na fila: o disco pode demorar um pouco sem perder som.
    let (tx, mut rx) = rtrb::RingBuffer::<[f32; 2]>::new(rate as usize * 5);
    let stop = Arc::new(AtomicBool::new(false));
    app.lock().rec.active = Some((path.clone(), Instant::now(), stop.clone()));
    cmd.send(Cmd::RecAttach(tx));
    let (app, cmd2) = (app.clone(), cmd.clone());
    std::thread::Builder::new()
        .name("gravacao".into())
        .spawn(move || {
            let mut out = std::io::BufWriter::with_capacity(1 << 20, file);
            let mut bytes: u64 = 0;
            let mut last_header = Instant::now();
            let mut last_space = Instant::now();
            let mut noise = 0x1234_5678u32;
            let mut write_failed = false;
            let mut dither = || {
                // Ruído triangular de 1 bit (dither) ao converter para 16 bits.
                noise ^= noise << 13;
                noise ^= noise >> 17;
                noise ^= noise << 5;
                let a = (noise & 0xffff) as f32 / 65535.0;
                let b = (noise >> 16) as f32 / 65535.0;
                a - b
            };
            loop {
                let stopping = stop.load(Ordering::Relaxed);
                while let Ok(f) = rx.pop() {
                    for x in f {
                        let v = (x * 32767.0 + dither()).round().clamp(-32768.0, 32767.0) as i16;
                        write_failed |= out.write_all(&v.to_le_bytes()).is_err();
                    }
                    bytes += 4;
                }
                // O WAV guarda o tamanho em 32 bits (até ~6 h a 48 kHz): perto disso, para.
                let full = bytes > u32::MAX as u64 - (1 << 20);
                if stopping || full || last_header.elapsed() > Duration::from_secs(2) {
                    let _ = out.flush();
                    let f = out.get_mut();
                    let _ = f.seek(SeekFrom::Start(0)).and_then(|_| f.write_all(&wav_header(rate, bytes as u32)));
                    let _ = f.seek(SeekFrom::End(0));
                    let _ = f.sync_data();
                    last_header = Instant::now();
                }
                if stopping {
                    break;
                }
                let low_space = last_space.elapsed() > Duration::from_secs(30) && crate::import::free_space(&dir).is_some_and(|f| f < MIN_FREE);
                if full || low_space || write_failed {
                    let why = if write_failed {
                        "não deu para escrever (pendrive retirado?)"
                    } else if full {
                        "gravação chegou ao limite de 6 h"
                    } else {
                        "disco quase cheio"
                    };
                    app.lock().toast(format!("Gravação parada: {why}"));
                    stop_from_writer(&app, &cmd2);
                }
                if last_space.elapsed() > Duration::from_secs(30) {
                    last_space = Instant::now();
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            drop(out);
            unlock_back(&relock);
            let mins = bytes as f64 / 4.0 / rate as f64 / 60.0;
            eprintln!("[djos] gravação parada: {:.1} s", bytes as f64 / 4.0 / rate as f64);
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let mut a = app.lock();
            let onde = if dir.starts_with(data_dir()) { "no aparelho (records)" } else { "no pendrive (records)" };
            a.toast(format!("Gravação salva {onde}: {name} ({mins:.0} min)"));
            a.lib.force_reload();
        })
        .expect("thread da gravação");
}

fn stop_from_writer(app: &SharedApp, cmd: &CmdTx) {
    stop(app, cmd);
}

/// Pendrive para onde copiar (o primeiro montado).
fn first_pendrive() -> Option<PathBuf> {
    crate::library::sources().into_iter().map(|(_, p)| p).find(|p| p.starts_with("/media") || p.starts_with("/run/media"))
}

/// Copia uma gravação do aparelho para o pendrive, na pasta "records".
pub fn export(app: &SharedApp, file: PathBuf) {
    let Some(drive) = first_pendrive() else {
        app.lock().toast("Coloque um pendrive para copiar a gravação".to_string());
        return;
    };
    let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    app.lock().toast(format!("Copiando {name} para o pendrive…"));
    let app = app.clone();
    std::thread::spawn(move || {
        crate::import::low_priority();
        let msg = match copy_direct(&file, &drive).or_else(|_| copy_with_tool(&file, &drive)) {
            Ok(()) => format!("{name} copiada para o pendrive (pasta \"records\")"),
            Err(e) => format!("Não copiou para o pendrive: {e}"),
        };
        app.lock().toast(msg);
    });
}

/// Pendrive com escrita liberada (ex.: no PC de desenvolvimento).
fn copy_direct(file: &Path, drive: &Path) -> std::io::Result<()> {
    let dest = drive.join("records");
    std::fs::create_dir_all(&dest)?;
    let target = dest.join(file.file_name().unwrap_or_default());
    std::fs::copy(file, &target)?;
    std::fs::File::open(&target)?.sync_all()
}

/// No aparelho (pendrive só leitura): a ferramenta libera a escrita só durante a cópia.
fn copy_with_tool(file: &Path, drive: &Path) -> Result<(), String> {
    let out = std::process::Command::new("sudo")
        .args(["-n", "/usr/local/sbin/dj-os-pendrive", "copiar"])
        .arg(file)
        .arg(drive)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stdout).lines().find_map(|l| l.strip_prefix("ERRO ").map(str::to_string));
        Err(err.unwrap_or_else(|| "o pendrive não aceitou gravação".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cabecalho_wav_valido() {
        let h = wav_header(48000, 4 * 48000);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(h[4..8].try_into().unwrap()), 36 + 4 * 48000);
        assert_eq!(u32::from_le_bytes(h[24..28].try_into().unwrap()), 48000);
        assert_eq!(u16::from_le_bytes(h[22..24].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(h[40..44].try_into().unwrap()), 4 * 48000);
    }
}
