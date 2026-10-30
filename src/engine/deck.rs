//! Um deck: reprodução, pitch, scratch, cue, hotcues, loops e o canal do mixer.

use super::dsp::*;
use super::fx::{FxCtx, FxKind, FxSlot};
use crate::shared::{DeckSnap, FxSnap, FX_SLOTS, HOTCUES};
use crate::track::Track;
use std::sync::Arc;

/// Pulsos do jog por volta e duração de uma volta (vinil a 33⅓ rpm).
pub const TICKS_PER_REV: f64 = 720.0;
pub const SECS_PER_REV: f64 = 1.8;
/// Prato solto sem se mexer por este tempo = parou (fim do backspin).
const SPIN_STOP_SECS: f64 = 0.06;
/// Retomada da velocidade normal depois do scratch (constante de tempo, ~0,3 s até chegar).
const RESUME_SECS: f64 = 0.08;
/// Com a mão no prato, o som segue o prato por um filtro de 2ª ordem (amortecimento crítico) de
/// ~20 Hz: a velocidade fica lisa, sem o chiado das mensagens do jog (centenas por segundo), e o
/// som chega onde a mão parou. Mão parada = som parado e mudo, como no vinil e nas CDJ.
const FOLLOW_W: f64 = 2.0 * std::f64::consts::PI * 20.0;
/// Abaixo desta velocidade o som some aos poucos (prato quase parado não faz barulho nem deixa
/// tensão contínua na saída).
const QUIET_SPEED: f64 = 0.04;

pub const PITCH_RANGES: [f64; 3] = [0.08, 0.16, 0.50];

pub struct Roll {
    start: f64,
    len: f64,
    virt: f64,
    pub beats: f64,
}

pub struct Deck {
    fs: f64,
    pub track: Option<Arc<Track>>,
    pub pos: f64,
    pub playing: bool,
    pub rate: f64,
    pub pitch_range: f64,
    /// Depois de sync/reset o fader de pitch só volta a valer quando passar pelo valor atual.
    takeover: bool,
    pub key_semis: f64,
    pub cue: f64,
    pub cue_preview: bool,
    pub hotcues: [Option<f64>; HOTCUES],
    hc_preview: Option<usize>,
    pub loop_on: bool,
    pub loop_in: Option<f64>,
    pub loop_out: Option<f64>,
    pub loop_beats: f64,
    pub roll: Option<Roll>,
    trans: Option<f64>,
    trans_gate: f32,
    pub vinyl: bool,
    /// Keylock: o pitch muda o andamento sem mudar o tom.
    pub keylock: bool,
    kl: super::keylock::Keylock,
    kl_on: bool,
    touched: bool,
    scratching: bool,
    scr_rate: f64,
    scr_ticks: f64,
    /// Distância (em segundos de música) entre onde o som está e onde a mão deixou o prato.
    scr_err: f64,
    /// Passo e volume do bloco anterior (o próximo bloco vai de um ao outro, sem degrau).
    last_step: f64,
    last_gain: f32,
    bend: f64,
    bend_ticks: f64,
    // canal do mixer
    trim: Smooth,
    /// Ganho automático (nivela o volume das músicas pelo LUFS medido na análise).
    auto_gain_on: bool,
    auto_gain: Smooth,
    eq: [Smooth; 3],
    eq_v: [f32; 3],
    /// Isolador (corte total no mínimo) ou EQ (-26 dB no mínimo).
    pub eq_iso: bool,
    /// Curva do fader de canal: 0 lenta, 1 normal, 2 rápida.
    pub fader_curve: u8,
    pub filter_v: f32,
    filt_amt: Smooth,
    filt_lp: StereoBiquad,
    filt_hp: StereoBiquad,
    fader: Smooth,
    pub fader_v: f32,
    pub pfl: bool,
    lo: [StereoBiquad; 2],
    hi: [StereoBiquad; 2],
    pub fx: [FxSlot; FX_SLOTS],
    /// Pico do canal antes do fader (o medidor do canal na controladora, como nas Pioneer).
    pub peak: f32,
    /// Posição em batidas da entrada ao vivo (para os efeitos com LFO).
    live_beat: f64,
    /// Prato solto ainda girando (inércia, ex.: backspin): tempo desde o último movimento dele.
    spin_idle: f64,
}

