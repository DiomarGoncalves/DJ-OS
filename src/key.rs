//! Tom da música (key), mostrado no formato Camelot (1A-12B) usado para mixagem harmônica:
//! tons vizinhos na roda (mesmo número, ou ±1 com a mesma letra) combinam.
//!
//! Detecção: energia de cada uma das 12 notas (5 oitavas, Dó2 a Si6) ao longo da música,
//! comparada com os perfis de Krumhansl-Kessler dos 24 tons maiores e menores.
//!
//! Guardado como um número: 0-11 = tônica (Dó = 0) em maior, 12-23 = em menor.

/// Como o tom fica guardado (1 byte): ainda não analisado, analisado sem tom definido, ou o tom
/// (+1).
pub const RAW_UNKNOWN: u8 = 0;
pub const RAW_NONE: u8 = 255;

pub fn to_raw(key: Option<u8>) -> u8 {
    key.map_or(RAW_NONE, |k| k + 1)
}

pub fn from_raw(raw: u8) -> Option<u8> {
    (1..=24).contains(&raw).then(|| raw - 1)
}

/// Perfis de Krumhansl-Kessler (peso de cada grau da escala a partir da tônica).
const MAJOR: [f32; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const MINOR: [f32; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];

/// Taxa usada na análise (o áudio é reduzido para ela; as notas vão até ~2 kHz).
const RATE: f64 = 11025.0;
const FRAME: usize = 4096;
/// Analisa até este tanto do meio da música (o começo e o fim costumam ser só batida).
const MAX_SECS: f64 = 150.0;

/// Detecta o tom de áudio estéreo intercalado. `None` se não houver notas suficientes.
pub fn detect(data: &[f32], sr: f64) -> Option<u8> {
    // Mono e reduzido para ~11 kHz (média de blocos: filtra o que passaria de 5,5 kHz).
    let step = (sr / RATE).round().max(1.0) as usize;
    let frames = data.len() / 2;
    let mono: Vec<f32> = (0..frames / step)
        .map(|i| {
            let s: f32 = (i * step..(i + 1) * step).map(|f| data[f * 2] + data[f * 2 + 1]).sum();
            s / (2 * step) as f32
        })
        .collect();
    let rate = sr / step as f64;
    // Só o miolo da música.
    let max = (MAX_SECS * rate) as usize;
    let (from, to) = if mono.len() > max { ((mono.len() - max) / 2, (mono.len() + max) / 2) } else { (0, mono.len()) };
    let audio = &mono[from..to];
    if audio.len() < FRAME * 4 {
        return None;
    }
    let window: Vec<f32> = (0..FRAME).map(|n| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / FRAME as f32).cos()).collect();
    // Goertzel para as notas MIDI 36 (Dó2, 65 Hz) a 95 (Si6, 1976 Hz).
    let coefs: Vec<(usize, f32)> = (36..96)
        .map(|m| {
            let f = 440.0 * 2f64.powf((m as f64 - 69.0) / 12.0);
            ((m % 12) as usize, (2.0 * (2.0 * std::f64::consts::PI * f / rate).cos()) as f32)
        })
        .collect();
    let mut chroma = [0.0f32; 12];
    let mut buf = vec![0.0f32; FRAME];
    for chunk in audio.chunks_exact(FRAME) {
        for (b, (x, w)) in buf.iter_mut().zip(chunk.iter().zip(&window)) {
            *b = x * w;
        }
        let mut frame = [0.0f32; 12];
        for &(pc, c) in &coefs {
            let (mut s1, mut s2) = (0.0f32, 0.0f32);
            for &x in &buf {
                let s0 = x + c * s1 - s2;
                s2 = s1;
                s1 = s0;
            }
            frame[pc] += (s1 * s1 + s2 * s2 - c * s1 * s2).max(0.0).sqrt();
        }
        // Cada trecho pesa igual (uma parte alta não domina a música toda).
        let total: f32 = frame.iter().sum();
        if total > 1e-6 {
            for (c, f) in chroma.iter_mut().zip(frame) {
                *c += f / total;
            }
        }
    }
    best_key(&chroma)
}

