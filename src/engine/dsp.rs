//! Blocos básicos de processamento de sinal: filtros, suavização e linhas de atraso.

use std::f32::consts::PI;

/// Coeficientes de um filtro biquad (fórmulas RBJ), já normalizados.
#[derive(Clone, Copy, Debug)]
pub struct Coefs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Default for Coefs {
    fn default() -> Self {
        Self::identity()
    }
}

impl Coefs {
    pub const fn identity() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 }
    }

    /// Retorna (cos w0, alpha) para a frequência e Q pedidos.
    fn prep(fs: f32, f: f32, q: f32) -> (f32, f32) {
        let f = f.clamp(10.0, fs * 0.45);
        let w0 = 2.0 * PI * f / fs;
        let (s, c) = w0.sin_cos();
        (c, s / (2.0 * q.max(0.05)))
    }

    pub fn lowpass(fs: f32, f: f32, q: f32) -> Self {
        let (c, alpha) = Self::prep(fs, f, q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - c) * 0.5 / a0,
            b1: (1.0 - c) / a0,
            b2: (1.0 - c) * 0.5 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    pub fn highpass(fs: f32, f: f32, q: f32) -> Self {
        let (c, alpha) = Self::prep(fs, f, q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 + c) * 0.5 / a0,
            b1: -(1.0 + c) / a0,
            b2: (1.0 + c) * 0.5 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// Passa-banda com ganho 0 dB no pico.
    pub fn bandpass(fs: f32, f: f32, q: f32) -> Self {
        let (c, alpha) = Self::prep(fs, f, q);
        let a0 = 1.0 + alpha;
        Self { b0: alpha / a0, b1: 0.0, b2: -alpha / a0, a1: -2.0 * c / a0, a2: (1.0 - alpha) / a0 }
    }
}

/// Biquad de um canal (forma direta II transposta).
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    pub c: Coefs,
    s1: f32,
    s2: f32,
}

impl Biquad {
    pub fn new(c: Coefs) -> Self {
        Self { c, s1: 0.0, s2: 0.0 }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let c = &self.c;
        let y = c.b0 * x + self.s1;
        self.s1 = c.b1 * x - c.a1 * y + self.s2;
        self.s2 = c.b2 * x - c.a2 * y;
        y
    }

    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }
}

/// Biquad estéreo com os mesmos coeficientes nos dois canais.
#[derive(Clone, Copy, Default)]
pub struct StereoBiquad {
    l: Biquad,
    r: Biquad,
}

impl StereoBiquad {
    pub fn new(c: Coefs) -> Self {
        Self { l: Biquad::new(c), r: Biquad::new(c) }
    }

    pub fn set(&mut self, c: Coefs) {
        self.l.c = c;
        self.r.c = c;
    }

    #[inline]
    pub fn process(&mut self, x: [f32; 2]) -> [f32; 2] {
        [self.l.process(x[0]), self.r.process(x[1])]
    }

    pub fn reset(&mut self) {
        self.l.reset();
        self.r.reset();
    }
}

/// Suavizador de primeira ordem, evita estalos quando um parâmetro muda.
#[derive(Clone, Copy)]
pub struct Smooth {
    pub cur: f32,
    pub target: f32,
    coef: f32,
}

impl Smooth {
    pub fn new(v: f32, coef: f32) -> Self {
        Self { cur: v, target: v, coef }
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        self.cur += (self.target - self.cur) * self.coef;
        self.cur
    }

    pub fn set(&mut self, v: f32) {
        self.target = v;
    }
}

pub fn db(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Linha de atraso estéreo com leitura fracionária (interpolação linear).
pub struct DelayLine {
    buf: Vec<[f32; 2]>,
    w: usize,
}

impl DelayLine {
    pub fn new(len: usize) -> Self {
        Self { buf: vec![[0.0; 2]; len.max(8)], w: 0 }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    #[inline]
    pub fn read(&self, delay: f32) -> [f32; 2] {
        let n = self.buf.len();
        let d = delay.clamp(1.0, (n - 3) as f32);
        let i = d.floor();
        let t = d - i;
        let i = i as usize;
        let a = self.buf[(self.w + n - i) % n];
        let b = self.buf[(self.w + n - i - 1) % n];
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }

    #[inline]
    pub fn write(&mut self, v: [f32; 2]) {
        self.w += 1;
        if self.w == self.buf.len() {
            self.w = 0;
        }
        self.buf[self.w] = v;
    }

    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = [0.0; 2]);
    }
}

/// Saturação suave para o master não estourar de forma dura.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    const K: f32 = 0.89;
    let a = x.abs();
    if a <= K {
        x
    } else {
        x.signum() * (K + (1.0 - K) * ((a - K) / (1.0 - K)).tanh())
    }
}