pub fn trim_gain(v: f32) -> f32 {
    if v < 0.5 {
        (v / 0.5).powi(2)
    } else {
        1.0 + (v - 0.5) / 0.5 * (db(9.0) - 1.0)
    }
}

/// EQ com 0 dB no centro e +6 dB no máximo. No mínimo: corte total (isolador) ou -26 dB (EQ).
pub fn eq_gain(v: f32, iso: bool) -> f32 {
    if v < 0.5 {
        let floor = if iso { 0.0 } else { db(-26.0) };
        floor + (1.0 - floor) * (v / 0.5).powi(2)
    } else {
        1.0 + (v - 0.5) / 0.5 * (db(6.0) - 1.0)
    }
}

/// Curva do fader de canal. 0 lenta: o som só aparece de verdade no fim do curso; 1 normal;
/// 2 rápida: quase cheio já na metade (corte, bom para scratch).
pub fn fader_gain(v: f32, curve: u8) -> f32 {
    let v = v.clamp(0.0, 1.0);
    match curve {
        0 => v * v * v,
        2 => 1.0 - (1.0 - v) * (1.0 - v),
        _ => v * v,
    }
}

impl Deck {
    pub fn new(fs: f64) -> Self {
        let f = fs as f32;
        let defaults = [FxKind::Echo];
        Self {
            fs,
            live_beat: 0.0,
            spin_idle: 0.0,
            track: None,
            pos: 0.0,
            playing: false,
            rate: 1.0,
            pitch_range: PITCH_RANGES[0],
            takeover: false,
            key_semis: 0.0,
            cue: 0.0,
            cue_preview: false,
            hotcues: [None; HOTCUES],
            hc_preview: None,
            loop_on: false,
            loop_in: None,
            loop_out: None,
            loop_beats: 4.0,
            roll: None,
            trans: None,
            trans_gate: 1.0,
            vinyl: true,
            keylock: false,
            kl: super::keylock::Keylock::new(),
            kl_on: false,
            touched: false,
            scratching: false,
            scr_rate: 0.0,
            scr_ticks: 0.0,
            scr_err: 0.0,
            last_step: 0.0,
            last_gain: 0.0,
            bend: 0.0,
            bend_ticks: 0.0,
            trim: Smooth::new(1.0, 0.002),
            auto_gain_on: true,
            auto_gain: Smooth::new(1.0, 0.002),
            eq: [Smooth::new(1.0, 0.002); 3],
            eq_v: [0.5; 3],
            eq_iso: true,
            fader_curve: 1,
            filter_v: 0.5,
            filt_amt: Smooth::new(0.0, 0.003),
            filt_lp: StereoBiquad::default(),
            filt_hp: StereoBiquad::default(),
            fader: Smooth::new(1.0, 0.002),
            fader_v: 1.0,
            pfl: false,
            lo: [StereoBiquad::new(Coefs::lowpass(f, 250.0, 0.7071)); 2],
            hi: [StereoBiquad::new(Coefs::highpass(f, 2500.0, 0.7071)); 2],
            fx: defaults.map(|k| FxSlot::new(k, f)),
            peak: 0.0,
        }
    }

    /// Troca a música e devolve a anterior (o motor descarta fora da thread de áudio).
    pub fn load(&mut self, t: Arc<Track>) -> Option<Arc<Track>> {
        self.pos = 0.0;
        self.playing = false;
        self.cue = if t.bpm() > 0.0 { t.first_beat().max(0.0) } else { 0.0 };
        self.pos = self.cue;
        self.cue_preview = false;
        self.hotcues = [None; HOTCUES];
        self.hc_preview = None;
        self.loop_on = false;
        self.loop_in = None;
        self.loop_out = None;
        self.roll = None;
        self.trans = None;
        self.key_semis = 0.0;
        self.scratching = false;
        self.kl.reset();
        let old = self.track.replace(t);
        self.update_auto_gain();
        old
    }

    /// Correção do ganho automático da música carregada, em dB (0 se desligado ou sem medida).
    pub fn auto_gain_db(&self) -> f32 {
        match &self.track {
            Some(t) if self.auto_gain_on => crate::loudness::gain_db(t.loudness),
            _ => 0.0,
        }
    }

    fn update_auto_gain(&mut self) {
        self.auto_gain.set(10f32.powf(self.auto_gain_db() / 20.0));
    }

