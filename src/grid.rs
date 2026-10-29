//! Ajuste da grade de batidas (quando o BPM vem errado ou as batidas estão deslocadas).
//!
//! Abre tocando no BPM do deck. Muda a grade da música carregada na hora (o motor lê o BPM e a
//! primeira batida sem travar o áudio) e grava na memória da música, mantendo os cues.

use crate::track::Track;
use std::time::{Duration, Instant};

/// Editor aberto para um deck.
pub struct GridEdit {
    pub deck: usize,
    taps: Vec<Instant>,
}

impl GridEdit {
    pub fn new(deck: usize) -> Self {
        Self { deck, taps: Vec::new() }
    }

    /// TAP: bate no ritmo. Com 4 toques ou mais (até 2 s entre eles) devolve o BPM da música, já
    /// descontando o pitch do deck (`rate`).
    pub fn tap(&mut self, now: Instant, rate: f64) -> Option<f64> {
        if self.taps.last().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(2)) {
            self.taps.clear();
        }
        self.taps.push(now);
        if self.taps.len() > 8 {
            self.taps.remove(0);
        }
        if self.taps.len() < 4 {
            return None;
        }
        let span = self.taps.last()?.duration_since(self.taps[0]).as_secs_f64();
        let heard = 60.0 * (self.taps.len() - 1) as f64 / span;
        Some(heard / rate.max(0.01))
    }

    pub fn taps(&self) -> usize {
        self.taps.len()
    }
}

/// Muda o BPM (a primeira batida fica no lugar).
pub fn set_bpm(t: &Track, bpm: f64) {
    t.set_grid(bpm.clamp(40.0, 250.0), t.first_beat());
}

/// Desloca a grade em segundos (positivo = batidas mais para a frente).
pub fn shift_secs(t: &Track, secs: f64) {
    t.set_grid(t.bpm(), wrap(t, t.first_beat() + secs * t.sr));
}

/// Desloca a grade numa fração de batida (ex.: 0,5 = meia batida).
pub fn shift_beats(t: &Track, beats: f64) {
    t.set_grid(t.bpm(), wrap(t, t.first_beat() + beats * t.beat_len()));
}

/// Põe uma batida exatamente em `pos` (frames).
pub fn beat_here(t: &Track, pos: f64) {
    t.set_grid(t.bpm(), wrap(t, pos));
}

/// A primeira batida fica entre 0 e uma batida (é a mesma grade, só a referência muda).
fn wrap(t: &Track, fb: f64) -> f64 {
    fb.rem_euclid(t.beat_len())
}

/// Grava a grade na memória da música (mantém cues, hotcues e loop).
pub fn save(t: &Track) {
    if cfg!(test) {
        return;
    }
    let mut s = crate::store::load(&t.path).unwrap_or_else(|| crate::store::Saved::from_track(t));
    s.bpm = t.bpm();
    s.first_beat = t.first_beat();
    crate::store::save(&t.path, &s);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ajustes_da_grade() {
        let t = crate::track::synthetic(10.0, 48000.0, 64.0, 0.1);
        set_bpm(&t, t.bpm() * 2.0);
        assert_eq!(t.bpm(), 128.0, "×2");
        let fb = t.first_beat();
        shift_secs(&t, 0.010);
        assert!((t.first_beat() - fb - 480.0).abs() < 1e-6, "+10 ms = 480 frames a 48 kHz");
        shift_beats(&t, 0.5);
        assert!((t.first_beat() - fb - 480.0 - t.beat_len() / 2.0).abs() < 1e-6, "meia batida");
        beat_here(&t, 48000.0 * 5.0 + 123.0);
        assert!((t.beats_at(48000.0 * 5.0 + 123.0).fract()).abs() < 1e-9, "a batida cai exatamente ali");
        assert!(t.first_beat() >= 0.0 && t.first_beat() < t.beat_len());
    }

    #[test]
    fn tap_tempo_desconta_o_pitch() {
        let mut g = GridEdit::new(0);
        let t0 = Instant::now();
        let mut bpm = None;
        // Batendo a 126 BPM ouvidos com o deck a +5%: a música tem 120 BPM.
        for i in 0..6 {
            bpm = g.tap(t0 + Duration::from_secs_f64(i as f64 * 60.0 / 126.0), 1.05);
        }
        assert!((bpm.unwrap() - 120.0).abs() < 0.01, "{bpm:?}");
        // Pausa longa recomeça a contagem.
        assert_eq!(g.tap(t0 + Duration::from_secs(20), 1.0), None);
        assert_eq!(g.taps(), 1);
    }
}
