//! Efeitos de deck.
//!
//! Regras de comportamento (iguais para todos os efeitos):
//! - O tempo segue o BPM do deck (`beats`), inclusive o LFO de flanger, phaser e wahwah.
//! - Echo, ping pong e reverb deixam a cauda soando ao desligar: só a entrada é cortada.
//! - O knob LEVEL/DEPTH é só a força do efeito (quanto do som do efeito entra).
//! - O "corte" é a divisão de batida (`beats`): 1/16 até 16, incluindo 3/4 e 1 e meia.
//! - Todo parâmetro é suavizado para não estalar.

use super::dsp::*;
use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FxKind {
    #[default]
    Echo,
    PingPong,
    Reverb,
    Flanger,
    Phaser,
    Wahwah,
    Noise,
    Cut,
}

impl FxKind {
    pub const ALL: [FxKind; 8] = [
        FxKind::Echo,
        FxKind::PingPong,
        FxKind::Reverb,
        FxKind::Flanger,
        FxKind::Phaser,
        FxKind::Wahwah,
        FxKind::Noise,
        FxKind::Cut,
    ];

    pub fn name(self) -> &'static str {
        match self {
            FxKind::Echo => "ECHO",
            FxKind::PingPong => "PING PONG",
            FxKind::Reverb => "REVERB",
            FxKind::Flanger => "FLANGER",
            FxKind::Phaser => "PHASER",
            FxKind::Wahwah => "WAHWAH",
            FxKind::Noise => "NOISE",
            FxKind::Cut => "CUT",
        }
    }

    pub fn default_beats(self) -> f32 {
        match self {
            FxKind::Echo | FxKind::PingPong => 0.5,
            FxKind::Flanger | FxKind::Phaser => 4.0,
            FxKind::Wahwah => 1.0,
            FxKind::Cut => 0.25,
            FxKind::Reverb | FxKind::Noise => 1.0,
        }
    }

    pub fn uses_beats(self) -> bool {
        !matches!(self, FxKind::Reverb | FxKind::Noise)
    }

    fn has_tail(self) -> bool {
        matches!(self, FxKind::Echo | FxKind::PingPong | FxKind::Reverb)
    }

    pub fn next(self, dir: i32) -> FxKind {
        let i = Self::ALL.iter().position(|k| *k == self).unwrap_or(0) as i32;
        Self::ALL[(i + dir).rem_euclid(Self::ALL.len() as i32) as usize]
    }
}

/// Tamanhos de batida disponíveis para os efeitos.
pub const BEATS: [f32; 11] = [1.0 / 16.0, 1.0 / 8.0, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 4.0, 8.0, 16.0];

pub fn step_beats(b: f32, dir: i32) -> f32 {
    let i = BEATS.iter().position(|x| (x - b).abs() < 1e-4).unwrap_or(3) as i32;
    BEATS[(i + dir).clamp(0, BEATS.len() as i32 - 1) as usize]
}

pub fn beats_label(b: f32) -> String {
    if !(b > 0.0) {
        "--".into()
    } else if (b - 0.75).abs() < 1e-3 {
        "3/4".into()
    } else if b >= 1.0 && (b - b.floor() - 0.5).abs() < 1e-3 {
        format!("{} 1/2", b.floor() as i32)
    } else if b >= 1.0 {
        format!("{}", b.round() as i32)
    } else {
        format!("1/{}", (1.0 / b).round() as i32)
    }
}

/// Contexto rítmico do deck no instante atual.
pub struct FxCtx {
    /// Duração de uma batida em amostras de saída.
    pub beat_len: f32,
    /// Posição em batidas na grade da música (para sincronizar LFOs).
    pub beat_pos: f64,
}

struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

impl Comb {
    fn new(n: usize) -> Self {
        Self { buf: vec![0.0; n.max(2)], i: 0, store: 0.0 }
    }

    #[inline]
    fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.i];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.i] = x + self.store * feedback;
        self.i += 1;
        if self.i == self.buf.len() {
            self.i = 0;
        }
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

impl Allpass {
    fn new(n: usize) -> Self {
        Self { buf: vec![0.0; n.max(2)], i: 0 }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = x + b * 0.5;
        self.i += 1;
        if self.i == self.buf.len() {
            self.i = 0;
        }
        b - x
    }
}

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;