    pub fn set_auto_gain(&mut self, on: bool) {
        self.auto_gain_on = on;
        self.update_auto_gain();
    }

    /// Cue, hotcues e loop salvos da música. O deck vai para o cue salvo; o loop fica guardado
    /// (desligado) para o RELOOP.
    pub fn restore(&mut self, cue: Option<f64>, hotcues: [Option<f64>; HOTCUES], loop_: Option<(f64, f64, f64)>) {
        if let Some(c) = cue {
            self.cue = c.max(0.0);
            if !self.playing {
                self.pos = self.cue;
            }
        }
        self.hotcues = hotcues;
        if let Some((a, b, beats)) = loop_ {
            if b > a {
                self.loop_in = Some(a);
                self.loop_out = Some(b);
                self.loop_beats = beats;
            }
        }
    }

    fn sr(&self) -> f64 {
        self.track.as_ref().map_or(self.fs, |t| t.sr)
    }

    pub fn beat_len(&self) -> f64 {
        self.track.as_ref().map_or(self.fs * 0.5, |t| t.beat_len())
    }

    pub fn beats_at(&self, pos: f64) -> f64 {
        self.track.as_ref().map_or(0.0, |t| t.beats_at(pos))
    }

    pub fn has_bpm(&self) -> bool {
        self.track.as_ref().is_some_and(|t| t.bpm() > 0.0)
    }

    fn key_mult(&self) -> f64 {
        2f64.powf(self.key_semis / 12.0)
    }

    /// Velocidade nominal (sem scratch nem bend).
    fn nominal(&self) -> f64 {
        if self.playing {
            self.rate * self.key_mult()
        } else {
            0.0
        }
    }

    pub fn bpm_eff(&self) -> f64 {
        self.track.as_ref().map_or(0.0, |t| t.bpm() * self.rate * self.key_mult())
    }

    pub fn at_cue(&self) -> bool {
        (self.pos - self.cue).abs() < self.sr() * 0.01
    }

    /// Início da grade (em frames) com quantização para `unit` batidas.
    fn quantize_floor(&self, unit: f64) -> f64 {
        match &self.track {
            Some(t) if t.bpm() > 0.0 => {
                let b = t.beats_at(self.pos);
                t.pos_of_beat((b / unit).floor() * unit)
            }
            _ => self.pos,
        }
    }

    // ----- transporte -----

    pub fn play_toggle(&mut self) {
        if self.track.is_none() {
            return;
        }
        if self.cue_preview || self.hc_preview.is_some() {
            // Segurando CUE/hotcue e apertando PLAY: continua tocando.
            self.cue_preview = false;
            self.hc_preview = None;
            self.playing = true;
            return;
        }
        self.playing = !self.playing;
    }

    pub fn play(&mut self) {
        if self.track.is_some() {
            self.playing = true;
            self.cue_preview = false;
        }
    }

    /// CUE no estilo Pioneer.
    pub fn cue_button(&mut self, down: bool) {
        if self.track.is_none() {
            return;
        }
        if down {
            if self.playing && !self.cue_preview {
                self.playing = false;
                self.pos = self.cue;
            } else if self.at_cue() {
                self.cue_preview = true;
                self.playing = true;
            } else {
                self.cue = self.pos;
            }
        } else if self.cue_preview {
            self.cue_preview = false;
            self.playing = false;
            self.pos = self.cue;
        }
    }

    pub fn goto_cue(&mut self) {
        self.pos = self.cue;
    }

    pub fn cue_stop(&mut self) {
        self.playing = false;
        self.cue_preview = false;
        self.pos = self.cue;
    }

    pub fn stutter(&mut self) {
        if self.track.is_some() {
            self.pos = self.cue;
            self.playing = true;
        }
    }

    pub fn set_tempo_fader(&mut self, v: f64) {
        let target = 1.0 + self.pitch_range * (2.0 * v - 1.0);
        if self.takeover {
            if (target - self.rate).abs() < 0.003 {
                self.takeover = false;
            } else {
                return;
            }
        }
        self.rate = target;
    }

    pub fn cycle_pitch_range(&mut self) {
        let i = PITCH_RANGES.iter().position(|r| (r - self.pitch_range).abs() < 1e-6).unwrap_or(0);
        self.pitch_range = PITCH_RANGES[(i + 1) % PITCH_RANGES.len()];
        self.takeover = true;
    }

