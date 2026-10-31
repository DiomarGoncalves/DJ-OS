//! Navegador de músicas igual ao do VirtualDJ: árvore de pastas (pendrives, SSDs e músicas
//! internas) e lista de músicas da pasta selecionada, mais a pesquisa em todas as unidades.

use crate::track::names_from_path;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use dj_os_analise::prepare::is_audio;
const MAX_RESULTS: usize = 500;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Folders,
    Tracks,
    Prepare,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    Source,
    Dir,
    Search,
    /// "Tocadas hoje": o histórico da noite (nó virtual, sem pasta).
    History,
    /// Pendrive do rekordbox: todas as músicas, uma pasta de playlists ou uma playlist (pelo
    /// número no banco). O caminho do nó é a raiz do pendrive.
    RbRoot,
    RbFolder(u32),
    RbPlaylist(u32),
}

impl NodeKind {
    /// Nó sem pasta de verdade por trás (pesquisa, histórico).
    pub fn is_virtual(self) -> bool {
        matches!(self, NodeKind::Search | NodeKind::History)
    }

    /// Item das playlists do rekordbox (não é uma pasta: não se copia nem se abre como pasta).
    pub fn is_rekordbox(self) -> bool {
        matches!(self, NodeKind::RbRoot | NodeKind::RbFolder(_) | NodeKind::RbPlaylist(_))
    }
}

/// Uma linha da árvore de pastas.
#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub depth: usize,
    pub kind: NodeKind,
    pub expanded: bool,
    pub has_children: bool,
}

#[derive(Clone, Debug)]
pub struct TrackEntry {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub ext: String,
    /// BPM já analisado (memória da música), se houver.
    pub bpm: Option<f64>,
    /// Tom já analisado (0-23, veja `key`), se houver.
    pub key: Option<u8>,
    /// Hora em que tocou (só nas "Tocadas hoje").
    pub when: Option<String>,
}

impl TrackEntry {
    pub fn from_path(p: &Path) -> Self {
        let (title, artist) = names_from_path(p);
        let ext = p.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
        Self { path: p.to_path_buf(), title, artist, ext, bpm: None, key: None, when: None }
    }

    /// Completa com o BPM e o tom guardados na memória da música (se já analisada).
    fn with_saved(mut self) -> Self {
        if let Some(p) = crate::store::peek(&self.path) {
            self.bpm = Some(p.bpm);
            self.key = crate::key::from_raw(p.key);
        }
        self
    }

    /// Música do banco do rekordbox: nome, artista, BPM e tom dele. Não abre nada no pendrive
    /// (a lista de todas as músicas pode ter milhares).
    fn from_rekordbox(t: &crate::rekordbox::RbTrack) -> Self {
        let mut e = Self::from_path(&t.path);
        e.title = t.title.clone();
        if !t.artist.is_empty() {
            e.artist = t.artist.clone();
        }
        e.bpm = (t.bpm > 0.0).then_some(t.bpm);
        e.key = crate::key::from_raw(t.key);
        e
    }
}

/// Índice de busca: todas as músicas das unidades, com o texto já normalizado.
#[derive(Default)]
pub struct Index {
    pub entries: Vec<(String, TrackEntry)>,
    pub scanning: bool,
}

/// Minúsculas, sem acentos e só letras/números separados por espaço.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        out.push(c);
    }
    out
}

/// Todas as palavras da busca precisam aparecer (em qualquer ordem).
pub fn matches(key: &str, words: &[String]) -> bool {
    !words.is_empty() && words.iter().all(|w| key.contains(w.as_str()))
}

fn query_words(q: &str) -> Vec<String> {
    normalize(q).split_whitespace().map(str::to_string).collect()
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn is_hidden(p: &Path) -> bool {
    // PIONEER: análise e banco do rekordbox (as playlists aparecem no item "rekordbox").
    p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.') || n == "PIONEER")
}

fn list_dirs(p: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(p)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() && !is_hidden(p)).collect())
        .unwrap_or_default();
    v.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    v
}


