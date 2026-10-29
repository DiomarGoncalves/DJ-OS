//! Motor de áudio: roda dentro da thread de áudio, sem alocar memória nem travar.

pub mod deck;
pub mod dsp;
pub mod fx;
pub mod keylock;
#[cfg(test)]
mod tests;

use crate::shared::{Cmd, SharedSnap, Snapshot, NUM_DECKS, SAMPLER_SLOTS};
use crate::track::Track;
use deck::Deck;
use dsp::{soft_clip, Limiter, Smooth};
use std::sync::Arc;

const BLOCK: usize = 64;

struct Voice {
    track: Option<Arc<Track>>,
    pos: f64,
    playing: bool,
}

pub struct Engine {
    fs: f64,
    decks: Vec<Deck>,
    cmds: rtrb::Consumer<Cmd>,
    trash: rtrb::Producer<Arc<Track>>,
    snap: SharedSnap,
    xf: Smooth,
    master: Smooth,
    hp: Smooth,
    hp_mix: Smooth,
    lim_master: Limiter,
    lim_phones: Limiter,
    voices: Vec<Voice>,
    pre: [[[f32; 2]; BLOCK]; NUM_DECKS],
    post: [[[f32; 2]; BLOCK]; NUM_DECKS],
    master_peak: [f32; 2],
    bt: Option<rtrb::Producer<[f32; 2]>>,
    bt_source: u8,
    xf_curve: u8,
    line_in: Option<LineIn>,
    line_route: Option<usize>,
    ambient_vol: f32,
    ambient_duck: bool,
    ambient_gain: Smooth,
    master_delay: dsp::DelayLine,
    master_delay_samples: Smooth,
}

/// Curva do crossfader. 0: os dois lados cheios no meio; 1: corte rápido nas pontas (scratch);
/// 2: potência constante (mixagem longa).
fn xf_gains(x: f32, curve: u8) -> (f32, f32) {
    match curve {
        1 => {
            let edge = 0.04;
            (((1.0 - x) / edge).min(1.0), (x / edge).min(1.0))
        }
        2 => {
            let a = x * std::f32::consts::FRAC_PI_2;
            (a.cos(), a.sin())
        }
        _ => ((2.0 * (1.0 - x)).min(1.0), (2.0 * x).min(1.0)),
    }
}

/// Leitura do som do celular. O relógio do celular é outro: a fila fica perto de 200 ms e a
/// leitura anda um pouco mais rápido ou mais devagar (no máximo 0,5%) para não encher nem esvaziar.
struct LineIn {
    rx: rtrb::Consumer<[f32; 2]>,
    primed: bool,
    t: f64,
    prev: [f32; 2],
    cur: [f32; 2],
}

impl LineIn {
    const TARGET: f64 = 9600.0;

    fn new(rx: rtrb::Consumer<[f32; 2]>) -> Self {
        Self { rx, primed: false, t: 0.0, prev: [0.0; 2], cur: [0.0; 2] }
    }

    #[inline]
    fn next(&mut self) -> [f32; 2] {
        let fill = self.rx.slots() as f64;
        if !self.primed {
            if fill < Self::TARGET {
                return [0.0; 2];
            }
            self.primed = true;
        }
        self.t += 1.0 + ((fill - Self::TARGET) / Self::TARGET * 0.002).clamp(-0.005, 0.005);
        while self.t >= 1.0 {
            self.prev = self.cur;
            match self.rx.pop() {
                Ok(f) => self.cur = f,
                Err(_) => {
                    // Esvaziou (celular pausou): silêncio até encher de novo.
                    self.primed = false;
                    self.cur = [0.0; 2];
                }
            }
            self.t -= 1.0;
        }
        let k = self.t as f32;
        [self.prev[0] + (self.cur[0] - self.prev[0]) * k, self.prev[1] + (self.cur[1] - self.prev[1]) * k]
    }
}