    pub fn set_pitch_range(&mut self, range: f64) {
        if PITCH_RANGES.iter().any(|r| (r - range).abs() < 1e-9) {
            self.pitch_range = range;
            self.takeover = true;
        }
    }

    pub fn pitch_reset(&mut self) {
        self.rate = 1.0;
        self.takeover = true;
    }

    pub fn set_synced_rate(&mut self, r: f64) {
        self.rate = r;
        self.takeover = true;
    }

    // ----- jog -----

    pub fn jog_touch(&mut self, down: bool) {
        self.touched = down;
        self.spin_idle = 0.0;
        if down && self.vinyl && self.track.is_some() {
            if !self.scratching {
                self.scr_rate = self.nominal();
            }
            self.scratching = true;
            self.scr_ticks = 0.0;
            // A mão segura o prato: o som freia até parar, sem passar do ponto e voltar.
            self.scr_err = -self.scr_rate / FOLLOW_W;
        }
    }

    pub fn jog_scratch(&mut self, ticks: f64) {
        if self.scratching {
            // Segurando o prato, ou o prato solto ainda girando depois do scratch: a música segue.
            self.scr_ticks += ticks;
        } else {
            self.jog_bend(ticks);
        }
    }

    pub fn jog_bend(&mut self, ticks: f64) {
        if self.scratching && !self.touched {
            // Prato solto girando por inércia (a REV1 manda como "lateral"): continua o scratch.
            self.scr_ticks += ticks;
            return;
        }
        if self.playing {
            self.bend_ticks += ticks;
        } else if self.track.is_some() {
            // Parado: o jog anda pela música devagar.
            self.pos = (self.pos + ticks * self.sr() * 0.004).max(0.0);
        }
    }

    pub fn jog_search(&mut self, ticks: f64) {
        if self.track.is_some() {
            self.pos = (self.pos + ticks * self.sr() * 0.03).max(0.0);
        }
    }

    // ----- hotcues e loops -----

    pub fn hotcue(&mut self, i: usize, down: bool) {
        if self.track.is_none() || i >= HOTCUES {
            return;
        }
        if down {
            match self.hotcues[i] {
                Some(p) => {
                    self.pos = p;
                    if !self.playing {
                        self.playing = true;
                        self.hc_preview = Some(i);
                    }
                }
                None => self.hotcues[i] = Some(self.pos),
            }
        } else if self.hc_preview == Some(i) {
            self.hc_preview = None;
            self.playing = false;
            if let Some(p) = self.hotcues[i] {
                self.pos = p;
            }
        }
    }

    fn set_loop(&mut self, beats: f64) {
        let bl = self.beat_len();
        let start = match (self.loop_on, self.loop_in) {
            (true, Some(a)) => a,
            _ => self.quantize_floor(beats.min(1.0)),
        };
        self.loop_beats = beats;
        self.loop_in = Some(start);
        self.loop_out = Some(start + beats * bl);
        self.loop_on = true;
    }

    pub fn loop_auto(&mut self, beats: f64) {
        if self.track.is_none() {
            return;
        }
        if self.loop_on && (self.loop_beats - beats).abs() < 1e-9 {
            self.loop_on = false;
        } else {
            self.set_loop(beats);
        }
    }

    pub fn loop_toggle(&mut self) {
        if self.loop_on {
            self.loop_on = false;
        } else if self.track.is_some() {
            self.set_loop(self.loop_beats);
        }
    }

    pub fn loop_resize(&mut self, factor: f64) {
        self.loop_beats = (self.loop_beats * factor).clamp(1.0 / 32.0, 64.0);
        if self.loop_on {
            if let Some(a) = self.loop_in {
                self.loop_out = Some(a + self.loop_beats * self.beat_len());
            }
        }
    }

    pub fn loop_in_point(&mut self) {
        self.loop_in = Some(self.pos);
        self.loop_on = false;
    }

    pub fn loop_out_point(&mut self) {
        if let Some(a) = self.loop_in {
            if self.pos > a {
                self.loop_out = Some(self.pos);
                self.loop_beats = (self.pos - a) / self.beat_len();
                self.loop_on = true;
            }
        }
    }

    pub fn reloop_exit(&mut self) {
        if self.loop_on {
            self.loop_on = false;
        } else if let (Some(a), Some(_)) = (self.loop_in, self.loop_out) {
            self.loop_on = true;
            self.pos = a;
        }
    }

