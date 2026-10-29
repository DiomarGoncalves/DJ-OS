//! Histórico da noite: uma música conta como tocada depois de 30 s tocando com o fader aberto
//! (pré-escuta no fone ou um play rápido não contam). Entra nas "Tocadas hoje" do browser, ganha
//! um ✓ nas listas e vai para uma playlist M3U8 em `~/DJOS/historico/` (um arquivo por noite).

use crate::app::{data_dir, AppState};
use crate::shared::{Snapshot, NUM_DECKS};
use std::io::Write;
use std::path::PathBuf;

/// Tempo tocando com o fader aberto para contar.
pub const PLAYED_SECS: f32 = 30.0;
/// Fader acima disto = a música está no ar.
const FADER_OPEN: f32 = 0.1;

#[derive(Default)]
pub struct Tracker {
    /// Por deck: música (geração), segundos no ar e se já contou.
    decks: [(u64, f32, bool); NUM_DECKS],
    /// Playlist desta noite (criada na primeira música).
    file: Option<PathBuf>,
}

pub fn dir() -> PathBuf {
    data_dir().join("historico")
}

/// Chamado a cada quadro da tela com o tempo desde o anterior.
pub fn tick(app: &mut AppState, snap: &Snapshot, dt: f32) {
    let mut newly = Vec::new();
    for d in 0..NUM_DECKS {
        let s = &snap.decks[d];
        let Some(track) = app.decks[d].track.clone() else { continue };
        let st = &mut app.history.decks[d];
        if st.0 != track.gen {
            *st = (track.gen, 0.0, false);
        }
        if s.loaded && s.playing && !s.live && s.fader > FADER_OPEN {
            st.1 += dt.min(0.5);
        }
        if !st.2 && st.1 >= PLAYED_SECS {
            st.2 = true;
            newly.push(track);
        }
    }
    for t in newly {
        let when = chrono::Local::now().format("%H:%M").to_string();
        let mut entry = crate::library::TrackEntry::from_path(&t.path);
        entry.when = Some(when.clone());
        append_to_file(app, &t, &when);
        app.lib.add_played(entry);
    }
}

/// Acrescenta a música na playlist M3U8 da noite (cria o arquivo na primeira).
fn append_to_file(app: &mut AppState, t: &crate::track::Track, when: &str) {
    if cfg!(test) {
        return; // os testes não mexem no histórico de verdade
    }
    let file = app.history.file.get_or_insert_with(|| dir().join(format!("{}.m3u8", chrono::Local::now().format("%Y-%m-%d-%H%M"))));
    let new = !file.exists();
    let _ = std::fs::create_dir_all(dir());
    let res = std::fs::OpenOptions::new().create(true).append(true).open(&*file).and_then(|mut f| {
        if new {
            writeln!(f, "#EXTM3U")?;
        }
        writeln!(f, "#EXTINF:{:.0},{}", t.duration_secs(), t.display_name())?;
        writeln!(f, "# tocou às {when}")?;
        writeln!(f, "{}", t.path.display())
    });
    if let Err(e) = res {
        eprintln!("[djos] não salvou o histórico: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::DeckSnap;
    use std::sync::Arc;

    #[test]
    fn conta_depois_de_30s_no_ar() {
        let mut app = AppState::new();
        let mut t = crate::track::synthetic(60.0, 48000.0, 128.0, 0.1);
        t.path = PathBuf::from("/tmp/dj-os-teste-historico/Artista - Musica.wav");
        app.decks[0].track = Some(Arc::new(t));
        let mut snap = Snapshot::default();
        snap.decks[0] = DeckSnap { loaded: true, playing: true, fader: 1.0, ..Default::default() };
        // Fader fechado (pré-escuta): não conta.
        snap.decks[0].fader = 0.0;
        for _ in 0..40 * 30 {
            tick(&mut app, &snap, 1.0 / 30.0);
        }
        assert!(app.lib.history.is_empty(), "pré-escuta com o fader fechado não conta");
        // No ar: conta aos 30 s, uma vez só.
        snap.decks[0].fader = 1.0;
        for _ in 0..29 * 30 {
            tick(&mut app, &snap, 1.0 / 30.0);
        }
        assert!(app.lib.history.is_empty(), "ainda não chegou a 30 s");
        for _ in 0..60 * 30 {
            tick(&mut app, &snap, 1.0 / 30.0);
        }
        assert_eq!(app.lib.history.len(), 1);
        assert!(app.lib.played.contains(&PathBuf::from("/tmp/dj-os-teste-historico/Artista - Musica.wav")));
        assert!(app.lib.history[0].when.is_some());
    }
}
