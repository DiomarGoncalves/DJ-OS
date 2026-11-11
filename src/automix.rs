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
    if let Some(m) = a.automix.take() {
        // Quem está tocando continua no volume cheio (a música não some); o deck parado volta
        // para onde o fader dele está de verdade na controladora (não entra estourando depois).
        for d in a.side {
            let tocando = m.no_ar == Some(d) || m.transicao.is_some();
            let v = if tocando { 1.0 } else { crate::midi::fader_fisico(d).unwrap_or(1.0) };
            cmd.send(Cmd::Fader { deck: d, v });
        }
        return "Automix desligado: confira os faders e o crossfader antes de mixar".into();
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
    let carregando = |a: &AppState, d: usize| a.decks[d].loading.is_some();
    // A música pedida já está no deck `d`.
    let pronta = |a: &AppState, m: &Automix, d: usize| m.pedida.is_some() && caminho(a, d) == m.pedida && !carregando(a, d) && s.decks[d].loaded;
    let no_ar = m.no_ar.filter(|&d| tocando(d)).or_else(|| [l, r].into_iter().find(|&d| tocando(d)));
    match no_ar {
        None => {
            m.transicao = None;
            // A música do deck chegou ao fim (ou ele está vazio)?
            let acabou = |d: usize| {
                let x = &s.decks[d];
                !x.loaded || x.sr <= 0.0 || x.frames - x.pos < x.sr * 0.5
            };
            if m.no_ar.is_some_and(|p| !acabou(p)) {
                // O DJ pausou (ou deu CUE) no deck que estava no ar: espera ele, sem trocar nada.
            } else if let Some(d) = [l, r].into_iter().find(|&d| pronta(a, &m, d)) {
                // A anterior acabou antes da mixagem (ou é a primeira): toca a que está pronta.
                cmd.send(Cmd::Fader { deck: d, v: 1.0 });
                cmd.send(Cmd::Play(d));
                m.no_ar = Some(d);
                m.pedida = None;
            } else if carregando(a, l) || carregando(a, r) {
                // Esperando carregar.
            } else if m.no_ar.is_none() && m.pedida.is_none() && s.decks[l].loaded && caminho(a, l).is_some_and(|p| !a.lib.played.contains(&p)) {
                // Começo: já tem uma música no deck da esquerda que ainda não tocou.
                cmd.send(Cmd::Fader { deck: l, v: 1.0 });
                cmd.send(Cmd::Play(l));
                m.no_ar = Some(l);
            } else {
                // Carrega a próxima no deck que não era o do ar (a pedida que falhou é esquecida).
                let d = if m.no_ar == Some(l) { r } else { l };
                m.pedida = None;
                if !carregar_proxima(a, &mut m, loader, d) {
                    a.toast("Automix desligado: acabaram as músicas (ponha na fila com SHIFT + LOAD)");
                    return;
                }
            }
        }
        Some(p) => {
            let o = if p == l { r } else { l };
            m.no_ar = Some(p);
            // O outro deck recebe a próxima (se a pedida falhou ao carregar, pede outra). Um deck
            // que o DJ deixou tocando no ar não é mexido.
            if m.transicao.is_none() && !tocando(o) && !carregando(a, o) && !pronta(a, &m, o) && !s.decks[o].no_ar {
                m.pedida = None;
                carregar_proxima(a, &mut m, loader, o);
            }
            let sp = &s.decks[p];
            let bpm = if sp.bpm > 0.0 { sp.bpm } else { 120.0 };
            let dur = BATIDAS * 60.0 / bpm;
            let resta = if sp.sr > 0.0 { (sp.frames - sp.pos) / sp.sr / sp.rate.max(0.01) } else { f64::MAX };
            match m.transicao {
                None if pronta(a, &m, o) && resta <= dur + 0.5 => {
                    // Começa a próxima com o fader fechado. BPM parecido (até 8%): no tempo e na
                    // batida da que está tocando. BPM bem diferente: no tempo dela mesma (forçar
                    // deixaria a música irreconhecível), só cruzando os volumes.
                    cmd.send(Cmd::Fader { deck: o, v: 0.0 });
                    let bo = a.decks[o].track.as_ref().map_or(0.0, |t| t.bpm());
                    if bo > 0.0 && sp.bpm > 0.0 && (sp.bpm / bo - 1.0).abs() <= 0.08 {
                        cmd.send(Cmd::Sync(o));
                    } else {
                        cmd.send(Cmd::PitchReset(o));
                    }
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
        let cmds = |rx: &mut rtrb::Consumer<Cmd>| {
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
        // O DJ pausa o deck do ar no meio da música: o automix espera, não carrega nem toca nada.
        s.decks[0].playing = false;
        s.decks[1].playing = false;
        s.decks[1].pos = 20.0 * sr;
        tick(&mut a, &s, &cmd, &loader, 0.02);
        assert!(cmds(&mut rx).is_empty(), "pausado pelo DJ: espera");
        assert!(a.decks.iter().all(|d| d.loading.is_none()));
        // Próxima com BPM bem diferente (95 depois de 128): entra no tempo dela, sem SYNC.
        s.decks[1].playing = true;
        s.decks[1].pos = 53.0 * sr;
        let mut t3 = synthetic(60.0, sr, 95.0, 0.25);
        t3.path = PathBuf::from("c.wav");
        a.decks[0].track = Some(Arc::new(t3));
        a.automix.as_mut().unwrap().pedida = Some(PathBuf::from("c.wav"));
        tick(&mut a, &s, &cmd, &loader, 0.02);
        assert_eq!(cmds(&mut rx), ["Fader { deck: 0, v: 0.0 }", "PitchReset(0)", "Play(0)"]);
        // Desligar devolve os faders.
        let msg = alternar(&mut a, &cmd);
        assert!(msg.contains("desligado") && a.automix.is_none());
    }
}