impl Engine {
    pub fn new(fs: f64, cmds: rtrb::Consumer<Cmd>, trash: rtrb::Producer<Arc<Track>>, snap: SharedSnap) -> Self {
        Self {
            fs,
            decks: (0..NUM_DECKS).map(|_| Deck::new(fs)).collect(),
            cmds,
            trash,
            snap,
            xf: Smooth::new(0.5, 0.002),
            master: Smooth::new(0.98, 0.002),
            hp: Smooth::new(0.72, 0.002),
            hp_mix: Smooth::new(0.0, 0.002),
            lim_master: Limiter::new(fs),
            lim_phones: Limiter::new(fs),
            voices: (0..SAMPLER_SLOTS).map(|_| Voice { track: None, pos: 0.0, playing: false }).collect(),
            pre: [[[0.0; 2]; BLOCK]; NUM_DECKS],
            post: [[[0.0; 2]; BLOCK]; NUM_DECKS],
            master_peak: [0.0; 2],
            bt: None,
            bt_source: 1,
            xf_curve: 0,
            line_in: None,
            line_route: Some(3),
            ambient_vol: 0.7,
            ambient_duck: true,
            ambient_gain: Smooth::new(0.0, 0.00006),
            master_delay: dsp::DelayLine::new((fs * 0.6) as usize),
            master_delay_samples: Smooth::new(0.0, 0.0005),
        }
    }

    /// Devolve as filas para recriar o motor com outra configuração.
    pub fn into_parts(self) -> (rtrb::Consumer<Cmd>, rtrb::Producer<Arc<Track>>) {
        (self.cmds, self.trash)
    }

    /// Descarta uma música fora da thread de áudio (liberar memória aqui poderia travar o som).
    fn discard(&mut self, t: Arc<Track>) {
        if let Err(rtrb::PushError::Full(t)) = self.trash.push(t) {
            std::mem::forget(t);
        }
    }

    fn commands(&mut self) {
        while let Ok(c) = self.cmds.pop() {
            self.apply(c);
        }
    }