/// Tom cujo perfil mais se parece com a energia das notas (correlação de Pearson).
fn best_key(chroma: &[f32; 12]) -> Option<u8> {
    let mean = chroma.iter().sum::<f32>() / 12.0;
    if mean <= 0.0 {
        return None;
    }
    let corr = |profile: &[f32; 12], tonic: usize| {
        let pm = profile.iter().sum::<f32>() / 12.0;
        let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
        for i in 0..12 {
            let x = chroma[(tonic + i) % 12] - mean;
            let y = profile[i] - pm;
            xy += x * y;
            xx += x * x;
            yy += y * y;
        }
        xy / (xx * yy).sqrt().max(1e-9)
    };
    (0..24u8)
        .map(|k| (k, if k < 12 { corr(&MAJOR, k as usize) } else { corr(&MINOR, k as usize - 12) }))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .filter(|(_, c)| c.is_finite() && *c > 0.2)
        .map(|(k, _)| k)
}

/// Número da roda Camelot (1-12) e letra (A = menor, B = maior).
pub fn camelot(key: u8) -> (u8, char) {
    let minor = key >= 12;
    // Em menor, o número é o do tom maior relativo (3 semitons acima).
    let tonic = if minor { (key as usize - 12 + 3) % 12 } else { key as usize };
    (((7 * tonic) % 12 + 7) as u8 % 12 + 1, if minor { 'A' } else { 'B' })
}

/// Texto curto para a tela, ex.: "8A".
pub fn label(key: u8) -> String {
    let (n, l) = camelot(key);
    format!("{n}{l}")
}

/// Nome tradicional, ex.: "Lá menor".
pub fn name(key: u8) -> String {
    const NOTES: [&str; 12] = ["Dó", "Dó#", "Ré", "Mi♭", "Mi", "Fá", "Fá#", "Sol", "Lá♭", "Lá", "Si♭", "Si"];
    format!("{} {}", NOTES[key as usize % 12], if key >= 12 { "menor" } else { "maior" })
}

/// Cor da posição na roda Camelot (tons que combinam têm cores vizinhas).
pub fn color(key: u8) -> [u8; 3] {
    let (n, l) = camelot(key);
    let h = (n as f32 - 1.0) / 12.0;
    // Arco-íris pela roda; menor (A) um pouco mais escuro que maior (B).
    let v = if l == 'A' { 0.85 } else { 1.0 };
    let (r, g, b) = hsv(h, 0.65, v);
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

fn hsv(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
    match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sequência de acordes (2 s cada) com notas puras + harmônicos, como uma música simples.
    fn chords(progression: &[[u8; 3]], sr: f64) -> Vec<f32> {
        let mut out = Vec::new();
        for _ in 0..4 {
            for chord in progression {
                let n = (2.0 * sr) as usize;
                for i in 0..n {
                    let t = i as f64 / sr;
                    let mut s = 0.0;
                    for &m in chord {
                        for (oct, amp) in [(0.0, 1.0), (12.0, 0.4), (-12.0, 0.5)] {
                            let f = 440.0 * 2f64.powf((m as f64 + oct - 69.0) / 12.0);
                            s += amp * (2.0 * std::f64::consts::PI * f * t).sin();
                        }
                    }
                    let v = (s * 0.1) as f32;
                    out.push(v);
                    out.push(v);
                }
            }
        }
        out
    }

    #[test]
    fn camelot_dos_tons_conhecidos() {
        assert_eq!(label(0), "8B", "Dó maior");
        assert_eq!(label(7), "9B", "Sol maior");
        assert_eq!(label(5), "7B", "Fá maior");
        assert_eq!(label(12 + 9), "8A", "Lá menor");
        assert_eq!(label(12 + 4), "9A", "Mi menor");
        assert_eq!(label(12 + 2), "7A", "Ré menor");
        assert_eq!(name(12 + 9), "Lá menor");
    }

    #[test]
    fn detecta_do_maior_e_la_menor() {
        let sr = 44100.0;
        // Dó - Fá - Sol - Dó (C4 E4 G4, F4 A4 C5, G4 B4 D5).
        let c_major = chords(&[[60, 64, 67], [65, 69, 72], [67, 71, 74], [60, 64, 67]], sr);
        assert_eq!(detect(&c_major, sr).map(label).as_deref(), Some("8B"));
        // Lám - Rém - Mi - Lám (A3 C4 E4, D4 F4 A4, E4 G#4 B4).
        let a_minor = chords(&[[57, 60, 64], [62, 65, 69], [64, 68, 71], [57, 60, 64]], sr);
        assert_eq!(detect(&a_minor, sr).map(label).as_deref(), Some("8A"));
    }

    #[test]
    fn silencio_nao_tem_tom() {
        assert_eq!(detect(&vec![0.0; 44100 * 20], 44100.0), None);
    }
}
