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

/// Saturação suave (aproximação de tanh): segura a realimentação sem estalar.
#[inline]
fn soft(x: f32) -> f32 {
    x / (1.0 + x * x / (3.0 + x * x / 5.0))
}

/// Troca de tempo do eco: as duas leituras (tempo velho e novo) se cruzam neste tempo.
const ECHO_XFADE: f32 = 2048.0;

pub struct FxSlot {
    pub kind: FxKind,
    pub active: bool,
    pub beats: f32,
    level: Smooth,
    input: Smooth,
    tail: f32,
    fs: f32,
    ctr: u32,
    // echo / ping pong: tempo atual, tempo antigo e quanto falta da troca entre os dois
    delay: DelayLine,
    delay_time: f32,
    delay_old: f32,
    xfade: f32,
    fb_lp: [f32; 2],
    fb_hp: StereoBiquad,
    wet_hp: StereoBiquad,
    // reverb
    plate: Box<super::plate::Plate>,
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
            delay_old: 0.0,
            xfade: 0.0,
            fb_lp: [0.0; 2],
            fb_hp: StereoBiquad::default(),
            wet_hp: StereoBiquad::default(),
            plate: Box::new(super::plate::Plate::new(fs)),
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
        self.xfade = 0.0;
        self.fb_lp = [0.0; 2];
        self.fb_hp.reset();
        self.wet_hp.reset();
        self.plate.clear();
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
            // Eco (base: echo do Mixxx). O tempo é a divisão de batida; ao trocar, a leitura no
            // tempo velho cruza com a do novo em ~40 ms (sem o "fita esticando"). A força aumenta o
            // volume do eco e quantas repetições ele faz. A realimentação passa por um corte de
            // grave e de agudo (cada repetição fica um pouco mais escura, como num eco de verdade)
            // e por uma saturação suave, que segura o volume sem estalar.
            FxKind::Echo | FxKind::PingPong => {
                let max = self.delay.len() as f32 - 4.0;
                let target = (self.beats * ctx.beat_len).round().clamp(16.0, max);
                if self.delay_time <= 0.0 {
                    self.delay_time = target;
                } else if self.xfade <= 0.0 && (target - self.delay_time).abs() >= 1.0 {
                    self.delay_old = self.delay_time;
                    self.delay_time = target;
                    self.xfade = ECHO_XFADE;
                }
                let mut r = self.delay.read(self.delay_time);
                if self.xfade > 0.0 {
                    let t = self.xfade / ECHO_XFADE;
                    let o = self.delay.read(self.delay_old);
                    r = [r[0] * (1.0 - t) + o[0] * t, r[1] * (1.0 - t) + o[1] * t];
                    self.xfade -= 1.0;
                }
                if update {
                    self.fb_hp.set(Coefs::highpass(fs, 140.0, 0.7071));
                    self.wet_hp.set(Coefs::highpass(fs, 110.0, 0.7071));
                }
                let fbk = 0.35 + 0.35 * lvl;
                for c in 0..2 {
                    self.fb_lp[c] += (r[c] - self.fb_lp[c]) * 0.6;
                }
                let fb = self.fb_hp.process(self.fb_lp);
                let w = if self.kind == FxKind::Echo {
                    [soft(x[0] * inp + fb[0] * fbk), soft(x[1] * inp + fb[1] * fbk)]
                } else {
                    // Ping pong: a entrada vai para a esquerda e cada repetição troca de lado.
                    [soft((x[0] + x[1]) * 0.5 * inp + fb[1] * fbk), soft(fb[0] * fbk)]
                };
                self.delay.write(w);
                let g = 0.6 * lvl;
                let wet = self.wet_hp.process([r[0] * g, r[1] * g]);
                self.track_tail(wet);
                [x[0] + wet[0], x[1] + wet[1]]
            }
            // Reverb de placa (Dattorro/CAPS, como no Mixxx). A força aumenta o volume e o tamanho
            // da cauda; os graves ficam fora do reverb para não embolar o grave da música.
            FxKind::Reverb => {
                let send = lvl.powf(1.53) * inp;
                let decay = 0.89 * (0.5 + 0.35 * lvl);
                let out = self.plate.process((x[0] + x[1]) * 0.5 * send, decay);
                if update {
                    self.wet_hp.set(Coefs::highpass(fs, 160.0, 0.7071));
                }
                let wet = self.wet_hp.process([out[0] * 0.55, out[1] * 0.55]);
                self.track_tail(wet);
                [x[0] + wet[0], x[1] + wet[1]]
            }
            // Flanger (base: flanger do Mixxx). Um atraso de 0,3 a 8 ms varrendo no ritmo da
            // divisão de batida; a força é a mistura e a realimentação (o "jato"). O volume é
            // compensado para não subir com o efeito.
            FxKind::Flanger => {
                let l = self.lfo(ctx);
                let d = (0.0003 + 0.0077 * l) * fs;
                let r = self.mod_line.read(d);
                let regen = 0.25 + 0.45 * lvl;
                self.mod_line.write([soft(x[0] + r[0] * regen), soft(x[1] + r[1] * regen)]);
                let m = lvl * inp;
                let norm = 1.0 / (1.0 - m + 1.41253754 * m);
                [(x[0] + r[0] * m) * norm, (x[1] + r[1] * m) * norm]
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

/// Demonstração para ouvir os efeitos fora do aparelho (`dj-os --demo-efeitos <música> <pasta>`):
/// para cada efeito grava um WAV de 30 s do meio da música, 8 s sem efeito, 12 s com o efeito
/// (força 70%, divisão padrão) e 10 s desligado (a cauda). Mostra o pico de cada um: acima de
/// 0 dB a saída estouraria (o limitador do master teria que apertar).
pub fn demo(track: &crate::track::Track, dir: &std::path::Path) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir)?;
    let fs = track.sr as f32;
    let secs = |s: f32| (s * fs) as usize;
    let start = (track.frames / 3).min(track.frames.saturating_sub(secs(30.0)));
    let n = secs(30.0).min(track.frames - start);
    for kind in [None, Some(FxKind::Echo), Some(FxKind::PingPong), Some(FxKind::Reverb), Some(FxKind::Flanger)] {
        let mut fx = FxSlot::new(kind.unwrap_or_default(), fs);
        fx.set_level(0.7);
        let (mut peak, mut over) = (0.0f32, 0usize);
        let mut pcm = Vec::with_capacity(n * 4);
        for i in 0..n {
            if kind.is_some() {
                if i == secs(8.0) {
                    fx.set_active(true);
                } else if i == secs(20.0) {
                    fx.set_active(false);
                }
            }
            let f = start + i;
            let x = [track.data[f * 2], track.data[f * 2 + 1]];
            let ctx = FxCtx { beat_len: (track.beat_len() as f32).max(1.0), beat_pos: track.beats_at(f as f64) };
            let y = if kind.is_some() { fx.process(x, &ctx) } else { x };
            for v in y {
                peak = peak.max(v.abs());
                over += (v.abs() > 1.0) as usize;
                pcm.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
            }
        }
        let name = kind.map_or("0-sem-efeito".to_string(), |k| k.name().to_lowercase().replace(' ', "-"));
        let mut f = std::fs::File::create(dir.join(format!("{name}.wav")))?;
        f.write_all(&crate::record::wav_header(fs as u32, pcm.len() as u32))?;
        f.write_all(&pcm)?;
        println!("{name:14} pico {:+5.1} dB, {over} amostras estourando", 20.0 * peak.max(1e-9).log10());
    }
    Ok(())
}
