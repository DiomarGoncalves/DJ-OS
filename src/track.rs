//! Leitura de arquivos de áudio e análise (waveform em 3 bandas, BPM e grade de batidas).

use crate::engine::dsp::{Biquad, Coefs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymErr;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;

/// Resolução da waveform: colunas por segundo de música.
pub const BINS_PER_SEC: f64 = 150.0;

static NEXT_GEN: AtomicU64 = AtomicU64::new(1);

/// Amplitude de pico por coluna em cada banda (0-255).
#[derive(Clone)]
pub struct Waveform {
    pub low: Vec<u8>,
    pub mid: Vec<u8>,
    pub high: Vec<u8>,
}

pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub sr: f64,
    pub frames: usize,
    /// Amostras estéreo intercaladas (L, R, L, R...).
    pub data: Vec<f32>,
    pub wave: Waveform,
    /// 0 quando não foi possível detectar.
    pub bpm: f64,
    /// Posição (em frames) da primeira batida da grade.
    pub first_beat: f64,
    /// Identificador único desta carga.
    pub gen: u64,
}

impl std::fmt::Debug for Track {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Track({})", self.title)
    }
}

impl Track {
    #[inline]
    fn frame(&self, i: isize) -> (f32, f32) {
        if i < 0 || i as usize >= self.frames {
            (0.0, 0.0)
        } else {
            let j = i as usize * 2;
            (self.data[j], self.data[j + 1])
        }
    }

    /// Leitura com interpolação Hermite (boa qualidade para pitch e scratch).
    #[inline]
    pub fn sample(&self, pos: f64) -> (f32, f32) {
        let fl = pos.floor();
        let t = (pos - fl) as f32;
        let i = fl as isize;
        let (am, bm) = self.frame(i - 1);
        let (a0, b0) = self.frame(i);
        let (a1, b1) = self.frame(i + 1);
        let (a2, b2) = self.frame(i + 2);
        (hermite(am, a0, a1, a2, t), hermite(bm, b0, b1, b2, t))
    }

    pub fn bpm_or_default(&self) -> f64 {
        if self.bpm > 0.0 {
            self.bpm
        } else {
            120.0
        }
    }

    /// Frames por batida.
    pub fn beat_len(&self) -> f64 {
        self.sr * 60.0 / self.bpm_or_default()
    }

    pub fn beats_at(&self, pos: f64) -> f64 {
        (pos - self.first_beat) / self.beat_len()
    }

    pub fn pos_of_beat(&self, b: f64) -> f64 {
        self.first_beat + b * self.beat_len()
    }

    pub fn duration_secs(&self) -> f64 {
        self.frames as f64 / self.sr
    }

    pub fn display_name(&self) -> String {
        if self.artist.is_empty() {
            self.title.clone()
        } else {
            format!("{} - {}", self.artist, self.title)
        }
    }
}

#[inline]
fn hermite(xm1: f32, x0: f32, x1: f32, x2: f32, t: f32) -> f32 {
    let c1 = 0.5 * (x1 - xm1);
    let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
    let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
    ((c3 * t + c2) * t + c1) * t + x0
}

/// Separa "Artista - Título" a partir do nome do arquivo.
pub fn names_from_path(path: &Path) -> (String, String) {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    match stem.split_once(" - ") {
        Some((a, t)) => (t.trim().to_string(), a.trim().to_string()),
        None => (stem, String::new()),
    }
}

fn read_tags(rev: &MetadataRevision, title: &mut String, artist: &mut String) {
    for tag in rev.tags() {
        match tag.std_key {
            Some(StandardTagKey::TrackTitle) => *title = tag.value.to_string(),
            Some(StandardTagKey::Artist) => *artist = tag.value.to_string(),
            _ => {}
        }
    }
}

