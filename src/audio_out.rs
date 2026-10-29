//! Abre a saída de áudio e liga o motor nela.
//!
//! Ordem de escolha: variável DJOS_AUDIO (parte do nome do dispositivo), depois a DDJ-REV1
//! com 4 canais (1-2 master, 3-4 fone), depois a saída padrão do sistema.

use crate::engine::Engine;
use crate::shared::{Cmd, SharedSnap};
use crate::track::Track;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, SampleFormat, StreamConfig};
use parking_lot::Mutex;
use std::sync::Arc;

type Slot = Arc<Mutex<Option<Engine>>>;

pub struct AudioHandle {
    _stream: Option<cpal::Stream>,
    _pipewire: Option<crate::pw_out::PwOutput>,
    pub description: String,
}

impl AudioHandle {
    /// Para onde o som está indo agora (muda quando a REV1 é ligada ou desligada).
    pub fn status(&self) -> String {
        self._pipewire.as_ref().map_or_else(|| self.description.clone(), |p| p.status.lock().clone())
    }
}

fn pick_config(dev: &Device, want_ch: u16) -> Option<(StreamConfig, SampleFormat)> {
    let ranges: Vec<_> = dev.supported_output_configs().ok()?.collect();
    let mut best: Option<(i32, StreamConfig, SampleFormat)> = None;
    for r in ranges {
        let fmt = r.sample_format();
        if !matches!(fmt, SampleFormat::F32 | SampleFormat::I16) {
            continue;
        }
        let ch = r.channels();
        for sr in [48000u32, 44100] {
            if r.min_sample_rate().0 <= sr && sr <= r.max_sample_rate().0 {
                let mut score = 0;
                score += if ch == want_ch { 100 } else if ch >= want_ch { 50 } else { 0 };
                score += if sr == 48000 { 10 } else { 5 };
                score += if fmt == SampleFormat::F32 { 2 } else { 0 };
                if best.as_ref().map_or(true, |b| score > b.0) {
                    let cfg = StreamConfig { channels: ch, sample_rate: cpal::SampleRate(sr), buffer_size: BufferSize::Default };
                    best = Some((score, cfg, fmt));
                }
            }
        }
    }
    best.map(|(_, c, f)| (c, f))
}

fn build(
    dev: &Device,
    cfg: &StreamConfig,
    fmt: SampleFormat,
    slot: Slot,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let ch = cfg.channels as usize;
    // O motor só sai do `slot` no primeiro callback; se a abertura falhar ele continua lá.
    let mut engine: Option<Engine> = None;
    let mut run = move |buf: &mut [f32]| {
        if engine.is_none() {
            engine = slot.try_lock().and_then(|mut g| g.take());
            if engine.is_some() && std::env::var_os("DJOS_DEBUG_SNAP").is_some() {
                eprintln!("[áudio] primeiro bloco às {}", chrono::Local::now().format("%H:%M:%S%.3f"));
            }
        }
        match engine.as_mut() {
            Some(e) => e.process(buf, ch),
            None => buf.iter_mut().for_each(|s| *s = 0.0),
        }
    };
    let err = |e| eprintln!("[djos] erro no áudio: {e}");
    match fmt {
        SampleFormat::I16 => {
            let mut tmp: Vec<f32> = Vec::with_capacity(8192 * ch);
            dev.build_output_stream(
                cfg,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    tmp.resize(data.len(), 0.0);
                    run(&mut tmp);
                    for (o, s) in data.iter_mut().zip(tmp.iter()) {
                        *o = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                    }
                },
                err,
                None,
            )
        }
        _ => dev.build_output_stream(
            cfg,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| run(data),
            err,
            None,
        ),
    }
}