pub struct FxSlot {
    pub kind: FxKind,
    pub active: bool,
    pub beats: f32,
    level: Smooth,
    input: Smooth,
    tail: f32,
    fs: f32,
    ctr: u32,
    // echo / ping pong
    delay: DelayLine,
    delay_time: f32,
    fb_lp: [f32; 2],
    wet_hp: StereoBiquad,
    // reverb
    combs: [Vec<Comb>; 2],
    aps: [Vec<Allpass>; 2],
    // flanger
    mod_line: DelayLine,
    // phaser
    ap: [[f32; 2]; 6],
    ph_a: f32,
    ph_fb: [f32; 2],
    // wahwah
    bp: StereoBiquad,
    // noise
    seed: u32,
    noise_hp: StereoBiquad,
    // cut
    gate: f32,
}

impl FxSlot {
    pub fn new(kind: FxKind, fs: f32) -> Self {
        let scale = fs / 44100.0;
        let combs = [0, SPREAD].map(|off| COMBS.iter().map(|&n| Comb::new(((n + off) as f32 * scale) as usize)).collect());
        let aps = [0, SPREAD].map(|off| ALLPASSES.iter().map(|&n| Allpass::new(((n + off) as f32 * scale) as usize)).collect());
        Self {
            kind,
            active: false,
            beats: kind.default_beats(),
            level: Smooth::new(0.0, 0.002),
            input: Smooth::new(0.0, 0.004),
            tail: 0.0,
            fs,
            ctr: 0,
            delay: DelayLine::new((fs * 4.0) as usize),
            delay_time: 0.0,
            fb_lp: [0.0; 2],
            wet_hp: StereoBiquad::default(),
            combs,
            aps,
            mod_line: DelayLine::new((fs * 0.02) as usize),
            ap: [[0.0; 2]; 6],
            ph_a: 0.0,
            ph_fb: [0.0; 2],
            bp: StereoBiquad::default(),
            seed: 0x1234_5678,
            noise_hp: StereoBiquad::default(),
            gate: 1.0,
        }
    }

    pub fn level(&self) -> f32 {
        self.level.target
    }

    pub fn set_level(&mut self, v: f32) {
        self.level.set(v.clamp(0.0, 1.0));
    }

    pub fn set_kind(&mut self, k: FxKind) {
        if k == self.kind {
            return;
        }
        self.kind = k;
        self.beats = k.default_beats();
        self.reset_state();
    }

    pub fn set_active(&mut self, on: bool) {
        if on && !self.active && self.input.cur < 1e-4 && self.tail < 1e-5 {
            // Efeito parado de vez: começa limpo, sem eco velho guardado.
            self.reset_state();
        }
        self.active = on;
        self.input.set(if on { 1.0 } else { 0.0 });
    }

    /// Indica se ainda sai som do efeito (ligado ou com cauda).
    pub fn sounding(&self) -> bool {
        self.active || self.input.cur > 1e-4 || self.tail > 1e-5
    }

    fn reset_state(&mut self) {
        self.delay.clear();
        self.delay_time = 0.0;
        self.fb_lp = [0.0; 2];
        self.wet_hp.reset();
        for side in self.combs.iter_mut() {
            for c in side.iter_mut() {
                c.buf.iter_mut().for_each(|s| *s = 0.0);
                c.store = 0.0;
            }
        }
        for side in self.aps.iter_mut() {
            for a in side.iter_mut() {
                a.buf.iter_mut().for_each(|s| *s = 0.0);
            }
        }
        self.mod_line.clear();
        self.ap = [[0.0; 2]; 6];
        self.ph_fb = [0.0; 2];
        self.bp.reset();
        self.noise_hp.reset();
        self.gate = 1.0;
        self.tail = 0.0;
    }

    #[inline]
    fn track_tail(&mut self, w: [f32; 2]) {
        let a = w[0].abs().max(w[1].abs());
        self.tail = if a > self.tail { a } else { self.tail * 0.9995 };
    }

    #[inline]
    fn lfo(&self, ctx: &FxCtx) -> f32 {
        let ph = (ctx.beat_pos / self.beats.max(1e-3) as f64).rem_euclid(1.0) as f32;
        0.5 - 0.5 * (2.0 * PI * ph).cos()
    }