/// Decodifica o arquivo inteiro para estéreo f32. Retorna (amostras, taxa, título, artista).
fn decode(path: &Path) -> Result<(Vec<f32>, f64, String, String), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("não abriu o arquivo: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("formato não suportado: {e}"))?;

    let (mut title, mut artist) = (String::new(), String::new());
    if let Some(md) = probed.metadata.get() {
        if let Some(rev) = md.current() {
            read_tags(rev, &mut title, &mut artist);
        }
    }
    let mut format = probed.format;
    if let Some(rev) = format.metadata().current() {
        read_tags(rev, &mut title, &mut artist);
    }

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("o arquivo não tem áudio")?;
    let track_id = track.id;
    let sr = track.codec_params.sample_rate.unwrap_or(44100) as f64;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("codec não suportado: {e}"))?;
    let mut data: Vec<f32> = Vec::new();
    if let Some(n) = track.codec_params.n_frames {
        data.reserve(n as usize * 2);
    }

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymErr::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymErr::ResetRequired) => break,
            Err(e) => {
                if data.is_empty() {
                    return Err(format!("erro de leitura: {e}"));
                }
                break;
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let ch = spec.channels.count().max(1);
                let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                buf.copy_interleaved_ref(decoded);
                let s = buf.samples();
                if ch == 1 {
                    for &v in s {
                        data.push(v);
                        data.push(v);
                    }
                } else {
                    for f in s.chunks_exact(ch) {
                        data.push(f[0]);
                        data.push(f[1]);
                    }
                }
            }
            Err(SymErr::DecodeError(_)) => continue,
            Err(e) => return Err(format!("erro ao decodificar: {e}")),
        }
    }
    if data.is_empty() {
        return Err("o arquivo está vazio".into());
    }
    Ok((data, sr, title, artist))
}

/// Carrega e analisa uma música. `analyze = false` pula a detecção de BPM (samples curtos).
pub fn load(path: &Path, analyze: bool) -> Result<Track, String> {
    load_cached(path, analyze, None)
}

/// Como `load`, mas reaproveita a waveform, o BPM e a grade já salvos (sem analisar de novo).
pub fn load_cached(path: &Path, analyze: bool, cached: Option<(Waveform, f64, f64)>) -> Result<Track, String> {
    let (data, sr, mut title, mut artist) = decode(path)?;
    let (ft, fa) = names_from_path(path);
    if title.is_empty() {
        title = ft;
    }
    if artist.is_empty() {
        artist = fa;
    }
    let frames = data.len() / 2;
    let (wave, bpm, first_beat) = match cached {
        Some(c) => c,
        None => {
            let (wave, onset) = waveform(&data, sr);
            let (bpm, first_beat) = if analyze { detect_bpm(&onset, sr) } else { (0.0, 0.0) };
            (wave, bpm, first_beat)
        }
    };
    Ok(Track {
        path: path.to_path_buf(),
        title,
        artist,
        sr,
        frames,
        data,
        wave,
        bpm,
        first_beat,
        gen: NEXT_GEN.fetch_add(1, Ordering::Relaxed),
    })
}

/// Calcula a waveform em 3 bandas e a curva de ataques usada no BPM.
fn waveform(data: &[f32], sr: f64) -> (Waveform, Vec<f32>) {
    let frames = data.len() / 2;
    let hop = (sr / BINS_PER_SEC).round().max(1.0) as usize;
    let nb = frames / hop + 1;
    let fs = sr as f32;
    let mut lp = [Biquad::new(Coefs::lowpass(fs, 220.0, 0.7071)); 2];
    let mut hp = [Biquad::new(Coefs::highpass(fs, 2200.0, 0.7071)); 2];
    let (mut low, mut mid, mut high) = (vec![0f32; nb], vec![0f32; nb], vec![0f32; nb]);
    let mut energy = vec![0f32; nb];
    for i in 0..frames {
        let m = 0.5 * (data[2 * i] + data[2 * i + 1]);
        let l0 = lp[0].process(m);
        let l = lp[1].process(l0);
        let h0 = hp[0].process(m);
        let h = hp[1].process(h0);
        let md = m - l - h;
        let b = i / hop;
        low[b] = low[b].max(l.abs());
        mid[b] = mid[b].max(md.abs());
        high[b] = high[b].max(h.abs());
        energy[b] += m * m + 2.0 * l * l;
    }
    let peak = low.iter().chain(mid.iter()).chain(high.iter()).fold(1e-6f32, |a, &b| a.max(b));
    let q = |v: &Vec<f32>| v.iter().map(|&x| ((x / peak).powf(0.8) * 255.0).min(255.0) as u8).collect();
    let wave = Waveform { low: q(&low), mid: q(&mid), high: q(&high) };

    // Ataques: aumento de energia em escala log, com a média local subtraída.
    let le: Vec<f32> = energy.iter().map(|&e| (e / hop as f32 + 1e-7).ln()).collect();
    let mut onset = vec![0f32; nb];
    for i in 1..nb {
        onset[i] = (le[i] - le[i - 1]).max(0.0);
    }
    let w = BINS_PER_SEC as usize / 2;
    let mut acc: f32 = onset.iter().take(2 * w + 1).sum();
    let mut out = vec![0f32; nb];
    for i in 0..nb {
        let avg = acc / (2 * w + 1) as f32;
        out[i] = (onset[i] - avg).max(0.0);
        if i + w + 1 < nb {
            acc += onset[i + w + 1];
        }
        if i >= w {
            acc -= onset[i - w];
        }
    }
    (wave, out)
}

