//! Testes do motor de áudio (rodar com `cargo test`).

use super::deck::Deck;
use super::fx::{beats_label, step_beats, FxCtx, FxKind, FxSlot, BEATS};
use super::dsp::Limiter;
use super::Engine;
use crate::shared::{Cmd, Snapshot};
use crate::track::{synthetic, Track};
use parking_lot::Mutex;
use std::sync::Arc;

const FS: f64 = 48000.0;

fn track(bpm: f64) -> Arc<Track> {
    Arc::new(synthetic(30.0, FS, bpm, 0.25))
}

fn deck_with(bpm: f64) -> Deck {
    let mut d = Deck::new(FS);
    d.load(track(bpm));
    d
}

/// Toca `frames` frames no deck e devolve o último bloco renderizado (pós-fader).
fn render(d: &mut Deck, frames: usize) -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(frames);
    let (mut pre, mut post) = ([[0.0f32; 2]; 64], [[0.0f32; 2]; 64]);
    let mut left = frames;
    while left > 0 {
        let n = left.min(64);
        d.render(&mut pre[..n], &mut post[..n]);
        out.extend_from_slice(&post[..n]);
        left -= n;
    }
    out
}

// ------------------------------------------------------------------ transporte

#[test]
fn carrega_no_primeiro_tempo_e_parado() {
    let d = deck_with(128.0);
    assert!(!d.playing);
    assert!((d.cue - 0.25 * FS).abs() < 1.0, "cue deve começar na primeira batida");
    assert!(d.at_cue());
}

#[test]
fn cue_estilo_cdj() {
    let mut d = deck_with(128.0);
    // Parado no cue: segurar CUE toca, soltar volta ao cue parado.
    d.cue_button(true);
    assert!(d.playing && d.cue_preview);
    render(&mut d, 4800);
    assert!(d.pos > d.cue);
    d.cue_button(false);
    assert!(!d.playing);
    assert_eq!(d.pos, d.cue);

    // Tocando: CUE volta ao cue e para.
    d.play_toggle();
    render(&mut d, 9600);
    d.cue_button(true);
    assert!(!d.playing);
    assert_eq!(d.pos, d.cue);
    d.cue_button(false);

    // Parado fora do cue: CUE marca um cue novo ali.
    d.pos = 5.0 * FS;
    d.cue_button(true);
    assert_eq!(d.cue, 5.0 * FS);
    assert!(!d.playing);
}

#[test]
fn play_durante_preview_continua_tocando() {
    let mut d = deck_with(128.0);
    d.cue_button(true);
    d.play_toggle();
    d.cue_button(false);
    assert!(d.playing, "CUE segurado + PLAY deve continuar tocando ao soltar o CUE");
}

#[test]
fn hotcue_marca_pula_e_preview() {
    let mut d = deck_with(128.0);
    d.pos = 3.0 * FS;
    d.hotcue(0, true);
    d.hotcue(0, false);
    assert_eq!(d.hotcues[0], Some(3.0 * FS));

    // Parado: segurar o hotcue toca dali, soltar volta e para.
    d.pos = 10.0 * FS;
    d.hotcue(0, true);
    assert!(d.playing);
    render(&mut d, 4800);
    d.hotcue(0, false);
    assert!(!d.playing);
    assert_eq!(d.pos, 3.0 * FS);

    // Tocando: só pula.
    d.play_toggle();
    d.pos = 12.0 * FS;
    d.hotcue(0, true);
    d.hotcue(0, false);
    assert!(d.playing);
}

#[test]
fn para_no_fim_da_musica() {
    let mut d = deck_with(128.0);
    d.pos = 29.99 * FS;
    d.play_toggle();
    render(&mut d, 4800);
    assert!(!d.playing);
}

#[test]
fn pitch_e_faixa() {
    let mut d = deck_with(128.0);
    d.set_tempo_fader(1.0);
    assert!((d.rate - 1.08).abs() < 1e-9);
    d.cycle_pitch_range();
    // Depois de trocar a faixa o fader só volta a valer ao passar pelo valor atual.
    d.set_tempo_fader(1.0);
    assert!((d.rate - 1.08).abs() < 1e-9);
    d.set_tempo_fader(0.75); // 1 + 0.16 * 0.5 = 1.08: pega o fader
    d.set_tempo_fader(1.0);
    assert!((d.rate - 1.16).abs() < 1e-9);
}