    #[inline]
    pub fn process(&mut self, x: [f32; 2], ctx: &FxCtx) -> [f32; 2] {
        let lvl = self.level.next();
        let inp = self.input.next();
        if inp < 1e-4 && (!self.kind.has_tail() || self.tail < 1e-5) {
            return x;
        }
        self.ctr = self.ctr.wrapping_add(1);
        let update = self.ctr % 32 == 0;
        let fs = self.fs;

        match self.kind {
            FxKind::Echo | FxKind::PingPong => {
                let max = self.delay.len() as f32 - 4.0;
                let target = (self.beats * ctx.beat_len).clamp(16.0, max);
                if self.delay_time <= 0.0 {
                    self.delay_time = target;
                }
                // Mudança de tempo desliza devagar, como fita.
                self.delay_time += (target - self.delay_time) * 0.0007;
                let r = self.delay.read(self.delay_time);
                let fbk = 0.5 + 0.2 * lvl;
                self.fb_lp[0] += (r[0] - self.fb_lp[0]) * 0.45;
                self.fb_lp[1] += (r[1] - self.fb_lp[1]) * 0.45;
                let fb = self.fb_lp;
                let w = if self.kind == FxKind::Echo {
                    [x[0] * inp + fb[0] * fbk, x[1] * inp + fb[1] * fbk]
                } else {
                    [(x[0] + x[1]) * 0.5 * inp + fb[1] * fbk, fb[0] * fbk]
                };
                self.delay.write(w);
                if update {
                    // Corte fixo de grave no eco para não embolar o grave da música.
                    self.wet_hp.set(Coefs::highpass(fs, 110.0, 0.7071));
                }
                let wet = self.wet_hp.process([r[0] * lvl, r[1] * lvl]);
                self.track_tail(wet);
                [x[0] + wet[0], x[1] + wet[1]]
            }
            FxKind::Reverb => {
                let m = (x[0] + x[1]) * 0.015 * inp;
                let room = 0.70 + 0.27 * lvl;
                let damp = 0.25;
                let mut out = [0.0f32; 2];
                for ch in 0..2 {
                    let mut acc = 0.0;
                    for c in self.combs[ch].iter_mut() {
                        acc += c.process(m, room, damp);
                    }
                    for a in self.aps[ch].iter_mut() {
                        acc = a.process(acc);
                    }
                    out[ch] = acc;
                }
                if update {
                    self.wet_hp.set(Coefs::highpass(fs, 160.0, 0.7071));
                }
                let g = lvl * 2.5;
                let wet = self.wet_hp.process([out[0] * g, out[1] * g]);
                self.track_tail(wet);
                [x[0] + wet[0], x[1] + wet[1]]
            }
            FxKind::Flanger => {
                let l = self.lfo(ctx);
                let d = (0.0007 + 0.0045 * l) * fs;
                let r = self.mod_line.read(d);
                let fb = 0.6 * lvl;
                self.mod_line.write([x[0] + r[0] * fb, x[1] + r[1] * fb]);
                let m = 0.5 * lvl * inp;
                [x[0] * (1.0 - m) + r[0] * m, x[1] * (1.0 - m) + r[1] * m]
            }
            FxKind::Phaser => {
                if self.ctr % 8 == 0 {
                    let f = 250.0 * 12f32.powf(self.lfo(ctx));
                    let t = (PI * f / fs).tan();
                    self.ph_a = (t - 1.0) / (t + 1.0);
                }
                let a = self.ph_a;
                let m = 0.5 * lvl * inp;
                let mut out = [0.0f32; 2];
                for ch in 0..2 {
                    let mut y = x[ch] + self.ph_fb[ch] * 0.6 * lvl;
                    for s in 0..6 {
                        let o = a * y + self.ap[s][ch];
                        self.ap[s][ch] = y - a * o;
                        y = o;
                    }
                    self.ph_fb[ch] = y;
                    out[ch] = x[ch] * (1.0 - m) + y * m;
                }
                out
            }
            FxKind::Wahwah => {
                if self.ctr % 16 == 0 {
                    let fc = 380.0 * 7.5f32.powf(self.lfo(ctx));
                    self.bp.set(Coefs::bandpass(fs, fc, 3.5));
                }
                let w = self.bp.process(x);
                let m = lvl * inp;
                [x[0] * (1.0 - m) + w[0] * 2.2 * m, x[1] * (1.0 - m) + w[1] * 2.2 * m]
            }
            FxKind::Noise => {
                let mut n = [0.0f32; 2];
                for v in n.iter_mut() {
                    self.seed ^= self.seed << 13;
                    self.seed ^= self.seed >> 17;
                    self.seed ^= self.seed << 5;
                    *v = (self.seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
                }
                if update {
                    self.noise_hp.set(Coefs::highpass(fs, 250.0 * 48f32.powf(lvl), 0.9));
                }
                let nn = self.noise_hp.process(n);
                let amp = 0.35 * lvl * lvl * inp;
                [x[0] + nn[0] * amp, x[1] + nn[1] * amp]
            }
            FxKind::Cut => {
                let ph = (ctx.beat_pos / self.beats.max(1e-3) as f64).rem_euclid(1.0);
                let open = if ph < 0.5 { 1.0 } else { 0.0 };
                self.gate += (open - self.gate) * 0.02;
                let g = 1.0 - lvl * inp * (1.0 - self.gate);
                [x[0] * g, x[1] * g]
            }
        }
    }
}