/// Detecta BPM e a posição da primeira batida. Assume tempo constante.
fn detect_bpm(onset: &[f32], sr: f64) -> (f64, f64) {
    let n = onset.len();
    if n < (BINS_PER_SEC * 10.0) as usize {
        return (0.0, 0.0);
    }
    // 1) Autocorrelação grossa entre 70 e 180 BPM, com preferência suave por ~120.
    let lag_of = |bpm: f64| BINS_PER_SEC * 60.0 / bpm;
    let (lmin, lmax) = (lag_of(180.0).floor() as usize, lag_of(70.0).ceil() as usize);
    let mut best = (0.0f64, 0usize);
    for lag in lmin..=lmax {
        let mut s = 0.0f64;
        for i in 0..n - lag {
            s += (onset[i] * onset[i + lag]) as f64;
        }
        let bpm = BINS_PER_SEC * 60.0 / lag as f64;
        let prior = (-0.5 * ((bpm / 120.0).log2() / 0.9).powi(2)).exp();
        let score = s * prior;
        if score > best.0 {
            best = (score, lag);
        }
    }
    if best.1 == 0 {
        return (0.0, 0.0);
    }
    let coarse = BINS_PER_SEC * 60.0 / best.1 as f64;

    // 2) Refinamento fino: varre BPM e fase somando os ataques sobre a grade.
    let mut fine = (f64::MIN, coarse, 0.0f64);
    let mut bpm = coarse - 2.0;
    while bpm <= coarse + 2.0 {
        let period = lag_of(bpm);
        let mut phase = 0.0;
        while phase < period {
            let mut s = 0.0f64;
            let mut p = phase;
            while (p as usize) < n {
                s += onset[p.round() as usize % n] as f64;
                p += period;
            }
            if s > fine.0 {
                fine = (s, bpm, phase);
            }
            phase += 1.0;
        }
        bpm += 0.02;
    }
    let mut bpm = fine.1;
    if (bpm - bpm.round()).abs() < 0.06 {
        bpm = bpm.round();
    } else {
        bpm = (bpm * 100.0).round() / 100.0;
    }
    let hop = (sr / BINS_PER_SEC).round();
    (bpm, fine.2 * hop)
}

/// Música sintética para os testes: senoide com um clique em cada batida, BPM e grade conhecidos.
#[cfg(test)]
pub fn synthetic(secs: f64, sr: f64, bpm: f64, first_beat_secs: f64) -> Track {
    let frames = (secs * sr) as usize;
    let beat = 60.0 / bpm;
    let mut data = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let t = i as f64 / sr;
        let mut v = 0.3 * (2.0 * std::f64::consts::PI * 220.0 * t).sin();
        let since = (t - first_beat_secs).rem_euclid(beat);
        if t >= first_beat_secs && since < 0.005 {
            v += 0.6;
        }
        data.push(v as f32);
        data.push(v as f32);
    }
    Track {
        path: PathBuf::from("teste.wav"),
        title: "Teste".into(),
        artist: String::new(),
        sr,
        frames,
        data,
        wave: Waveform { low: Vec::new(), mid: Vec::new(), high: Vec::new() },
        bpm,
        first_beat: first_beat_secs * sr,
        gen: NEXT_GEN.fetch_add(1, Ordering::Relaxed),
    }
}
