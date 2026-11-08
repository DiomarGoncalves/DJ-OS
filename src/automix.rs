//! Automix (como no VirtualDJ): o DJ OS toca sozinho, misturando uma música na outra.
//!
//! Toca a fila (SHIFT + LOAD) em ordem; com a fila vazia, segue a lista aberta no browser a partir
//! da música selecionada (pulando as já tocadas). Usa os 2 decks da frente: enquanto um toca, a
//! próxima entra no outro; perto do fim, a próxima começa no BPM e na batida da que está tocando e
//! os faders cruzam em `BATIDAS` batidas. Mexer no PLAY/CUE não desliga: o DJ pode assumir a
//! qualquer hora desligando o automix.

use crate::app::{AppState, Loader};
use crate::library::TrackEntry;
use crate::shared::{Cmd, CmdTx, Snapshot};
use std::path::PathBuf;

/// Tamanho da mixagem (batidas).
pub const BATIDAS: f64 = 16.0;

#[derive(Debug, Default)]
pub struct Automix {
    /// Deck tocando no ar (o outro da frente recebe a próxima).
    no_ar: Option<usize>,
    /// Mixagem em andamento: quanto já foi (0 a 1).
    transicao: Option<f64>,
    /// Última música tirada da lista do browser (a próxima vem depois dela).
    cursor: Option<PathBuf>,
    /// Música pedida para o outro deck (esperando carregar).
    pedida: Option<PathBuf>,
}

/// Liga/desliga; devolve a mensagem para a tela.
pub fn alternar(a: &mut AppState, cmd: &CmdTx) -> String {
    if a.automix.take().is_some() {
        // Devolve os faders para o DJ (os de verdade valem de novo quando forem mexidos).
        for d in a.side {
            cmd.send(Cmd::Fader { deck: d, v: 1.0 });
        }
        return "Automix desligado".into();
    }
    a.automix = Some(Automix::default());
    cmd.send(Cmd::Crossfader(0.5));
    "Automix ligado: toca a fila (ou a lista aberta) e mistura sozinho".into()
}

/// Próxima música: a primeira da fila (sai dela) ou a seguinte na lista do browser, a partir da
/// selecionada, pulando as já tocadas e as que estão nos decks.
fn proxima(a: &mut AppState, cursor: &mut Option<PathBuf>) -> Option<TrackEntry> {
    if !a.lib.prepare.is_empty() {
        let t = a.lib.prepare.remove(0);
        a.lib.prepare_sel = a.lib.prepare_sel.min(a.lib.prepare.len().saturating_sub(1));
        return Some(t);
    }
    let lista = &a.lib.tracks;
    let inicio = match cursor {
        Some(c) => lista.iter().position(|t| &t.path == c).map_or(a.lib.track_sel, |i| i + 1),
        None => a.lib.track_sel,
    };
    let carregadas: Vec<PathBuf> = a.decks.iter().filter_map(|d| d.track.as_ref().map(|t| t.path.clone())).collect();
    let t = lista.iter().skip(inicio).find(|t| !a.lib.played.contains(&t.path) && !carregadas.contains(&t.path))?.clone();
    *cursor = Some(t.path.clone());
    Some(t)
}

/// Pede a próxima música para o deck `d` (devolve `false` se acabaram).
fn carregar_proxima(a: &mut AppState, m: &mut Automix, loader: &Loader, d: usize) -> bool {
    match proxima(a, &mut m.cursor) {
        Some(t) => {
            m.pedida = Some(t.path.clone());
            loader.load_deck(a, d, t.path);
            true
        }
        None => false,
    }
}