    fn apply(&mut self, c: Cmd) {
        let ok = |d: usize| d < NUM_DECKS;
        match c {
            Cmd::Load { deck, track } if ok(deck) => {
                if let Some(old) = self.decks[deck].load(track) {
                    self.discard(old);
                }
            }
            Cmd::CloneDeck { from, to } if ok(from) && ok(to) && from != to => {
                if let Some(t) = self.decks[from].track.clone() {
                    if let Some(old) = self.decks[to].load(t) {
                        self.discard(old);
                    }
                    let (pos, playing, rate, cue, hotcues) = {
                        let f = &self.decks[from];
                        (f.pos, f.playing, f.rate, f.cue, f.hotcues)
                    };
                    let d = &mut self.decks[to];
                    d.pos = pos;
                    d.playing = playing;
                    d.set_synced_rate(rate);
                    d.cue = cue;
                    d.hotcues = hotcues;
                }
            }
            Cmd::SetPos { deck, pos } if ok(deck) => self.decks[deck].pos = pos.max(0.0),
            Cmd::RestoreCues { deck, cue, hotcues, loop_ } if ok(deck) => self.decks[deck].restore(cue, hotcues, loop_),
            Cmd::PlayToggle(d) if ok(d) => self.decks[d].play_toggle(),
            Cmd::Play(d) if ok(d) => self.decks[d].play(),
            Cmd::Cue { deck, down } if ok(deck) => self.decks[deck].cue_button(down),
            Cmd::GotoCue(d) if ok(d) => self.decks[d].goto_cue(),
            Cmd::CueStop(d) if ok(d) => self.decks[d].cue_stop(),
            Cmd::GotoStart(d) if ok(d) => self.decks[d].pos = 0.0,
            Cmd::Stutter(d) if ok(d) => self.decks[d].stutter(),
            Cmd::Tempo { deck, v } if ok(deck) => self.decks[deck].set_tempo_fader(v),
            Cmd::PitchRangeCycle(d) if ok(d) => self.decks[d].cycle_pitch_range(),
            Cmd::SetPitchRange { deck, range } if ok(deck) => self.decks[deck].set_pitch_range(range),
            Cmd::XfCurve(c) => self.xf_curve = c,
            Cmd::PitchReset(d) if ok(d) => self.decks[d].pitch_reset(),
            Cmd::Sync(d) if ok(d) => self.sync(d),
            Cmd::Vinyl { deck, on } if ok(deck) => self.decks[deck].vinyl = on,
            Cmd::Keylock { deck, on } if ok(deck) => self.decks[deck].keylock = on,
            Cmd::JogTouch { deck, down } if ok(deck) => self.decks[deck].jog_touch(down),
            Cmd::JogScratch { deck, ticks } if ok(deck) => self.decks[deck].jog_scratch(ticks),
            Cmd::JogBend { deck, ticks } if ok(deck) => self.decks[deck].jog_bend(ticks),
            Cmd::JogSearch { deck, ticks } if ok(deck) => self.decks[deck].jog_search(ticks),
            Cmd::Hotcue { deck, idx, down } if ok(deck) => self.decks[deck].hotcue(idx, down),
            Cmd::HotcueDelete { deck, idx } if ok(deck) => {
                if let Some(h) = self.decks[deck].hotcues.get_mut(idx) {
                    *h = None;
                }
            }
            Cmd::LoopAuto { deck, beats } if ok(deck) => self.decks[deck].loop_auto(beats),
            Cmd::LoopToggle(d) if ok(d) => self.decks[d].loop_toggle(),
            Cmd::LoopHalve(d) if ok(d) => self.decks[d].loop_resize(0.5),
            Cmd::LoopDouble(d) if ok(d) => self.decks[d].loop_resize(2.0),
            Cmd::LoopIn(d) if ok(d) => self.decks[d].loop_in_point(),
            Cmd::LoopOut(d) if ok(d) => self.decks[d].loop_out_point(),
            Cmd::ReloopExit(d) if ok(d) => self.decks[d].reloop_exit(),
            Cmd::BeatJump { deck, beats } if ok(deck) => self.decks[deck].beat_jump(beats),
            Cmd::Roll { deck, beats, down } if ok(deck) => self.decks[deck].roll(beats, down),
            Cmd::Trans { deck, beats, down } if ok(deck) => self.decks[deck].trans(beats, down),
            Cmd::TrackingPad { deck, semis } if ok(deck) => self.decks[deck].tracking_pad(semis),
            Cmd::Trim { deck, v } if ok(deck) => self.decks[deck].set_trim(v),
            Cmd::AutoGain(on) => self.decks.iter_mut().for_each(|d| d.set_auto_gain(on)),
            Cmd::Eq { deck, band, v } if ok(deck) => self.decks[deck].set_eq(band, v),
            Cmd::Filter { deck, v } if ok(deck) => self.decks[deck].filter_v = v.clamp(0.0, 1.0),
            Cmd::Fader { deck, v } if ok(deck) => self.decks[deck].set_fader(v),
            Cmd::Pfl { deck, on } if ok(deck) => self.decks[deck].pfl = on,
            Cmd::Crossfader(v) => self.xf.set(v.clamp(0.0, 1.0)),
            Cmd::MasterLevel(v) => self.master.set(v * v * 2.0),
            Cmd::HeadphoneLevel(v) => self.hp.set(v * v * 2.0),
            Cmd::HeadphoneMix(v) => self.hp_mix.set(v.clamp(0.0, 1.0)),
            Cmd::FxKind { deck, slot, kind } if ok(deck) => {
                if let Some(f) = self.decks[deck].fx.get_mut(slot) {
                    f.set_kind(kind);
                }
            }
            Cmd::FxActive { deck, slot, on } if ok(deck) => {
                if let Some(f) = self.decks[deck].fx.get_mut(slot) {
                    f.set_active(on);
                }
            }
            Cmd::FxLevel { deck, slot, v } if ok(deck) => {
                if let Some(f) = self.decks[deck].fx.get_mut(slot) {
                    f.set_level(v);
                }
            }
            Cmd::FxBeats { deck, slot, beats } if ok(deck) => {
                if let Some(f) = self.decks[deck].fx.get_mut(slot) {
                    f.beats = beats;
                }
            }
            Cmd::SamplerPlay { slot, track } if slot < SAMPLER_SLOTS => {
                let v = &mut self.voices[slot];
                v.pos = 0.0;
                v.playing = true;
                if let Some(old) = v.track.replace(track) {
                    self.discard(old);
                }
            }
            Cmd::SamplerStop(s) if s < SAMPLER_SLOTS => self.voices[s].playing = false,
            Cmd::BtAttach(p) => self.bt = Some(p),
            Cmd::BtDetach => self.bt = None,
            Cmd::BtSource(s) => self.bt_source = s,
            Cmd::AmbientVolume(v) => self.ambient_vol = v.clamp(0.0, 1.0),
            Cmd::AmbientDuck(on) => self.ambient_duck = on,
            Cmd::LineInAttach(c) => self.line_in = Some(LineIn::new(c)),
            Cmd::LineInDetach => self.line_in = None,
            Cmd::LineInRoute(r) => self.line_route = r.filter(|&d| d < NUM_DECKS),
            Cmd::MasterDelay(ms) => self.master_delay_samples.set((ms.clamp(0.0, 500.0) / 1000.0) * self.fs as f32),
            other => {
                // Índice inválido: só descarta (sem alocação; a música vai para a lixeira).
                if let Cmd::Load { track, .. } | Cmd::SamplerPlay { track, .. } = other {
                    self.discard(track);
                }
            }
        }
    }

