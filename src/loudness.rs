//! Volume percebido da música (LUFS, norma ITU-R BS.1770 / EBU R128), para o ganho automático:
//! músicas gravadas mais altas ou mais baixas entram no mesmo nível.
//!
//! O áudio passa pelo filtro "K" (realça os agudos e corta os graves muito baixos, como o ouvido),
//! é medido em blocos de 400 ms, e os trechos de silêncio e os muito baixos ficam de fora.

/// Nível de referência do ganho automático.
pub const TARGET_LUFS: f32 = -9.0;
/// Limites da correção (não aumenta demais uma música muito baixa).
pub const MIN_GAIN_DB: f32 = -12.0;
pub const MAX_GAIN_DB: f32 = 6.0;

/// Correção em dB para levar a música ao nível de referência.
pub fn gain_db(lufs: f32) -> f32 {
    if lufs.is_finite() {
        (TARGET_LUFS - lufs).clamp(MIN_GAIN_DB, MAX_GAIN_DB)
    } else {
        0.0
    }
}

/// Filtro de 2ª ordem (forma direta II transposta).
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// Os dois estágios do filtro K para a taxa `sr` (coeficientes da norma, recalculados).
fn k_filter(sr: f64) -> (Biquad, Biquad) {
    // Estágio 1: realce de agudos (+4 dB acima de ~1,7 kHz).
    let (f0, g, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
    let k = (std::f64::consts::PI * f0 / sr).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b: [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        z: [0.0; 2],
    };
    // Estágio 2: corta abaixo de ~38 Hz.
    let (f0, q) = (38.13547087602444, 0.5003270373238773);
    let k = (std::f64::consts::PI * f0 / sr).tan();
    let a0 = 1.0 + k / q + k * k;
    let hp = Biquad { b: [1.0, -2.0, 1.0], a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0], z: [0.0; 2] };
    (shelf, hp)
}

/// Volume integrado em LUFS de áudio estéreo intercalado. `None` se for quase só silêncio.
pub fn measure(data: &[f32], sr: f64) -> Option<f32> {
    let frames = data.len() / 2;
    let block = (0.4 * sr) as usize;
    let hop = (0.1 * sr) as usize;
    if frames < block || hop == 0 {
        return None;
    }
    // Energia filtrada de cada pedaço de 100 ms (soma dos dois canais).
    let mut filters = [k_filter(sr), k_filter(sr)];
    let mut hops = Vec::with_capacity(frames / hop + 1);
    let mut acc = 0.0f64;
    for f in 0..frames {
        for (c, (shelf, hp)) in filters.iter_mut().enumerate() {
            let y = hp.process(shelf.process(data[f * 2 + c] as f64));
            acc += y * y;
        }
        if (f + 1) % hop == 0 {
            hops.push(acc);
            acc = 0.0;
        }
    }
    // Blocos de 400 ms (4 pedaços), andando de 100 em 100 ms.
    let blocks: Vec<f64> = hops.windows(4).map(|w| w.iter().sum::<f64>() / block as f64).collect();
    let lufs = |ms: f64| -0.691 + 10.0 * ms.log10();
    // Portão absoluto (-70 LUFS) e relativo (10 LU abaixo da média).
    let above = |thr: f64| blocks.iter().copied().filter(move |&ms| ms > 0.0 && lufs(ms) > thr);
    let n_abs = above(-70.0).count();
    if n_abs == 0 {
        return None;
    }
    let mean_abs = above(-70.0).sum::<f64>() / n_abs as f64;
    let rel = lufs(mean_abs) - 10.0;
    let gated: Vec<f64> = above(rel.max(-70.0)).collect();
    let mean = gated.iter().sum::<f64>() / gated.len().max(1) as f64;
    Some(lufs(mean) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, amp: f32, secs: f64, sr: f64) -> Vec<f32> {
        (0..(secs * sr) as usize)
            .flat_map(|i| {
                let v = amp * (2.0 * std::f64::consts::PI * freq * i as f64 / sr).sin() as f32;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn seno_de_1khz_mede_o_esperado() {
        // Pela norma, um seno de 997 Hz nos dois canais mede ~20·log10(amplitude) LUFS.
        let l = measure(&sine(997.0, 0.5, 10.0, 48000.0), 48000.0).unwrap();
        assert!((l - -6.02).abs() < 0.2, "seno a -6 dBFS: {l:.2} LUFS");
        let l2 = measure(&sine(997.0, 0.25, 10.0, 44100.0), 44100.0).unwrap();
        assert!((l - l2 - 6.02).abs() < 0.2, "metade da amplitude = 6 dB a menos ({l:.2} → {l2:.2})");
    }

    #[test]
    fn silencio_fica_de_fora() {
        // Música com metade silêncio mede o mesmo que a parte com som.
        let mut d = sine(997.0, 0.5, 10.0, 48000.0);
        let so_som = measure(&d, 48000.0).unwrap();
        d.extend(std::iter::repeat(0.0).take(48000 * 2 * 10));
        let com_silencio = measure(&d, 48000.0).unwrap();
        assert!((so_som - com_silencio).abs() < 0.3, "{so_som:.2} vs {com_silencio:.2}");
        assert_eq!(measure(&vec![0.0; 48000 * 2 * 5], 48000.0), None);
    }

    #[test]
    fn correcao_tem_limites() {
        assert_eq!(gain_db(-9.0), 0.0);
        assert_eq!(gain_db(-5.0), -4.0, "música alta desce");
        assert_eq!(gain_db(-30.0), MAX_GAIN_DB, "música muito baixa sobe no máximo +6 dB");
        assert_eq!(gain_db(f32::NAN), 0.0);
    }
}