    pub fn beat_jump(&mut self, beats: f64) {
        if self.track.is_some() {
            let d = beats * self.beat_len();
            self.pos = (self.pos + d).max(0.0);
            if self.loop_on {
                if let (Some(a), Some(b)) = (self.loop_in, self.loop_out) {
                    self.loop_in = Some(a + d);
                    self.loop_out = Some(b + d);
                }
            }
        }
    }

    pub fn roll(&mut self, beats: f64, down: bool) {
        if self.track.is_none() {
            return;
        }
        if down {
            let virt = self.roll.as_ref().map_or(self.pos, |r| r.virt);
            let start = self.quantize_floor(beats.min(1.0));
            self.roll = Some(Roll { start, len: beats * self.beat_len(), virt, beats });
        } else if let Some(r) = self.roll.take() {
            if (r.beats - beats).abs() < 1e-9 {
                self.pos = r.virt;
            } else {
                self.roll = Some(r);
            }
        }
    }

    pub fn trans(&mut self, beats: f64, down: bool) {
        if down {
            self.trans = Some(beats);
        } else if self.trans.is_some_and(|b| (b - beats).abs() < 1e-9) {
            self.trans = None;
        }
    }

    pub fn tracking_pad(&mut self, semis: f64) {
        self.key_semis = semis;
        if self.track.is_some() {
            self.pos = self.cue;
        }
    }

    // ----- mixer -----

    pub fn set_trim(&mut self, v: f32) {
        self.trim.set(trim_gain(v));
    }

    pub fn set_eq(&mut self, band: usize, v: f32) {
        if band < 3 {
            self.eq_v[band] = v;
            self.eq[band].set(eq_gain(v, self.eq_iso));
        }
    }

    pub fn set_eq_iso(&mut self, on: bool) {
        self.eq_iso = on;
        for b in 0..3 {
            self.set_eq(b, self.eq_v[b]);
        }
    }

    pub fn set_fader(&mut self, v: f32) {
        self.fader_v = v;
        self.fader.set(fader_gain(v, self.fader_curve));
    }

    pub fn set_fader_curve(&mut self, c: u8) {
        self.fader_curve = c;
        self.set_fader(self.fader_v);
    }

    fn update_filter(&mut self) {
        let fs = self.fs as f32;
        let v = self.filter_v;
        let d = (v - 0.5).abs();
        self.filt_amt.set(((d - 0.01) / 0.04).clamp(0.0, 1.0));
        if v < 0.5 {
            let fc = 20000.0 * 0.006f32.powf(1.0 - v / 0.5);
            self.filt_lp.set(Coefs::lowpass(fs, fc, 1.2));
        } else {
            let fc = 20.0 * 600f32.powf((v - 0.5) / 0.5);
            self.filt_hp.set(Coefs::highpass(fs, fc, 1.2));
        }
    }