    /// Escolhe o deck de referência para o sync: outro deck tocando, o mais alto no fader.
    fn sync_master(&self, d: usize) -> Option<usize> {
        let usable = |i: usize| i != d && self.decks[i].has_bpm();
        let pick = |playing_only: bool| {
            (0..NUM_DECKS)
                .filter(|&i| usable(i) && (!playing_only || self.decks[i].playing))
                .max_by(|&a, &b| self.decks[a].fader_v.total_cmp(&self.decks[b].fader_v))
        };
        pick(true).or_else(|| pick(false))
    }

    fn sync(&mut self, d: usize) {
        if !self.decks[d].has_bpm() {
            return;
        }
        let Some(m) = self.sync_master(d) else { return };
        let mb = self.decks[m].bpm_eff();
        let tb = self.decks[d].track.as_ref().map_or(0.0, |t| t.bpm);
        if mb <= 0.0 || tb <= 0.0 {
            return;
        }
        // Aceita meia/dupla velocidade, escolhendo a razão mais perto de 1.
        let r = [mb / tb, mb * 2.0 / tb, mb / 2.0 / tb]
            .into_iter()
            .min_by(|a, b| (a - 1.0).abs().total_cmp(&(b - 1.0).abs()))
            .unwrap_or(1.0);
        let key = 2f64.powf(self.decks[d].key_semis / 12.0);
        self.decks[d].set_synced_rate(r / key);
        let pm = self.decks[m].beats_at(self.decks[m].pos).rem_euclid(1.0);
        let pd = self.decks[d].beats_at(self.decks[d].pos).rem_euclid(1.0);
        let mut diff = pm - pd;
        if diff > 0.5 {
            diff -= 1.0;
        } else if diff < -0.5 {
            diff += 1.0;
        }
        let bl = self.decks[d].beat_len();
        let deck = &mut self.decks[d];
        deck.pos = (deck.pos + diff * bl).max(0.0);
    }

    fn sync_ok(&self, d: usize) -> bool {
        let deck = &self.decks[d];
        if !deck.has_bpm() || !deck.playing {
            return false;
        }
        let Some(m) = self.sync_master(d) else { return false };
        let (a, b) = (deck.bpm_eff(), self.decks[m].bpm_eff());
        let bpm_ok = [b, b * 2.0, b / 2.0].iter().any(|x| (a - x).abs() < 0.05);
        let pm = self.decks[m].beats_at(self.decks[m].pos).rem_euclid(1.0);
        let pd = deck.beats_at(deck.pos).rem_euclid(1.0);
        let diff = (pm - pd).abs();
        bpm_ok && diff.min(1.0 - diff) < 0.04
    }