pub fn start(
    cmds: rtrb::Consumer<Cmd>,
    trash: rtrb::Producer<Arc<Track>>,
    snap: SharedSnap,
) -> Result<AudioHandle, String> {
    let wanted = std::env::var("DJOS_AUDIO").ok().map(|s| s.to_lowercase());
    let mut parts = Some((cmds, trash));

    // 1º: PipeWire direto (estável); as pontes ALSA ficam como reserva ou quando DJOS_AUDIO pede uma.
    if wanted.is_none() && std::env::var("DJOS_SAIDA").map_or(true, |v| v != "alsa") {
        match crate::pw_out::start(parts.take().expect("filas"), snap.clone()) {
            Ok(out) => {
                let description = out.status.lock().clone();
                eprintln!("[djos] áudio: {description}");
                return Ok(AudioHandle { _stream: None, _pipewire: Some(out), description });
            }
            Err((e, back)) => {
                eprintln!("[djos] PipeWire direto não funcionou ({e}); tentando pelo ALSA");
                parts = Some(back);
            }
        }
    }

    let host = cpal::default_host();
    let devices: Vec<Device> = host.output_devices().map_err(|e| e.to_string())?.collect();
    let name_of = |d: &Device| d.name().unwrap_or_default();

    let mut candidates: Vec<(Device, u16)> = Vec::new();
    if let Some(w) = &wanted {
        // Dispositivo pedido explicitamente: usa só ele (nome exato primeiro), sem cair em outro.
        let mut chosen: Vec<Device> = devices.iter().filter(|d| name_of(d).to_lowercase().contains(w)).cloned().collect();
        chosen.sort_by_key(|d| name_of(d).to_lowercase() != *w);
        if chosen.is_empty() {
            return Err(format!("DJOS_AUDIO={w}: dispositivo não encontrado (veja a lista com --listar-audio)"));
        }
        candidates.extend(chosen.into_iter().map(|d| (d, 4)));
    }
    let rev1 = |d: &&Device| {
        let n = name_of(d).to_uppercase();
        n.contains("DDJREV1") || n.contains("DDJ-REV1")
    };
    // Primeiro o acesso direto ao hardware (menor latência), depois as outras formas.
    let mut rev1_devs: Vec<Device> = devices.iter().filter(rev1).cloned().collect();
    rev1_devs.sort_by_key(|d| if name_of(d).starts_with("hw:") { 0 } else if name_of(d).starts_with("plughw:") { 1 } else { 2 });
    if wanted.is_none() {
        candidates.extend(rev1_devs.into_iter().map(|d| (d, 4)));
        // "pulse" antes de "pipewire": a ponte ALSA→PipeWire às vezes leva 5 s ou nunca começa
        // (medido em 2026-10-29); a ponte "pulse" começou na hora em todos os testes.
        for name in ["pulse", "pipewire"] {
            candidates.extend(devices.iter().filter(|d| name_of(d) == name).cloned().map(|d| (d, 2)));
        }
        if let Some(d) = host.default_output_device() {
            candidates.push((d, 2));
        }
    }

    let mut last_err = String::from("nenhuma saída de áudio encontrada");
    for (dev, want_ch) in candidates {
        let Some((mut cfg, fmt)) = pick_config(&dev, want_ch) else { continue };
        let name = name_of(&dev);
        // DJOS_BUFFER=<frames> força um tamanho de buffer (diagnóstico); "0" usa o padrão do sistema.
        let sizes: Vec<BufferSize> = match std::env::var("DJOS_BUFFER").ok().and_then(|v| v.parse::<u32>().ok()) {
            Some(0) => vec![BufferSize::Default],
            Some(n) => vec![BufferSize::Fixed(n)],
            None => vec![BufferSize::Fixed(256), BufferSize::Default],
        };
        for buf in sizes {
            cfg.buffer_size = buf;
            let Some((c, t)) = parts.take() else { break };
            let slot: Slot = Arc::new(Mutex::new(Some(Engine::new(cfg.sample_rate.0 as f64, c, t, snap.clone()))));
            match build(&dev, &cfg, fmt, slot.clone()).and_then(|s| s.play().map(|_| s).map_err(|e| cpal::BuildStreamError::BackendSpecific { err: cpal::BackendSpecificError { description: e.to_string() } })) {
                Ok(stream) => {
                    let description = format!(
                        "{name} · {} canais · {} Hz{}",
                        cfg.channels,
                        cfg.sample_rate.0,
                        if cfg.channels >= 4 { " · master 1-2, fone 3-4" } else { " · sem saída de fone" }
                    );
                    eprintln!("[djos] áudio: {description} ({})", chrono::Local::now().format("%H:%M:%S%.3f"));
                    return Ok(AudioHandle { _stream: Some(stream), _pipewire: None, description });
                }
                Err(e) => {
                    last_err = format!("{name}: {e}");
                    eprintln!("[djos] tentativa de áudio falhou ({last_err})");
                    // Pega o motor de volta para a próxima tentativa.
                    parts = slot.lock().take().map(Engine::into_parts);
                    if parts.is_none() {
                        return Err(last_err);
                    }
                }
            }
        }
    }
    Err(last_err)
}

/// Lista as saídas de áudio com os canais e taxas que aceitam (para escolher com DJOS_AUDIO).
pub fn list_devices() -> Vec<String> {
    let host = cpal::default_host();
    let Ok(devices) = host.output_devices() else { return Vec::new() };
    devices
        .map(|d| {
            let name = d.name().unwrap_or_default();
            let caps: Vec<String> = d
                .supported_output_configs()
                .map(|r| {
                    r.map(|c| format!("{}ch {}-{} Hz {:?}", c.channels(), c.min_sample_rate().0, c.max_sample_rate().0, c.sample_format()))
                        .collect()
                })
                .unwrap_or_default();
            format!("{name}\n    {}", if caps.is_empty() { "(sem configurações)".into() } else { caps.join("\n    ") })
        })
        .collect()
}