    /// Gera `pre.len()` frames: `pre` antes do fader (vai para o fone), `post` depois.
    pub fn render(&mut self, pre: &mut [[f32; 2]], post: &mut [[f32; 2]]) {
        let n = pre.len();
        self.update_filter();
        let Some(t) = self.track.clone() else {
            pre.iter_mut().for_each(|s| *s = [0.0; 2]);
            post.iter_mut().for_each(|s| *s = [0.0; 2]);
            self.peak *= 0.9;
            return;
        };
        let ratio = t.sr / self.fs;
        let block_secs = n as f64 / self.fs;

        // Velocidade do bloco.
        let mut speed = if self.scratching {
            let (target, k) = if self.touched {
                // Segue o prato: a mão andou `moved`; acelera em direção a ela sem tranco.
                let moved = self.scr_ticks / TICKS_PER_REV * SECS_PER_REV;
                self.scr_err -= moved;
                let acc = -FOLLOW_W * FOLLOW_W * self.scr_err - 2.0 * FOLLOW_W * self.scr_rate;
                let v = self.scr_rate + acc * block_secs;
                self.scr_err += v * block_secs;
                (v, 1.0)
            } else if self.scr_ticks != 0.0 {
                // Prato solto ainda girando: segue o prato (velocidade desde o último movimento).
                let v = (self.scr_ticks / TICKS_PER_REV * SECS_PER_REV) / (self.spin_idle + block_secs);
                self.spin_idle = 0.0;
                (v, 0.3)
            } else {
                self.spin_idle += block_secs;
                if self.spin_idle < SPIN_STOP_SECS {
                    // Entre um movimento e outro do prato: mantém a velocidade.
                    (self.scr_rate, 0.0)
                } else {
                    // O prato parou: a música volta a tocar normal numa retomada curta.
                    (self.nominal(), 1.0 - (-block_secs / RESUME_SECS).exp())
                }
            };
            self.scr_ticks = 0.0;
            self.scr_rate += (target - self.scr_rate) * k;
            if !self.touched && self.spin_idle >= SPIN_STOP_SECS && (self.scr_rate - self.nominal()).abs() < 0.01 {
                self.scratching = false;
            }
            self.scr_rate
        } else {
            self.nominal()
        };
        self.bend = (self.bend + self.bend_ticks * 0.004).clamp(-0.6, 0.6) * 0.9;
        self.bend_ticks = 0.0;
        if self.playing && !self.scratching {
            speed += self.bend * self.rate;
        }
        let step = speed * ratio;
        let step0 = if self.scratching { self.last_step } else { step };
        self.last_step = step;
        let gain1 = (speed.abs() / QUIET_SPEED).min(1.0) as f32;
        let gain0 = self.last_gain;
        self.last_gain = gain1;
        // Keylock só quando faz diferença: tocando sem scratch, pitch fora do zero e sem o tom
        // dos pads de Tracking (que muda o tom de propósito).
        let use_kl = self.keylock && self.playing && !self.scratching && self.key_semis == 0.0 && (speed - 1.0).abs() > 0.0005;
        if use_kl && !self.kl_on {
            self.kl.reset();
        }
        self.kl_on = use_kl;

        let nominal_out = (self.rate * self.key_mult() * ratio).max(0.05);
        let fx_on = self.fx.iter().any(|f| f.sounding());
        let mut ctx = FxCtx { beat_len: (self.beat_len() / nominal_out) as f32, beat_pos: 0.0 };
        let end = t.frames as f64;
        let mut peak = 0.0f32;

        for i in 0..n {
            let frac = (i + 1) as f64 / n as f64;
            let step = step0 + (step - step0) * frac;
            // Dentro do loop também no scratch e no backspin: para trás, sai no fim do loop.
            if self.loop_on {
                if let (Some(a), Some(b)) = (self.loop_in, self.loop_out) {
                    if b > a && step > 0.0 && self.pos >= b {
                        self.pos = a + (self.pos - b) % (b - a);
                    } else if b > a && step < 0.0 && self.pos < a {
                        self.pos = b - (a - self.pos) % (b - a);
                    }
                }
            }
            let (l, r) = if use_kl {
                self.kl.next(&t, self.pos, ratio)
            } else if step.abs() > 1.5 {
                // Scratch rápido: média das amostras do caminho (tira o chiado de "digital").
                let k = (step.abs().ceil() as usize).min(8);
                let (mut l, mut r) = (0.0f32, 0.0f32);
                for j in 0..k {
                    let (a, b) = t.sample(self.pos + step * j as f64 / k as f64);
                    l += a;
                    r += b;
                }
                (l / k as f32, r / k as f32)
            } else {
                t.sample(self.pos)
            };
            let quiet = gain0 + (gain1 - gain0) * frac as f32;
            let (l, r) = (l * quiet, r * quiet);
            self.pos += step;
            if self.pos < 0.0 {
                self.pos = 0.0;
            }
            if let Some(roll) = &mut self.roll {
                roll.virt += step;
                if self.pos >= roll.start + roll.len {
                    self.pos -= roll.len;
                }
            }
            if self.pos >= end && step > 0.0 {
                self.pos = end;
                self.playing = false;
                self.scratching = false;
            }

            let open = match self.trans {
                Some(b) => {
                    if (t.beats_at(self.pos) / b).rem_euclid(1.0) < 0.5 {
                        1.0
                    } else {
                        0.0
                    }
                }
                None => 1.0,
            };
            self.trans_gate += (open - self.trans_gate) * 0.02;

            let g = self.trim.next() * self.auto_gain.next() * self.trans_gate;
            let mut x = [l * g, r * g];
            if fx_on {
                ctx.beat_pos = t.beats_at(self.pos);
                for f in self.fx.iter_mut() {
                    x = f.process(x, &ctx);
                }
            }

            (pre[i], post[i]) = self.channel(x);
            peak = peak.max(pre[i][0].abs()).max(pre[i][1].abs());
        }
        self.peak = peak.max(self.peak * 0.85);
    }