// ------------------------------------------------------------------- loops

#[test]
fn autoloop_fica_dentro_do_loop() {
    let mut d = deck_with(120.0);
    d.pos = 2.3 * FS;
    d.play_toggle();
    d.loop_auto(4.0);
    let (a, b) = (d.loop_in.unwrap(), d.loop_out.unwrap());
    assert!((b - a - 4.0 * d.beat_len()).abs() < 1.0, "loop de 4 batidas");
    // A entrada do loop cai numa batida da grade.
    let beats = d.beats_at(a);
    assert!((beats - beats.round()).abs() < 1e-6);
    for _ in 0..20 {
        render(&mut d, 4800);
        assert!(d.pos >= a - 1.0 && d.pos < b + 1.0, "posição {} fora do loop [{a}, {b})", d.pos);
    }
    // Mesmo tamanho de novo: desliga.
    d.loop_auto(4.0);
    assert!(!d.loop_on);
}

#[test]
fn loop_metade_dobro_e_reloop() {
    let mut d = deck_with(120.0);
    d.pos = 5.0 * FS;
    d.loop_toggle();
    assert!(d.loop_on && d.loop_beats == 4.0);
    d.loop_resize(0.5);
    assert_eq!(d.loop_beats, 2.0);
    assert!((d.loop_out.unwrap() - d.loop_in.unwrap() - 2.0 * d.beat_len()).abs() < 1.0);
    d.loop_resize(2.0);
    d.loop_resize(2.0);
    assert_eq!(d.loop_beats, 8.0);
    d.reloop_exit();
    assert!(!d.loop_on);
    d.pos = 20.0 * FS;
    d.reloop_exit();
    assert!(d.loop_on);
    assert_eq!(d.pos, d.loop_in.unwrap());
}

#[test]
fn loop_in_out_manual() {
    let mut d = deck_with(120.0);
    d.pos = 4.0 * FS;
    d.loop_in_point();
    d.pos = 6.0 * FS;
    d.loop_out_point();
    assert!(d.loop_on);
    assert_eq!(d.loop_in, Some(4.0 * FS));
    assert_eq!(d.loop_out, Some(6.0 * FS));
}

#[test]
fn roll_volta_onde_estaria() {
    let mut d = deck_with(120.0);
    d.pos = 3.0 * FS;
    d.play_toggle();
    let start = d.pos;
    d.roll(0.25, true);
    let frames = 48000;
    render(&mut d, frames);
    // Durante o roll a posição fica presa num pedaço de 1/4 de batida.
    d.roll(0.25, false);
    let expected = start + frames as f64;
    assert!((d.pos - expected).abs() < 2.0, "depois do roll deve estar em {expected}, está em {}", d.pos);
}

#[test]
fn beat_jump_anda_batidas() {
    let mut d = deck_with(120.0);
    d.pos = 10.0 * FS;
    d.beat_jump(4.0);
    assert!((d.pos - (10.0 * FS + 4.0 * d.beat_len())).abs() < 1e-6);
    d.beat_jump(-8.0);
    assert!((d.pos - (10.0 * FS - 4.0 * d.beat_len())).abs() < 1e-6);
    d.pos = 0.1 * FS;
    d.beat_jump(-8.0);
    assert_eq!(d.pos, 0.0, "não passa do começo");
}

// --------------------------------------------------------------- mixer do deck

#[test]
fn eq_e_filtro_no_centro_nao_mudam_o_som() {
    let mut d = deck_with(128.0);
    let t = d.track.clone().unwrap();
    d.pos = 1.0 * FS;
    d.play_toggle();
    let start = d.pos;
    let out = render(&mut d, 9600);
    // Depois que os filtros assentam, a saída é igual à música.
    for (i, s) in out.iter().enumerate().skip(4800) {
        let (l, _) = t.sample(start + i as f64);
        assert!((s[0] - l).abs() < 1e-3, "frame {i}: {} vs {l}", s[0]);
    }
}

