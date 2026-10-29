//! Retomar depois de um travamento: a cada 2 s guarda a música e a posição de cada deck. Se o
//! programa cair no meio do set, a sessão do aparelho o reabre com `DJOS_RETOMAR=1` e as músicas
//! voltam carregadas no mesmo ponto (paradas, prontas para o play). Ao ligar o aparelho, começa
//! limpo (como uma CDJ).

use crate::app::{data_dir, AppState, Loader};
use crate::shared::{Snapshot, NUM_DECKS};
use std::path::PathBuf;

fn file() -> PathBuf {
    data_dir().join("estado-dos-decks.txt")
}

/// Estado dos decks em texto: uma linha por deck carregado, "deck<TAB>posição<TAB>arquivo".
pub fn encode(app: &AppState, snap: &Snapshot) -> String {
    let mut out = String::new();
    for d in 0..NUM_DECKS {
        if let (Some(t), true) = (&app.decks[d].track, snap.decks[d].loaded && !snap.decks[d].live) {
            out.push_str(&format!("{d}\t{:.0}\t{}\n", snap.decks[d].pos, t.path.display()));
        }
    }
    out
}

pub fn decode(text: &str) -> Vec<(usize, f64, PathBuf)> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.splitn(3, '\t');
            let d = f.next()?.parse::<usize>().ok().filter(|d| *d < NUM_DECKS)?;
            let pos = f.next()?.parse::<f64>().ok().filter(|p| p.is_finite() && *p >= 0.0)?;
            Some((d, pos, PathBuf::from(f.next()?)))
        })
        .collect()
}

/// Guarda se mudou (chamado a cada 2 s pela tela).
pub fn save(app: &AppState, snap: &Snapshot, last: &mut String) {
    let now = encode(app, snap);
    if now != *last {
        let tmp = file().with_extension("tmp");
        if std::fs::write(&tmp, &now).and_then(|_| std::fs::rename(&tmp, file())).is_ok() {
            *last = now;
        }
    }
}

/// Na abertura: se a sessão reabriu o programa depois de um travamento, recarrega os decks.
pub fn restore_after_crash(app: &mut AppState, loader: &Loader) {
    if std::env::var_os("DJOS_RETOMAR").is_none() {
        return;
    }
    let decks: Vec<_> = decode(&std::fs::read_to_string(file()).unwrap_or_default()).into_iter().filter(|(_, _, p)| p.is_file()).collect();
    if decks.is_empty() {
        return;
    }
    for (d, pos, path) in decks {
        loader.resume_deck(app, d, path, pos);
    }
    app.toast("O DJ OS reiniciou: as músicas voltaram para os decks, no mesmo ponto".to_string());
    eprintln!("[djos] retomando depois de reiniciar");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_o_que_grava_e_ignora_linhas_ruins() {
        let txt = "0\t48000\t/musicas/a b.mp3\n3\t96000\t/x/c.wav\n9\t1\t/fora.mp3\nlixo\n1\t-5\t/neg.mp3\n";
        let d = decode(txt);
        assert_eq!(d, vec![(0, 48000.0, PathBuf::from("/musicas/a b.mp3")), (3, 96000.0, PathBuf::from("/x/c.wav"))]);
    }
}