    /// EQ, filtro e fader do canal: devolve (antes do fader, para o fone; depois do fader).
    #[inline]
    fn channel(&mut self, x: [f32; 2]) -> ([f32; 2], [f32; 2]) {
        // EQ de 3 bandas por divisão de frequências: soma exata quando tudo em 0 dB.
        let lo0 = self.lo[0].process(x);
        let lo = self.lo[1].process(lo0);
        let hi0 = self.hi[0].process(x);
        let hi = self.hi[1].process(hi0);
        let (gl, gm, gh) = (self.eq[0].next(), self.eq[1].next(), self.eq[2].next());
        let mut y = [0.0f32; 2];
        for c in 0..2 {
            let mid = x[c] - lo[c] - hi[c];
            y[c] = lo[c] * gl + mid * gm + hi[c] * gh;
        }

        let amt = self.filt_amt.next();
        if amt > 1e-4 {
            let f = if self.filter_v < 0.5 { self.filt_lp.process(y) } else { self.filt_hp.process(y) };
            y = [y[0] + (f[0] - y[0]) * amt, y[1] + (f[1] - y[1]) * amt];
        }

        let fg = self.fader.next();
        (y, [y[0] * fg, y[1] * fg])
    }

    /// Entrada ao vivo (celular por Bluetooth) no lugar da música: passa por trim, efeito, EQ,
    /// filtro e fader. `beat_len` (amostras por batida) vem da música que está tocando.
    pub fn render_live(&mut self, input: &[[f32; 2]], pre: &mut [[f32; 2]], post: &mut [[f32; 2]], beat_len: f32) {
        self.update_filter();
        let fx_on = self.fx.iter().any(|f| f.sounding());
        let mut ctx = FxCtx { beat_len, beat_pos: self.live_beat };
        let mut peak = 0.0f32;
        for i in 0..input.len() {
            let g = self.trim.next();
            let mut x = [input[i][0] * g, input[i][1] * g];
            if fx_on {
                for f in self.fx.iter_mut() {
                    x = f.process(x, &ctx);
                }
            }
            ctx.beat_pos += 1.0 / beat_len as f64;
            (pre[i], post[i]) = self.channel(x);
            peak = peak.max(pre[i][0].abs()).max(pre[i][1].abs());
        }
        self.live_beat = ctx.beat_pos % 64.0;
        self.peak = peak.max(self.peak * 0.85);
    }

    pub fn snapshot(&self) -> DeckSnap {
        let (loaded, gen, frames, sr, bpm_track) = match &self.track {
            Some(t) => (true, t.gen, t.frames as f64, t.sr, t.bpm()),
            None => (false, 0, 0.0, self.fs, 0.0),
        };
        DeckSnap {
            loaded,
            track_gen: gen,
            pos: self.pos,
            frames,
            sr,
            playing: self.playing,
            cue_preview: self.cue_preview || self.hc_preview.is_some(),
            cue: self.cue,
            at_cue: self.at_cue(),
            hotcues: self.hotcues,
            loop_on: self.loop_on,
            loop_in: self.loop_in,
            loop_out: self.loop_out,
            loop_beats: self.loop_beats,
            roll_beats: self.roll.as_ref().map_or(0.0, |r| r.beats),
            bpm_track,
            bpm: self.bpm_eff(),
            rate: self.rate,
            pitch_range: self.pitch_range,
            key_semis: self.key_semis,
            sync_ok: false,
            vinyl: self.vinyl,
            keylock: self.keylock,
            pfl: self.pfl,
            fader: self.fader_v,
            peak: self.peak,
            auto_gain_db: self.auto_gain_db(),
            eq: [self.eq[0].cur, self.eq[1].cur, self.eq[2].cur],
            trim: self.trim.cur,
            live: false,
            fx: std::array::from_fn(|i| {
                let f = &self.fx[i];
                FxSnap { kind: f.kind, active: f.active, sounding: f.sounding(), level: f.level(), beats: f.beats }
            }),
        }
    }
}