#[test]
fn eq_no_minimo_corta() {
    let mut d = deck_with(128.0);
    for b in 0..3 {
        d.set_eq(b, 0.0);
    }
    d.play_toggle();
    let out = render(&mut d, 48000);
    let peak = out[24000..].iter().fold(0.0f32, |a, s| a.max(s[0].abs()));
    assert!(peak < 0.01, "com os 3 EQs no mínimo sobrou {peak}");
}

#[test]
fn modo_eq_corta_tudo_ou_ate_26_db() {
    use super::deck::eq_gain;
    assert_eq!(eq_gain(0.0, true), 0.0);
    let g = 20.0 * eq_gain(0.0, false).log10();
    assert!((g + 26.0).abs() < 0.1, "EQ no mínimo: {g} dB");
    for iso in [true, false] {
        assert!((eq_gain(0.5, iso) - 1.0).abs() < 1e-6, "centro em 0 dB");
    }
    // Trocar o modo com o EQ já no mínimo vale na hora.
    let mut d = deck_with(128.0);
    for b in 0..3 {
        d.set_eq(b, 0.0);
    }
    d.set_eq_iso(false);
    d.play_toggle();
    let out = render(&mut d, 48000);
    let peak = out[24000..].iter().fold(0.0f32, |a, s| a.max(s[0].abs()));
    assert!(peak > 0.005, "modo EQ: o som baixa, mas não some ({peak})");
}

#[test]
fn curvas_do_fader_de_canal() {
    use super::deck::fader_gain;
    for c in 0..3 {
        assert_eq!(fader_gain(0.0, c), 0.0);
        assert_eq!(fader_gain(1.0, c), 1.0);
    }
    assert!(fader_gain(0.5, 0) < fader_gain(0.5, 1) && fader_gain(0.5, 1) < fader_gain(0.5, 2));
}

#[test]
fn fader_no_zero_cala() {
    let mut d = deck_with(128.0);
    d.set_fader(0.0);
    d.play_toggle();
    let out = render(&mut d, 24000);
    assert!(out[12000..].iter().all(|s| s[0].abs() < 1e-4));
}

// ------------------------------------------------------------------- efeitos