/// Chamado a cada quadro da tela.
pub fn tick(a: &mut AppState, s: &Snapshot, cmd: &CmdTx, loader: &Loader, dt: f64) {
    let Some(mut m) = a.automix.take() else { return };
    let [l, r] = a.side;
    let tocando = |d: usize| s.decks[d].playing && s.decks[d].loaded;
    let caminho = |a: &AppState, d: usize| a.decks[d].track.as_ref().map(|t| t.path.clone());
    // A música pedida não carregou (arquivo com erro): esquece e pede outra.
    let falhou = |a: &AppState, m: &Automix, d: usize| m.pedida.is_some() && a.decks[d].loading.is_none() && caminho(a, d) != m.pedida;
    let no_ar = m.no_ar.filter(|&d| tocando(d)).or_else(|| [l, r].into_iter().find(|&d| tocando(d)));
    match no_ar {
        None => {
            // Nada tocando: toca a da esquerda (se ainda não tocou) ou carrega a próxima nela.
            m.transicao = None;
            if a.decks[l].loading.is_some() || !s.decks[l].loaded && m.pedida.is_some() && !falhou(a, &m, l) {
                // Esperando carregar.
            } else if caminho(a, l).is_some_and(|p| m.pedida.as_ref() == Some(&p) || m.pedida.is_none() && !a.lib.played.contains(&p)) && s.decks[l].loaded {
                cmd.send(Cmd::Fader { deck: l, v: 1.0 });
                cmd.send(Cmd::Play(l));
                m.no_ar = Some(l);
                m.pedida = None;
            } else if !carregar_proxima(a, &mut m, loader, l) {
                a.toast("Automix desligado: acabaram as músicas (ponha na fila com SHIFT + LOAD)");
                return;
            }
        }
        Some(p) => {
            let o = if p == l { r } else { l };
            m.no_ar = Some(p);
            if m.transicao.is_none() && !tocando(o) && a.decks[o].loading.is_none() && (m.pedida.is_none() || falhou(a, &m, o)) {
                m.pedida = None;
                carregar_proxima(a, &mut m, loader, o);
            }
            let pronta = m.pedida.is_some() && caminho(a, o) == m.pedida && a.decks[o].loading.is_none() && s.decks[o].loaded;
            let sp = &s.decks[p];
            let bpm = if sp.bpm > 0.0 { sp.bpm } else { 120.0 };
            let dur = BATIDAS * 60.0 / bpm;
            let resta = if sp.sr > 0.0 { (sp.frames - sp.pos) / sp.sr / sp.rate.max(0.01) } else { f64::MAX };
            match m.transicao {
                None if pronta && resta <= dur + 0.5 => {
                    // Começa a próxima: no BPM e na batida da que está tocando, fader fechado.
                    cmd.send(Cmd::Fader { deck: o, v: 0.0 });
                    cmd.send(Cmd::Sync(o));
                    cmd.send(Cmd::Play(o));
                    m.transicao = Some(0.0);
                    eprintln!("[djos] automix: mixando o deck {} no deck {} ({dur:.1} s)", o + 1, p + 1);
                }
                Some(t) => {
                    // A que entra sobe na primeira metade; a que sai desce na segunda.
                    let t = (t + dt / dur).min(1.0);
                    cmd.send(Cmd::Fader { deck: o, v: (t * 2.0).min(1.0) as f32 });
                    cmd.send(Cmd::Fader { deck: p, v: ((1.0 - t) * 2.0).min(1.0) as f32 });
                    if t >= 1.0 || !tocando(p) {
                        // Acabou a mixagem: a anterior para e a nova fica no ar.
                        cmd.send(Cmd::CueStop(p));
                        cmd.send(Cmd::Fader { deck: o, v: 1.0 });
                        cmd.send(Cmd::Fader { deck: p, v: 1.0 });
                        m.no_ar = Some(o);
                        m.transicao = None;
                        m.pedida = None;
                    } else {
                        m.transicao = Some(t);
                    }
                }
                None => {}
            }
        }
    }
    a.automix = Some(m);
}

/// Está ligado? E a mixagem em andamento (0 a 1), para a tela.
pub fn estado(a: &AppState) -> Option<Option<f64>> {
    a.automix.as_ref().map(|m| m.transicao)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::synthetic;
    use parking_lot::Mutex;
    use std::sync::Arc;

    #[test]
    fn mistura_a_proxima_no_fim_da_musica() {
        let (tx, mut rx) = rtrb::RingBuffer::new(256);
        let cmd = CmdTx::new(tx);
        let app = Arc::new(Mutex::new(AppState::new()));
        let snap = Arc::new(Mutex::new(Snapshot::default()));
        let loader = Loader::start(app.clone(), cmd.clone(), snap);
        let mut a = app.lock();
        let sr = 48000.0;
        let t1 = synthetic(60.0, sr, 128.0, 0.25);
        let mut t2 = synthetic(60.0, sr, 128.0, 0.25);
        t2.path = PathBuf::from("b.wav");
        a.side = [0, 1];
        a.decks[0].track = Some(Arc::new(t1));
        a.decks[1].track = Some(Arc::new(t2));
        a.automix = Some(Automix { no_ar: Some(0), pedida: Some(PathBuf::from("b.wav")), ..Default::default() });
        let mut s = Snapshot::default();
        for d in 0..2 {
            s.decks[d].loaded = true;
            s.decks[d].sr = sr;
            s.decks[d].frames = 60.0 * sr;
            s.decks[d].rate = 1.0;
            s.decks[d].bpm = 128.0;
        }
        s.decks[0].playing = true;
        let mut cmds = |rx: &mut rtrb::Consumer<Cmd>| {
            let mut v = Vec::new();
            while let Ok(c) = rx.pop() {
                v.push(format!("{c:?}"));
            }
            v
        };
        // Longe do fim: nada acontece.
        s.decks[0].pos = 20.0 * sr;
        tick(&mut a, &s, &cmd, &loader, 0.02);
        assert!(cmds(&mut rx).is_empty());
        // 16 batidas a 128 BPM = 7,5 s: com 7 s faltando, a próxima entra.
        s.decks[0].pos = 53.0 * sr;
        tick(&mut a, &s, &cmd, &loader, 0.02);
        let v = cmds(&mut rx);
        assert_eq!(v, ["Fader { deck: 1, v: 0.0 }", "Sync(1)", "Play(1)"]);
        // Mixando: os faders cruzam; no fim a anterior para e a nova fica no ar.
        s.decks[1].playing = true;
        tick(&mut a, &s, &cmd, &loader, 3.75);
        let v = cmds(&mut rx);
        assert_eq!(v, ["Fader { deck: 1, v: 1.0 }", "Fader { deck: 0, v: 1.0 }"], "na metade: os dois cheios");
        tick(&mut a, &s, &cmd, &loader, 3.75);
        let v = cmds(&mut rx);
        assert!(v.contains(&"CueStop(0)".to_string()), "{v:?}");
        assert_eq!(a.automix.as_ref().unwrap().no_ar, Some(1));
        // Desligar devolve os faders.
        let msg = alternar(&mut a, &cmd);
        assert!(msg.contains("desligado") && a.automix.is_none());
    }
}