    pub fn process(&mut self, out: &mut [f32], channels: usize) {
        self.commands();
        let frames = out.len() / channels.max(1);
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(BLOCK);
            // Som do celular: lido uma vez por bloco; vai para um deck ou para o master.
            let mut live = [[0.0f32; 2]; BLOCK];
            let live_deck = match self.line_in.as_mut() {
                Some(li) => {
                    live[..n].iter_mut().for_each(|f| *f = li.next());
                    self.line_route
                }
                None => None,
            };
            let beat_len = self.live_beat_len();
            for d in 0..NUM_DECKS {
                let (pre, post) = (&mut self.pre[d][..n], &mut self.post[d][..n]);
                if Some(d) == live_deck {
                    self.decks[d].render_live(&live[..n], pre, post, beat_len);
                } else {
                    self.decks[d].render(pre, post);
                }
            }
            let live_master = self.line_in.is_some() && live_deck.is_none();
            // Som ambiente abaixa sozinho (em ~1 s) quando algum deck está tocando com o fader aberto.
            let dj_playing = self.decks.iter().any(|d| d.playing && d.fader_v > 0.05);
            let sp_target = if self.ambient_duck && dj_playing { 0.0 } else { self.ambient_vol };
            self.ambient_gain.set(sp_target);
            for i in 0..n {
                let (ga, gb) = xf_gains(self.xf.next(), self.xf_curve);
                let mut m = [0.0f32; 2];
                let mut c = [0.0f32; 2];
                for d in 0..NUM_DECKS {
                    let g = if d % 2 == 0 { ga } else { gb };
                    let p = self.post[d][i];
                    m[0] += p[0] * g;
                    m[1] += p[1] * g;
                    if self.decks[d].pfl {
                        let q = self.pre[d][i];
                        c[0] += q[0];
                        c[1] += q[1];
                    }
                }
                for v in self.voices.iter_mut() {
                    if !v.playing {
                        continue;
                    }
                    if let Some(t) = &v.track {
                        let (l, r) = t.sample(v.pos);
                        m[0] += l * 0.8;
                        m[1] += r * 0.8;
                        v.pos += t.sr / self.fs;
                        if v.pos >= t.frames as f64 {
                            v.playing = false;
                        }
                    }
                }
                let sg = self.ambient_gain.next();
                // Celular no master: som ambiente, com volume próprio e "abaixar quando um deck toca".
                if live_master {
                    m[0] += live[i][0] * sg;
                    m[1] += live[i][1] * sg;
                }
                let mg = self.master.next();
                // Limitador (picos abaixo de -1 dBFS) e, por segurança, o soft clip.
                let l = self.lim_master.process([m[0] * mg, m[1] * mg]);
                let mo = [soft_clip(l[0]), soft_clip(l[1])];
                let mix = self.hp_mix.next();
                let hg = self.hp.next();
                let hl = self.lim_phones.process([(c[0] * (1.0 - mix) + mo[0] * mix) * hg, (c[1] * (1.0 - mix) + mo[1] * mix) * hg]);
                let h = [soft_clip(hl[0]), soft_clip(hl[1])];
                // Bluetooth: master (sem o atraso) ou fone; se a fila encher, o quadro é descartado.
                if let Some(p) = self.bt.as_mut() {
                    let _ = p.push(if self.bt_source == 2 { h } else { mo });
                }
                // Atraso do master com fio para alinhar com a caixa Bluetooth.
                let dly = self.master_delay_samples.next();
                self.master_delay.write(mo);
                let mw = if dly >= 1.0 { self.master_delay.read(dly) } else { mo };
                let base = (done + i) * channels;
                let frame = &mut out[base..base + channels];
                frame.iter_mut().for_each(|s| *s = 0.0);
                frame[0] = mw[0];
                if channels > 1 {
                    frame[1] = mw[1];
                }
                if channels >= 4 {
                    frame[2] = h[0];
                    frame[3] = h[1];
                }
                self.master_peak[0] = self.master_peak[0].max(mo[0].abs());
                self.master_peak[1] = self.master_peak[1].max(mo[1].abs());
            }
            done += n;
        }
        self.publish();
    }

    /// Batida para os efeitos no deck do celular: a da música que está tocando (ou 120 BPM).
    fn live_beat_len(&self) -> f32 {
        let bpm = self.decks.iter().find(|d| d.playing && d.has_bpm()).map_or(120.0, |d| d.bpm_eff());
        (self.fs * 60.0 / bpm.clamp(40.0, 250.0)) as f32
    }

    fn publish(&mut self) {
        if let Some(mut s) = self.snap.try_lock() {
            let mut snap = Snapshot {
                decks: std::array::from_fn(|d| self.decks[d].snapshot()),
                master_peak: self.master_peak,
                master_limit: self.lim_master.min_gain,
                sampler_playing: std::array::from_fn(|i| self.voices[i].playing),
                sample_rate: self.fs,
            };
            for d in 0..NUM_DECKS {
                snap.decks[d].sync_ok = self.sync_ok(d);
                snap.decks[d].live = self.line_in.is_some() && self.line_route == Some(d);
            }
            *s = snap;
            self.master_peak = [0.0; 2];
            self.lim_master.min_gain = 1.0;
        }
    }
}