fn noise(n: usize, seed: &mut u32) -> Vec<[f32; 2]> {
    (0..n)
        .map(|_| {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 17;
            *seed ^= *seed << 5;
            let v = (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
            [v * 0.5, v * 0.5]
        })
        .collect()
}

#[test]
fn efeitos_nao_estouram() {
    let mut seed = 99u32;
    for kind in FxKind::ALL {
        for &beats in &BEATS {
            let mut f = FxSlot::new(kind, FS as f32);
            f.beats = beats;
            f.set_level(1.0);
            f.set_active(true);
            let mut ctx = FxCtx { beat_len: (FS * 60.0 / 128.0) as f32, beat_pos: 0.0 };
            for (i, x) in noise(96000, &mut seed).into_iter().enumerate() {
                ctx.beat_pos = i as f64 / ctx.beat_len as f64;
                let y = f.process(x, &ctx);
                assert!(y[0].is_finite() && y[1].is_finite(), "{kind:?} {beats}: NaN/inf");
                assert!(y[0].abs() < 8.0 && y[1].abs() < 8.0, "{kind:?} {beats}: estourou ({})", y[0]);
            }
        }
    }
}

#[test]
fn cauda_continua_e_depois_some() {
    let mut seed = 7u32;
    for kind in [FxKind::Echo, FxKind::PingPong, FxKind::Reverb] {
        let mut f = FxSlot::new(kind, FS as f32);
        f.set_level(0.8);
        f.set_active(true);
        let ctx = FxCtx { beat_len: (FS * 60.0 / 128.0) as f32, beat_pos: 0.0 };
        for x in noise(48000, &mut seed) {
            f.process(x, &ctx);
        }
        f.set_active(false);
        // Logo depois de desligar ainda sai som (a cauda), mesmo sem entrada.
        let mut early = 0.0f32;
        for _ in 0..9600 {
            early = early.max(f.process([0.0, 0.0], &ctx)[0].abs());
        }
        assert!(early > 1e-3, "{kind:?}: a cauda sumiu na hora");
        // Depois de bastante tempo, some e o efeito para de processar.
        for _ in 0..(48000 * 20) {
            f.process([0.0, 0.0], &ctx);
        }
        let late = f.process([0.0, 0.0], &ctx)[0].abs();
        assert!(late < 1e-4, "{kind:?}: a cauda não sumiu ({late})");
        assert!(!f.sounding(), "{kind:?}: continua marcado como soando");
    }
}

#[test]
fn efeito_desligado_sem_cauda_passa_direto() {
    let mut seed = 3u32;
    for kind in [FxKind::Flanger, FxKind::Phaser, FxKind::Wahwah, FxKind::Noise, FxKind::Cut] {
        let mut f = FxSlot::new(kind, FS as f32);
        f.set_level(1.0);
        let ctx = FxCtx { beat_len: 22500.0, beat_pos: 0.0 };
        for x in noise(1000, &mut seed) {
            assert_eq!(f.process(x, &ctx), x, "{kind:?} desligado mexeu no som");
        }
    }
}

// --------------------------------------------------------------- motor inteiro

struct Rig {
    engine: Engine,
    tx: rtrb::Producer<Cmd>,
    snap: Arc<Mutex<Snapshot>>,
}

fn rig() -> Rig {
    let (tx, rx) = rtrb::RingBuffer::new(1024);
    let (trash_tx, _trash_rx) = rtrb::RingBuffer::new(64);
    let snap = Arc::new(Mutex::new(Snapshot::default()));
    Rig { engine: Engine::new(FS, rx, trash_tx, snap.clone()), tx, snap }
}

impl Rig {
    fn send(&mut self, c: Cmd) {
        assert!(self.tx.push(c).is_ok());
    }

    /// Roda o motor com 4 canais e devolve o pico de cada canal.
    fn run(&mut self, frames: usize) -> [f32; 4] {
        let mut buf = vec![0.0f32; 512 * 4];
        let mut peak = [0.0f32; 4];
        for _ in 0..frames / 512 {
            self.engine.process(&mut buf, 4);
            for f in buf.chunks_exact(4) {
                for c in 0..4 {
                    peak[c] = peak[c].max(f[c].abs());
                }
            }
        }
        peak
    }
}

#[test]
fn crossfader_corta_o_lado_oposto() {
    let mut r = rig();
    r.send(Cmd::Load { deck: 1, track: track(128.0) });
    r.send(Cmd::PlayToggle(1));
    r.send(Cmd::Crossfader(0.0));
    r.run(48000);
    let p = r.run(24000);
    assert!(p[0] < 1e-3, "crossfader todo à esquerda deveria calar o deck 2 (sobrou {})", p[0]);
    r.send(Cmd::Crossfader(1.0));
    r.run(24000);
    let p = r.run(24000);
    assert!(p[0] > 0.1, "crossfader à direita deveria tocar o deck 2");
}

#[test]
fn deck_fora_do_crossfader_ignora_o_crossfader() {
    let mut r = rig();
    r.send(Cmd::Load { deck: 1, track: track(128.0) });
    r.send(Cmd::PlayToggle(1));
    r.send(Cmd::Crossfader(0.0));
    r.send(Cmd::XfAssign { deck: 1, side: 1 });
    r.run(48000);
    let p = r.run(24000);
    assert!(p[0] > 0.1, "THRU: o deck 2 toca com o crossfader todo à esquerda");
    r.send(Cmd::XfAssign { deck: 1, side: 0 });
    r.run(24000);
    let p = r.run(24000);
    assert!(p[0] > 0.1, "deck 2 no lado A: toca com o crossfader à esquerda");
}

#[test]
fn fone_mono_dividido() {
    let mut r = rig();
    r.send(Cmd::Load { deck: 0, track: track(128.0) });
    r.send(Cmd::PlayToggle(0));
    r.send(Cmd::Pfl { deck: 0, on: true });
    r.send(Cmd::Fader { deck: 0, v: 0.0 });
    r.send(Cmd::HpSplit(true));
    r.run(48000);
    let p = r.run(24000);
    assert!(p[2] > 0.1, "esquerdo: o CUE");
    assert!(p[3] < 1e-3, "direito: o master (mudo com o fader fechado)");
}

#[test]
fn saidas_master_e_fone_separadas() {
    let mut r = rig();
    r.send(Cmd::Load { deck: 0, track: track(128.0) });
    r.send(Cmd::PlayToggle(0));
    r.send(Cmd::Pfl { deck: 0, on: true });
    r.send(Cmd::Fader { deck: 0, v: 0.0 });
    r.send(Cmd::HeadphoneMix(0.0));
    r.run(48000);
    let p = r.run(24000);
    assert!(p[0] < 1e-3 && p[1] < 1e-3, "fader fechado: master (1-2) deve ficar mudo");
    assert!(p[2] > 0.1 && p[3] > 0.1, "PFL ligado: o fone (3-4) deve tocar mesmo com o fader fechado");

    r.send(Cmd::Pfl { deck: 0, on: false });
    r.run(24000);
    let p = r.run(24000);
    assert!(p[2] < 1e-3, "PFL desligado e mix só cue: fone mudo");
}

#[test]
fn sync_iguala_bpm_e_fase() {
    let mut r = rig();
    r.send(Cmd::Load { deck: 0, track: track(128.0) });
    r.send(Cmd::Load { deck: 1, track: Arc::new(synthetic(30.0, FS, 120.0, 0.4)) });
    r.send(Cmd::PlayToggle(0));
    r.send(Cmd::PlayToggle(1));
    r.run(48000);
    r.send(Cmd::Sync(1));
    r.run(4096);
    let s = *r.snap.lock();
    let (a, b) = (s.decks[0], s.decks[1]);
    assert!((a.bpm - b.bpm).abs() < 0.01, "BPM depois do sync: {} vs {}", a.bpm, b.bpm);
    let fase = |d: &crate::shared::DeckSnap| {
        let bl = d.sr * 60.0 / d.bpm_track;
        let first = if d.bpm_track == 128.0 { 0.25 } else { 0.4 } * d.sr;
        ((d.pos - first) / bl).rem_euclid(1.0)
    };
    let diff = (fase(&a) - fase(&b)).abs();
    assert!(diff.min(1.0 - diff) < 0.02, "fase depois do sync difere {diff}");
    assert!(b.sync_ok, "LED de sync deveria acender");
}

#[test]
fn comandos_com_deck_invalido_nao_quebram() {
    let mut r = rig();
    r.send(Cmd::Load { deck: 9, track: track(128.0) });
    r.send(Cmd::PlayToggle(7));
    r.send(Cmd::FxActive { deck: 5, slot: 0, on: true });
    r.send(Cmd::SamplerStop(40));
    r.run(4096);
}

#[test]
fn batidas_incluem_uma_e_meia() {
    assert_eq!(step_beats(1.0, 1), 1.5);
    assert_eq!(step_beats(1.5, 1), 2.0);
    assert_eq!(step_beats(1.0 / 8.0, -1), 1.0 / 16.0);
    assert_eq!(step_beats(1.0 / 16.0, -1), 1.0 / 32.0, "eco robô");
    assert_eq!(step_beats(1.0 / 64.0, -1), 1.0 / 64.0, "não passa do mínimo");
    assert_eq!(step_beats(32.0, 1), 32.0, "não passa do máximo");
    assert_eq!(beats_label(1.0 / 64.0), "1/64");
    assert_eq!(beats_label(1.5), "1 1/2");
    assert_eq!(beats_label(0.125), "1/8");
    assert_eq!(beats_label(0.75), "3/4");
    assert_eq!(beats_label(2.0), "2");
}

#[test]
fn forca_zero_nao_muda_o_som() {
    let mut seed = 11u32;
    for kind in FxKind::ALL {
        let mut f = FxSlot::new(kind, FS as f32);
        f.set_level(0.0);
        f.set_active(true);
        let ctx = FxCtx { beat_len: 22500.0, beat_pos: 0.0 };
        for x in noise(4800, &mut seed) {
            let y = f.process(x, &ctx);
            assert!((y[0] - x[0]).abs() < 1e-6, "{kind:?} com força 0 mudou o som");
        }
    }
}

#[test]
fn um_efeito_por_deck() {
    let d = Deck::new(FS);
    assert_eq!(d.fx.len(), 1);
    assert_eq!(d.fx[0].kind, FxKind::Echo);
}

#[test]
fn restaura_cue_hotcues_e_loop_salvos() {
    let mut d = deck_with(120.0);
    let mut hc = [None; 8];
    hc[2] = Some(3.0 * FS);
    d.restore(Some(5.0 * FS), hc, Some((6.0 * FS, 8.0 * FS, 4.0)));
    assert_eq!(d.cue, 5.0 * FS);
    assert_eq!(d.pos, 5.0 * FS, "parado: vai para o cue salvo");
    assert_eq!(d.hotcues[2], Some(3.0 * FS));
    assert!(!d.loop_on, "loop salvo fica desligado");
    d.reloop_exit();
    assert!(d.loop_on && d.pos == 6.0 * FS, "RELOOP usa o loop salvo");
}

#[test]
fn bluetooth_recebe_o_master_e_o_fio_pode_atrasar() {
    let mut r = rig();
    let (bt_tx, mut bt_rx) = rtrb::RingBuffer::<[f32; 2]>::new(1 << 18);
    r.send(Cmd::BtAttach(bt_tx));
    r.send(Cmd::BtSource(1));
    r.send(Cmd::MasterDelay(100.0));
    r.run(48000); // atraso assenta antes de tocar
    while bt_rx.pop().is_ok() {}
    r.send(Cmd::Load { deck: 0, track: track(120.0) });
    r.send(Cmd::Crossfader(0.0));
    r.send(Cmd::PlayToggle(0));
    // Roda e guarda o master com fio (canal 0) e o que foi para o Bluetooth.
    let mut wired = Vec::new();
    let mut buf = vec![0.0f32; 512 * 4];
    for _ in 0..40 {
        r.engine.process(&mut buf, 4);
        wired.extend(buf.chunks_exact(4).map(|f| f[0]));
    }
    let bt: Vec<f32> = std::iter::from_fn(|| bt_rx.pop().ok()).map(|f| f[0]).collect();
    assert_eq!(bt.len(), wired.len(), "o Bluetooth recebe um quadro por quadro do motor");
    let onset = |v: &[f32]| v.iter().position(|x| x.abs() > 0.3).expect("clique da batida");
    let delay = onset(&wired) as i64 - onset(&bt) as i64;
    assert!((delay - 4800).abs() < 60, "o master com fio deveria sair 100 ms (4800 quadros) depois; saiu {delay}");
    r.send(Cmd::BtDetach);
    r.run(512);
}

/// Frequência dominante pelos cruzamentos de zero (subindo) num trecho.
fn freq(samples: &[[f32; 2]]) -> f64 {
    let ups = samples.windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count();
    ups as f64 / (samples.len() as f64 / FS)
}

fn tone_deck(keylock: bool) -> Deck {
    let mut d = Deck::new(FS);
    // Só a senoide de 220 Hz (a primeira batida fica depois do fim: sem cliques).
    d.load(Arc::new(synthetic(30.0, FS, 120.0, 100.0)));
    d.keylock = keylock;
    d.pos = FS;
    d.set_tempo_fader(1.0); // +8%
    d.play_toggle();
    d
}

#[test]
fn keylock_mantem_o_tom_e_muda_o_andamento() {
    let mut sem = tone_deck(false);
    let out = render(&mut sem, 72000);
    let f_sem = freq(&out[24000..]);
    assert!((f_sem - 237.6).abs() < 2.0, "sem keylock o tom sobe 8% (deu {f_sem:.1} Hz)");

    let mut com = tone_deck(true);
    let start = com.pos;
    let out = render(&mut com, 72000);
    let f_com = freq(&out[24000..]);
    assert!((f_com - 220.0).abs() < 2.0, "com keylock o tom fica em 220 Hz (deu {f_com:.1} Hz)");
    let advanced = (com.pos - start) / 72000.0;
    assert!((advanced - 1.08).abs() < 1e-6, "o andamento acelera 8% (andou {advanced:.4}x)");
    // Volume parecido com o original (a janela soma 1).
    let peak = out[24000..].iter().fold(0.0f32, |a, s| a.max(s[0].abs()));
    assert!((peak - 0.3).abs() < 0.05, "volume do keylock: {peak}");
}

#[test]
fn celular_no_master_abaixa_quando_um_deck_toca() {
    let mut r = rig();
    let (mut tx, rx) = rtrb::RingBuffer::<[f32; 2]>::new(48000 * 8);
    for _ in 0..48000 * 8 - 1 {
        let _ = tx.push([0.3, 0.3]);
    }
    r.send(Cmd::AmbientVolume(1.0));
    r.send(Cmd::LineInRoute(None));
    r.send(Cmd::LineInAttach(rx));
    r.run(48000);
    let p = r.run(24000);
    assert!(p[0] > 0.2, "sem deck tocando o celular sai no master ({})", p[0]);
    // Deck tocando com o fader aberto, mas com o crossfader fechado para ele: só o celular mudaria.
    r.send(Cmd::Load { deck: 1, track: track(128.0) });
    r.send(Cmd::Crossfader(0.0));
    r.send(Cmd::PlayToggle(1));
    r.run(48000 * 3);
    let p = r.run(24000);
    assert!(p[0] < 0.01, "com um deck tocando o celular abaixa sozinho ({})", p[0]);
}

#[test]
fn celular_entra_no_deck_4_pelo_fader_e_no_fone() {
    let mut r = rig();
    let (mut tx, rx) = rtrb::RingBuffer::<[f32; 2]>::new(48000 * 4);
    for _ in 0..48000 * 4 - 1 {
        let _ = tx.push([0.3, 0.3]);
    }
    r.send(Cmd::LineInRoute(Some(3)));
    r.send(Cmd::LineInAttach(rx));
    r.send(Cmd::Fader { deck: 3, v: 0.0 });
    r.send(Cmd::Pfl { deck: 3, on: true });
    r.run(24000);
    let p = r.run(12000);
    assert!(p[0] < 0.01, "fader fechado: nada no master ({})", p[0]);
    assert!(p[2] > 0.1, "CUE do deck 4 manda o celular para o fone ({})", p[2]);
    assert!(r.snap.lock().decks[3].live, "tela mostra o deck 4 como celular");
    r.send(Cmd::Fader { deck: 3, v: 1.0 });
    r.run(12000);
    let p = r.run(12000);
    assert!(p[0] > 0.1, "fader aberto: celular no master ({})", p[0]);
}

#[test]
fn celular_no_master_como_ambiente() {
    let mut r = rig();
    let (mut tx, rx) = rtrb::RingBuffer::<[f32; 2]>::new(48000 * 4);
    for _ in 0..48000 * 4 - 1 {
        let _ = tx.push([0.3, 0.3]);
    }
    r.send(Cmd::AmbientVolume(1.0));
    r.send(Cmd::LineInRoute(None));
    r.send(Cmd::LineInAttach(rx));
    r.run(48000);
    let p = r.run(12000);
    assert!(p[0] > 0.2, "celular sai no master ({})", p[0]);
    assert!(!r.snap.lock().decks[3].live);
}

#[test]
fn backspin_segue_o_prato_solto_e_depois_volta_a_tocar() {
    let mut d = deck_with(128.0);
    d.vinyl = true;
    d.play_toggle();
    render(&mut d, 48000);
    let start = d.pos;
    // Segura o prato, puxa para trás e solta.
    d.jog_touch(true);
    d.jog_scratch(-60.0);
    render(&mut d, 256);
    d.jog_touch(false);
    // O prato solto continua girando para trás por inércia (a REV1 manda como lateral), cada vez
    // mais devagar, por 0,4 s.
    let after_release = d.pos;
    for i in 0..40 {
        d.jog_bend(-(40.0 - i as f64) * 0.5);
        render(&mut d, 480);
    }
    assert!(d.pos < after_release - 0.3 * 48000.0 * 0.1, "seguiu o prato para trás depois de soltar ({} → {})", after_release, d.pos);
    assert!(d.pos < start, "a música voltou");
    // O prato parou: em menos de meio segundo volta a tocar normal, para a frente.
    render(&mut d, 24000);
    let p = d.pos;
    render(&mut d, 48000);
    let speed = (d.pos - p) / 48000.0;
    assert!((speed - 1.0).abs() < 0.02, "voltou à velocidade normal ({speed:.3})");
}

#[test]
fn limitador_segura_o_master_sem_distorcer() {
    // Dois decks no máximo, EQ de grave em cima e master no máximo: soma muito acima de 0 dB.
    let mut r = rig();
    for d in 0..2 {
        r.send(Cmd::Load { deck: d, track: Arc::new(synthetic(30.0, FS, 128.0, 0.9)) });
        r.send(Cmd::Fader { deck: d, v: 1.0 });
        r.send(Cmd::Trim { deck: d, v: 1.0 });
        r.send(Cmd::Eq { deck: d, band: 0, v: 1.0 });
        r.send(Cmd::PlayToggle(d));
    }
    r.send(Cmd::Crossfader(0.5));
    r.send(Cmd::MasterLevel(1.0));
    r.run(24000);
    let p = r.run(48000);
    assert!(p[0] <= Limiter::THRESHOLD + 0.001 && p[1] <= Limiter::THRESHOLD + 0.001, "master nunca passa de -1 dBFS ({:?})", p);
    assert!(p[0] > 0.8, "mas continua alto ({:?})", p);
    assert!(r.snap.lock().master_limit < 0.9, "a tela mostra que está limitando");
}

#[test]
fn ganho_automatico_nivela_musicas_alta_e_baixa() {
    // Mesma música, uma "masterizada" 8 dB mais alta: com o ganho automático as duas saem iguais.
    let mut alta = synthetic(10.0, FS, 128.0, 0.25);
    let mut baixa = synthetic(10.0, FS, 128.0, 0.25);
    let g = 10f32.powf(-8.0 / 20.0);
    baixa.data.iter_mut().for_each(|x| *x *= g);
    // Volumes como os medidos na análise (dentro da faixa de correção).
    alta.loudness = -5.0;
    baixa.loudness = -13.0;
    let mut d1 = Deck::new(FS);
    let mut d2 = Deck::new(FS);
    d1.load(Arc::new(alta));
    d2.load(Arc::new(baixa));
    assert!((d2.auto_gain_db() - d1.auto_gain_db() - 8.0).abs() < 0.2, "correções diferem em 8 dB");
    d1.play_toggle();
    d2.play_toggle();
    // Mede depois de a correção assentar (ela entra suave, em ~10 ms).
    let peak = |d: &mut Deck| {
        render(d, 4800);
        render(d, 48000).iter().fold(0.0f32, |a, s| a.max(s[0].abs()))
    };
    let (p1, p2) = (peak(&mut d1), peak(&mut d2));
    assert!((20.0 * (p1 / p2).log10()).abs() < 0.3, "saem no mesmo nível ({p1:.3} vs {p2:.3})");
    // Desligado: a diferença original volta.
    d1.set_auto_gain(false);
    d2.set_auto_gain(false);
    let (p1, p2) = (peak(&mut d1), peak(&mut d2));
    assert!((20.0 * (p1 / p2).log10() - 8.0).abs() < 0.3, "sem ganho automático: 8 dB de diferença");
}

#[test]
fn gravacao_recebe_o_master() {
    let mut r = rig();
    let (tx, mut rx) = rtrb::RingBuffer::<[f32; 2]>::new(48000 * 2);
    r.send(Cmd::Load { deck: 0, track: track(128.0) });
    r.send(Cmd::Crossfader(0.0));
    r.send(Cmd::PlayToggle(0));
    r.send(Cmd::RecAttach(tx));
    let p = r.run(24064);
    let mut n = 0;
    let mut peak = 0.0f32;
    while let Ok(f) = rx.pop() {
        n += 1;
        peak = peak.max(f[0].abs());
    }
    assert_eq!(n, 24064, "um quadro gravado por quadro tocado");
    assert!((peak - p[0]).abs() < 1e-6, "grava o mesmo que sai no master ({peak} vs {})", p[0]);
    r.send(Cmd::RecDetach);
    r.run(4096);
    assert!(rx.pop().is_err(), "parou de gravar");
}