fn list_tracks(p: &Path) -> Vec<TrackEntry> {
    // Pasta de um pendrive do rekordbox: nome, artista, BPM e tom vêm do banco dele.
    let rb = crate::rekordbox::find_root(p).and_then(|r| crate::rekordbox::library(&r));
    let mut v: Vec<TrackEntry> = std::fs::read_dir(p)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| is_audio(p))
                .map(|p| match rb.as_ref().and_then(|l| l.by_path(&p)) {
                    Some(t) => TrackEntry::from_rekordbox(t).with_saved(),
                    None => TrackEntry::from_path(&p).with_saved(),
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|t| (t.artist.to_lowercase(), t.title.to_lowercase()));
    v
}

/// Músicas de um item do rekordbox: todas (em ordem de artista), as da playlist (na ordem dela)
/// ou nenhuma (pasta de playlists).
fn rb_tracks(root: &Path, kind: NodeKind) -> Vec<TrackEntry> {
    let Some(lib) = crate::rekordbox::library(root) else { return Vec::new() };
    match kind {
        NodeKind::RbRoot => {
            let mut v: Vec<TrackEntry> = lib.tracks.values().map(TrackEntry::from_rekordbox).collect();
            v.sort_by_key(|t| (t.artist.to_lowercase(), t.title.to_lowercase()));
            v
        }
        NodeKind::RbPlaylist(id) => match lib.node(id) {
            Some(crate::rekordbox::Node::Playlist { tracks, .. }) => tracks.iter().filter_map(|t| lib.tracks.get(t)).map(|t| TrackEntry::from_rekordbox(t).with_saved()).collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Unidades: pendrives/SSDs montados pelo sistema (`/media/<usuário>`, `/run/media/<usuário>`)
/// e as músicas internas. Partições do próprio PC (`/mnt`, outros usuários) ficam de fora.
pub fn sources() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    // Músicas internas aparecem sempre, mesmo vazias e sem pendrive.
    let internal = crate::app::data_dir().join("musicas");
    let _ = std::fs::create_dir_all(&internal);
    out.push(("🏠 Músicas internas".to_string(), internal));
    // Gravações do set no aparelho (só aparece depois da primeira).
    let rec = crate::record::dir();
    if std::fs::read_dir(&rec).is_ok_and(|mut d| d.next().is_some()) {
        out.push(("🎙 Records (gravações)".to_string(), rec));
    }
    let user = std::env::var("USER").unwrap_or_default();
    for base in [PathBuf::from("/media").join(&user), PathBuf::from("/run/media").join(&user)] {
        // Só pendrives montados de verdade: uma pasta que sobrou de uma montagem que falhou
        // apareceria vazia.
        for d in list_dirs(&base).into_iter().filter(|d| is_mountpoint(d)) {
            let name = d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            out.push((format!("💾 {name}"), d));
        }
    }
    for local in ["Música", "Music"] {
        let p = home().join(local);
        if p.is_dir() {
            out.push((format!("🏠 {local}"), p));
        }
    }
    out
}

/// A pasta é o começo de outro disco (está num dispositivo diferente da pasta de cima)?
fn is_mountpoint(p: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(p), p.parent().map(std::fs::metadata)) {
        (Ok(m), Some(Ok(parent))) => m.dev() != parent.dev(),
        _ => false,
    }
}

/// Varre as unidades em segundo plano e preenche o índice de busca.
fn scan_index(index: Arc<Mutex<Index>>, roots: Vec<PathBuf>, gen: Arc<AtomicU64>, my_gen: u64) {
    std::thread::Builder::new()
        .name("indice".into())
        .spawn(move || {
            let mut entries = Vec::new();
            let mut stack: Vec<(PathBuf, usize)> = roots.into_iter().map(|r| (r, 0)).collect();
            while let Some((dir, depth)) = stack.pop() {
                if gen.load(Ordering::Relaxed) != my_gen {
                    return; // outra varredura começou (pendrive trocado)
                }
                let Ok(rd) = std::fs::read_dir(&dir) else { continue };
                for e in rd.flatten() {
                    let p = e.path();
                    if is_hidden(&p) {
                        continue;
                    }
                    if p.is_dir() {
                        // Pastas "records" (gravações do set) ficam fora da pesquisa e da análise.
                        if depth < 12 && p.file_name().is_none_or(|n| n != "records") {
                            stack.push((p, depth + 1));
                        }
                    } else if is_audio(&p) {
                        let t = TrackEntry::from_path(&p);
                        let key = normalize(&format!("{} {} {}", t.artist, t.title, p.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default()));
                        entries.push((key, t));
                    }
                }
            }
            entries.sort_by(|a, b| a.1.artist.to_lowercase().cmp(&b.1.artist.to_lowercase()).then(a.1.title.to_lowercase().cmp(&b.1.title.to_lowercase())));
            if gen.load(Ordering::Relaxed) == my_gen {
                let mut ix = index.lock();
                ix.entries = entries;
                ix.scanning = false;
            }
        })
        .expect("thread do índice");
}

pub struct Library {
    pub folders: Vec<Node>,
    pub folder_sel: usize,
    pub tracks: Vec<TrackEntry>,
    pub track_sel: usize,
    tracks_for: Option<(NodeKind, PathBuf)>,
    pub focus: Focus,
    pub prepare: Vec<TrackEntry>,
    pub prepare_sel: usize,
    /// Última pesquisa: termo e resultados.
    pub results: Option<(String, Vec<TrackEntry>)>,
    pub index: Arc<Mutex<Index>>,
    /// Sobe a cada música analisada em segundo plano (para atualizar o BPM da lista).
    pub analyzed: Arc<AtomicU64>,
    analyzed_seen: u64,
    scan_gen: Arc<AtomicU64>,
    last_check: Instant,
    sources_sig: Vec<PathBuf>,
    /// Músicas tocadas nesta noite (em ordem) e o conjunto para marcar com ✓ nas listas.
    pub history: Vec<TrackEntry>,
    pub played: std::collections::HashSet<PathBuf>,
}

impl Library {
    pub fn new() -> Self {
        let mut lib = Self {
            folders: Vec::new(),
            folder_sel: 0,
            tracks: Vec::new(),
            track_sel: 0,
            tracks_for: None,
            focus: Focus::Folders,
            prepare: Vec::new(),
            prepare_sel: 0,
            results: None,
            index: Arc::new(Mutex::new(Index::default())),
            analyzed: Arc::new(AtomicU64::new(0)),
            analyzed_seen: 0,
            scan_gen: Arc::new(AtomicU64::new(0)),
            last_check: Instant::now(),
            sources_sig: Vec::new(),
            history: Vec::new(),
            played: Default::default(),
        };
        lib.reload_sources();
        lib
    }

    fn node(name: String, path: PathBuf, depth: usize, kind: NodeKind) -> Node {
        let has_children = !kind.is_virtual() && (!list_dirs(&path).is_empty() || crate::rekordbox::is_export(&path));
        Node { name, path, depth, kind, expanded: false, has_children }
    }

    /// Itens das playlists do rekordbox (pastas e playlists, na ordem dele).
    fn rb_nodes(root: &Path, nodes: &[crate::rekordbox::Node], depth: usize) -> Vec<Node> {
        use crate::rekordbox::Node as Rb;
        nodes
            .iter()
            .map(|n| match n {
                Rb::Folder { id, name, children } => {
                    Node { name: format!("📁 {name}"), path: root.to_path_buf(), depth, kind: NodeKind::RbFolder(*id), expanded: false, has_children: !children.is_empty() }
                }
                Rb::Playlist { id, name, tracks } => Node {
                    name: format!("☰ {name} ({})", tracks.len()),
                    path: root.to_path_buf(),
                    depth,
                    kind: NodeKind::RbPlaylist(*id),
                    expanded: false,
                    has_children: false,
                },
            })
            .collect()
    }

    /// Recria a árvore com as unidades atuais e reindexa as músicas.
    fn reload_sources(&mut self) {
        let src = sources();
        self.sources_sig = src.iter().map(|(_, p)| p.clone()).collect();
        let virtuals: Vec<Node> = self.folders.iter().filter(|n| n.kind.is_virtual()).cloned().collect();
        self.folders = virtuals.into_iter().chain(src.into_iter().map(|(n, p)| Self::node(n, p, 0, NodeKind::Source))).collect();
        self.folder_sel = self.folder_sel.min(self.folders.len().saturating_sub(1));
        self.tracks_for = None;
        self.refresh_tracks();
        let my_gen = self.scan_gen.fetch_add(1, Ordering::Relaxed) + 1;
        self.index.lock().scanning = true;
        // Gravações ficam fora da pesquisa e da análise automática (um set de horas não pode ser
        // aberto inteiro na memória só para analisar).
        let rec = crate::record::dir();
        let roots = self.sources_sig.iter().filter(|p| **p != rec).cloned().collect();
        scan_index(self.index.clone(), roots, self.scan_gen.clone(), my_gen);
    }

    /// Mostra as músicas da pasta selecionada (ou os resultados, no item da pesquisa).
    fn refresh_tracks(&mut self) {
        let Some(n) = self.folders.get(self.folder_sel) else {
            self.tracks.clear();
            self.tracks_for = None;
            return;
        };
        let key = (n.kind, n.path.clone());
        if self.tracks_for.as_ref() == Some(&key) {
            return;
        }
        self.tracks = match n.kind {
            NodeKind::Search => self.results.as_ref().map(|(_, r)| r.clone()).unwrap_or_default(),
            // A mais recente em cima.
            NodeKind::History => self.history.iter().rev().cloned().collect(),
            NodeKind::RbRoot | NodeKind::RbFolder(_) | NodeKind::RbPlaylist(_) => rb_tracks(&n.path, n.kind),
            _ => list_tracks(&n.path),
        };
        self.track_sel = 0;
        self.tracks_for = Some(key);
    }

    /// Depois de uma importação: relê as pastas (mantendo a seleção) e reindexa.
    pub fn force_reload(&mut self) {
        let sel = self.folder_sel;
        self.reload_sources();
        self.folder_sel = sel.min(self.folders.len().saturating_sub(1));
        self.tracks_for = None;
        self.refresh_tracks();
    }

    /// Música ou pasta selecionada no painel em foco (para apagar/importar).
    pub fn selected_path(&self) -> Option<PathBuf> {
        match self.focus {
            Focus::Folders => self.folders.get(self.folder_sel).filter(|n| !n.kind.is_virtual() && !n.kind.is_rekordbox()).map(|n| n.path.clone()),
            _ => self.selected_track().map(|t| t.path),
        }
    }

    /// Chamado periodicamente: percebe pendrive/SSD conectado ou removido.
    pub fn tick(&mut self) {
        // Análise automática avançou: atualiza o BPM das músicas da lista.
        let n = self.analyzed.load(Ordering::Relaxed);
        if n != self.analyzed_seen {
            self.analyzed_seen = n;
            for t in self.tracks.iter_mut().filter(|t| t.bpm.is_none() || t.key.is_none()) {
                *t = t.clone().with_saved();
            }
        }
        if self.last_check.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.last_check = Instant::now();
        let now: Vec<PathBuf> = sources().into_iter().map(|(_, p)| p).collect();
        if now != self.sources_sig {
            self.reload_sources();
        }
    }

    pub fn scroll(&mut self, delta: i32) {
        let step = |sel: &mut usize, len: usize| {
            if len > 0 {
                *sel = (*sel as i64 + delta as i64).clamp(0, len as i64 - 1) as usize;
            }
        };
        match self.focus {
            Focus::Folders => {
                step(&mut self.folder_sel, self.folders.len());
                self.refresh_tracks();
            }
            Focus::Tracks => step(&mut self.track_sel, self.tracks.len()),
            Focus::Prepare => step(&mut self.prepare_sel, self.prepare.len()),
        }
    }

    /// Apertar o BROWSE: entra na lista de músicas da pasta; apertar de novo volta para as pastas.
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Folders => Focus::Tracks,
            _ => Focus::Folders,
        };
    }

    pub fn focus_prepare(&mut self) {
        self.focus = Focus::Prepare;
    }

    /// LOAD numa pasta: abre/fecha as subpastas na árvore; sem subpastas, entra nas músicas.
    pub fn open_selected(&mut self) {
        let i = self.folder_sel;
        let Some(n) = self.folders.get(i).cloned() else { return };
        if !n.has_children {
            self.focus = Focus::Tracks;
            return;
        }
        if n.expanded {
            let end = (i + 1..self.folders.len()).find(|&j| self.folders[j].depth <= n.depth).unwrap_or(self.folders.len());
            self.folders.drain(i + 1..end);
            self.folders[i].expanded = false;
        } else {
            let children: Vec<Node> = match n.kind {
                NodeKind::RbRoot => crate::rekordbox::library(&n.path).map_or(Vec::new(), |l| Self::rb_nodes(&n.path, &l.playlists, n.depth + 1)),
                NodeKind::RbFolder(id) => match crate::rekordbox::library(&n.path).as_deref().and_then(|l| l.node(id)) {
                    Some(crate::rekordbox::Node::Folder { children, .. }) => Self::rb_nodes(&n.path, children, n.depth + 1),
                    _ => Vec::new(),
                },
                _ => {
                    // Pendrive do rekordbox: as playlists dele em primeiro.
                    let rb = crate::rekordbox::library(&n.path).map(|l| Node {
                        name: format!("📀 rekordbox ({} músicas)", l.tracks.len()),
                        path: n.path.clone(),
                        depth: n.depth + 1,
                        kind: NodeKind::RbRoot,
                        expanded: false,
                        has_children: !l.playlists.is_empty(),
                    });
                    rb.into_iter()
                        .chain(list_dirs(&n.path).into_iter().map(|d| {
                            let name = d.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                            Self::node(name, d, n.depth + 1, NodeKind::Dir)
                        }))
                        .collect()
                }
            };
            self.folders.splice(i + 1..i + 1, children);
            self.folders[i].expanded = true;
        }
    }

    /// Abre a árvore até `target` e seleciona essa pasta (se ela existir na árvore).
    pub fn reveal(&mut self, target: &Path) {
        for _ in 0..32 {
            let Some(i) = self
                .folders
                .iter()
                .enumerate()
                .filter(|(_, n)| !n.kind.is_virtual() && !n.kind.is_rekordbox() && !n.path.as_os_str().is_empty() && target.starts_with(&n.path))
                .max_by_key(|(_, n)| n.depth)
                .map(|(i, _)| i)
            else {
                return;
            };
            self.folder_sel = i;
            self.focus = Focus::Folders;
            self.tracks_for = None;
            self.refresh_tracks();
            let n = &self.folders[i];
            if n.path == target || n.expanded || !n.has_children {
                return;
            }
            self.open_selected();
        }
    }

    /// Uma música tocou (entra nas "Tocadas hoje" e ganha o ✓ nas listas).
    pub fn add_played(&mut self, entry: TrackEntry) {
        self.played.insert(entry.path.clone());
        self.history.push(entry.with_saved());
        let label = format!("🕘 Tocadas hoje ({})", self.history.len());
        match self.folders.iter_mut().find(|n| n.kind == NodeKind::History) {
            Some(n) => n.name = label,
            None => {
                // Logo depois da pesquisa (se houver), antes das unidades.
                let at = self.folders.iter().take_while(|n| n.kind == NodeKind::Search).count();
                self.folders.insert(at, Node { name: label, path: PathBuf::new(), depth: 0, kind: NodeKind::History, expanded: false, has_children: false });
                if self.folder_sel >= at && self.folders.len() > 1 {
                    self.folder_sel += 1;
                }
            }
        }
        if self.folders.get(self.folder_sel).is_some_and(|n| n.kind == NodeKind::History) {
            self.tracks_for = None;
            self.refresh_tracks();
        }
    }

    /// Música que o LOAD deve carregar, conforme o painel em foco.
    pub fn selected_track(&self) -> Option<TrackEntry> {
        match self.focus {
            Focus::Prepare => self.prepare.get(self.prepare_sel).cloned(),
            _ => self.tracks.get(self.track_sel).cloned(),
        }
    }

    pub fn add_selected_to_prepare(&mut self) -> Option<String> {
        let t = self.tracks.get(self.track_sel)?.clone();
        if self.prepare.iter().any(|p| p.path == t.path) {
            return None;
        }
        let name = t.title.clone();
        self.prepare.push(t);
        Some(name)
    }

    /// Quantas músicas a busca acharia (para mostrar enquanto digita).
    pub fn count_matches(&self, query: &str) -> usize {
        let words = query_words(query);
        self.index.lock().entries.iter().filter(|(k, _)| matches(k, &words)).count()
    }

    /// Pesquisa em todas as unidades. Os resultados viram o item "🔍" no topo da árvore,
    /// selecionado, com o foco na lista de músicas.
    pub fn search(&mut self, query: &str) -> usize {
        let words = query_words(query);
        let found: Vec<TrackEntry> =
            self.index.lock().entries.iter().filter(|(k, _)| matches(k, &words)).take(MAX_RESULTS).map(|(_, t)| t.clone().with_saved()).collect();
        let n = found.len();
        self.results = Some((query.trim().to_string(), found));
        self.folders.retain(|f| f.kind != NodeKind::Search);
        let label = format!("🔍 {} ({n})", query.trim());
        self.folders.insert(0, Node { name: label, path: PathBuf::new(), depth: 0, kind: NodeKind::Search, expanded: false, has_children: false });
        self.folder_sel = 0;
        self.tracks_for = None;
        self.refresh_tracks();
        self.focus = Focus::Tracks;
        n
    }

    #[cfg(test)]
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        let mut lib = Self::new();
        lib.folders = roots
            .iter()
            .map(|p| Self::node(p.file_name().unwrap().to_string_lossy().to_string(), p.clone(), 0, NodeKind::Source))
            .collect();
        lib.folder_sel = 0;
        lib.tracks_for = None;
        lib.refresh_tracks();
        let my_gen = lib.scan_gen.fetch_add(1, Ordering::Relaxed) + 1;
        lib.index.lock().scanning = true;
        scan_index(lib.index.clone(), roots, lib.scan_gen.clone(), my_gen);
        lib
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        let base = std::env::temp_dir().join(format!("dj-os-lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for (dir, files) in [
            ("Funk", vec!["MC Kevin O Chris - Évoluiu.mp3", "DJ Ávila - Batidão.mp3"]),
            ("Funk/Antigas", vec!["MC Créu - Dança.wav"]),
            ("House", vec!["Artista - Noite Longa.flac", "capa.jpg"]),
        ] {
            std::fs::create_dir_all(base.join(dir)).unwrap();
            for f in files {
                std::fs::write(base.join(dir).join(f), b"x").unwrap();
            }
        }
        base
    }

    fn wait_index(lib: &Library) {
        for _ in 0..200 {
            if !lib.index.lock().scanning {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("índice não terminou");
    }

    #[test]
    fn normaliza_acentos_e_maiusculas() {
        assert_eq!(normalize("MC Kevin - Évoluiu Ação"), "mc kevin   evoluiu acao");
        assert!(matches(&normalize("Kevin O Chris - Evoluiu"), &query_words("evoluiu KEVIN")));
        assert!(!matches(&normalize("Kevin O Chris"), &query_words("kevin creu")));
        assert!(!matches("qualquer", &query_words("   ")), "busca vazia não acha nada");
    }

    #[test]
    fn arvore_abre_e_fecha_subpastas() {
        let base = fixture();
        let mut lib = Library::with_roots(vec![base.clone()]);
        assert!(lib.folders[0].has_children);
        lib.open_selected();
        let names: Vec<_> = lib.folders.iter().map(|n| n.name.clone()).collect();
        assert_eq!(names[1..], ["Funk".to_string(), "House".to_string()]);
        lib.scroll(1); // Funk
        assert_eq!(lib.tracks.len(), 2, "músicas da pasta Funk");
        lib.open_selected(); // abre Funk → Antigas
        assert_eq!(lib.folders[2].name, "Antigas");
        assert_eq!(lib.folders[2].depth, 2);
        lib.folder_sel = 0;
        lib.open_selected(); // fecha a raiz inteira
        assert_eq!(lib.folders.len(), 1);
        // Pasta sem subpastas: LOAD entra nas músicas.
        lib.open_selected();
        lib.scroll(2); // House
        lib.open_selected();
        assert_eq!(lib.focus, Focus::Tracks);
        assert_eq!(lib.tracks.len(), 1, "o .jpg não entra");
    }

    #[test]
    fn pesquisa_em_todas_as_pastas() {
        let base = fixture();
        let mut lib = Library::with_roots(vec![base.clone()]);
        wait_index(&lib);
        assert_eq!(lib.count_matches("mc"), 2);
        let n = lib.search("creu danca");
        assert_eq!(n, 1, "acha na subpasta, sem acento");
        assert_eq!(lib.folders[0].kind, NodeKind::Search);
        assert_eq!(lib.focus, Focus::Tracks);
        assert_eq!(lib.selected_track().unwrap().title, "Dança");
        // Nova pesquisa substitui o item.
        lib.search("noite");
        assert_eq!(lib.folders.iter().filter(|f| f.kind == NodeKind::Search).count(), 1);
        assert_eq!(lib.tracks[0].artist, "Artista");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn pendrive_do_rekordbox_mostra_as_musicas_dele() {
        let root = std::env::temp_dir().join(format!("dj-os-rekordbox-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let demo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("analise/tests/rekordbox/demo/PIONEER");
        let copy = |from: &Path, to: &Path| {
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(from, to).unwrap();
        };
        copy(&demo.join("rekordbox/export.pdb"), &root.join("PIONEER/rekordbox/export.pdb"));
        let album = root.join("Contents/Loopmasters/UnknownAlbum");
        std::fs::create_dir_all(&album).unwrap();
        for f in ["Demo Track 1.mp3", "Demo Track 2.mp3"] {
            std::fs::write(album.join(f), b"x").unwrap();
        }
        let mut lib = Library::with_roots(vec![root.clone()]);
        lib.open_selected();
        let names: Vec<_> = lib.folders.iter().map(|n| n.name.clone()).collect();
        assert_eq!(names[1], "📀 rekordbox (2 músicas)", "{names:?}");
        assert!(!names.iter().any(|n| n == "PIONEER"), "a pasta do rekordbox fica escondida");
        lib.scroll(1);
        assert_eq!(lib.tracks.len(), 2);
        assert_eq!((lib.tracks[0].title.as_str(), lib.tracks[0].artist.as_str()), ("Demo Track 1", "Loopmasters"));
        assert_eq!(lib.tracks[0].bpm, Some(128.0));
        assert!(lib.selected_path().is_none(), "o item do rekordbox não é uma pasta para copiar");
        lib.focus = Focus::Folders;
        lib.reveal(&album);
        assert_eq!(lib.tracks.len(), 2);
        assert_eq!(lib.tracks[1].title, "Demo Track 2", "pela pasta também vem o nome do rekordbox");
        assert_eq!(lib.tracks[1].bpm, Some(120.0));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn mostra_a_pasta_copiada_abrindo_a_arvore() {
        // Pasta própria: os outros testes recriam a do fixture() ao mesmo tempo.
        let root = std::env::temp_dir().join(format!("dj-os-arvore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Funk").join("Antigas")).unwrap();
        std::fs::write(root.join("Funk").join("Antigas").join("MC Créu - Dança.wav"), b"x").unwrap();
        let mut lib = Library::with_roots(vec![root.clone()]);
        lib.reveal(&root.join("Funk").join("Antigas"));
        let n = &lib.folders[lib.folder_sel];
        assert_eq!(n.name, "Antigas");
        assert_eq!(n.depth, 2);
        assert!(lib.folders.iter().any(|f| f.name == "Funk" && f.expanded), "abriu os pais");
        assert_eq!(lib.tracks.len(), 1, "já mostra as músicas da pasta");
    }
}
