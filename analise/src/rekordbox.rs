//! Pendrive exportado pelo rekordbox: músicas, playlists, grade de batidas, tom e cues.
//!
//! Lê o banco `PIONEER/rekordbox/export.pdb` e os arquivos de análise `ANLZ0000.DAT`/`.EXT`
//! pelo formato documentado em público pela comunidade (Deep Symmetry, "crate-digger"), conferido
//! com os pendrives de exemplo do projeto rekordcrate. Só leitura: nada é gravado no pendrive.
//! O banco com cadeado dos aparelhos novos (`exportLibrary.db`) fica de fora; o rekordbox continua
//! gravando o `export.pdb` junto.
//!
//! O banco é feito de páginas (normalmente 4 KB). Cada tabela (músicas, artistas, tons, playlists)
//! é uma lista de páginas ligadas; no fim de cada página ficam grupos de 16 posições que apontam
//! para as linhas. Números do banco em little-endian; os da análise (ANLZ) em big-endian.

use crate::store::Saved;
use crate::track::Track;
use crate::HOTCUES;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Uma música do banco do rekordbox.
#[derive(Clone, Debug)]
pub struct RbTrack {
    pub id: u32,
    pub title: String,
    pub artist: String,
    /// Arquivo de áudio (caminho completo, já dentro do pendrive).
    pub path: PathBuf,
    /// 0 = sem BPM.
    pub bpm: f64,
    /// Tom como em `key::to_raw` (0 = desconhecido).
    pub key: u8,
    pub duration_secs: u32,
    /// Arquivo de análise (`ANLZ0000.DAT`), se houver.
    pub anlz: Option<PathBuf>,
}

/// Pasta ou playlist (na ordem do rekordbox).
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Folder { id: u32, name: String, children: Vec<Node> },
    Playlist { id: u32, name: String, tracks: Vec<u32> },
}

impl Node {
    pub fn name(&self) -> &str {
        match self {
            Node::Folder { name, .. } | Node::Playlist { name, .. } => name,
        }
    }
}

/// Tudo que o rekordbox exportou num pendrive.
#[derive(Debug, Default)]
pub struct Library {
    pub root: PathBuf,
    pub tracks: HashMap<u32, RbTrack>,
    pub playlists: Vec<Node>,
    by_path: HashMap<PathBuf, u32>,
}

impl Library {
    /// Lê o banco do pendrive em `root` (a pasta que tem `PIONEER`).
    pub fn open(root: &Path) -> Option<Library> {
        let bytes = std::fs::read(pdb_path(root)).ok()?;
        Self::parse(root, &bytes)
    }

    pub fn parse(root: &Path, bytes: &[u8]) -> Option<Library> {
        let db = Pdb::new(bytes)?;
        let strings = |table: u32, id_at: usize, name: fn(&Pdb, usize) -> Option<String>| -> HashMap<u32, String> {
            db.rows(table).into_iter().filter_map(|r| Some((db.u32(r + id_at)?, name(&db, r)?))).collect()
        };
        let artists = strings(T_ARTISTS, 4, |db, r| {
            // Nome logo depois (deslocamento de 1 byte) ou longe (2 bytes), conforme o subtipo.
            let at = if db.u16(r)? & 0x04 == 0 { *db.b.get(r + 9)? as usize } else { db.u16(r + 0x0A)? as usize };
            db.string(r + at)
        });
        let keys = strings(T_KEYS, 0, |db, r| db.string(r + 8));

        let mut lib = Library { root: root.to_path_buf(), ..Default::default() };
        for r in db.rows(T_TRACKS) {
            if let Some(t) = db.track(r, root, &artists, &keys) {
                lib.by_path.insert(t.path.clone(), t.id);
                lib.tracks.insert(t.id, t);
            }
        }
        lib.playlists = db.playlists(&lib.tracks);
        Some(lib)
    }

    /// Música do banco pelo caminho do arquivo.
    pub fn by_path(&self, path: &Path) -> Option<&RbTrack> {
        self.by_path.get(path).and_then(|id| self.tracks.get(id))
    }

    /// Pasta ou playlist pelo número.
    pub fn node(&self, id: u32) -> Option<&Node> {
        fn find(nodes: &[Node], id: u32) -> Option<&Node> {
            nodes.iter().find_map(|n| match n {
                Node::Folder { id: i, children, .. } => if *i == id { Some(n) } else { find(children, id) },
                Node::Playlist { id: i, .. } => (*i == id).then_some(n),
            })
        }
        find(&self.playlists, id)
    }
}

fn pdb_path(root: &Path) -> PathBuf {
    root.join("PIONEER").join("rekordbox").join("export.pdb")
}

/// O pendrive em `root` foi exportado pelo rekordbox?
pub fn is_export(root: &Path) -> bool {
    pdb_path(root).is_file()
}

/// Bancos já lidos (pela data do arquivo: exportou de novo, lê de novo).
static CACHE: Mutex<Vec<(PathBuf, SystemTime, Arc<Library>)>> = Mutex::new(Vec::new());

/// Banco do pendrive em `root`, lido uma vez só.
pub fn library(root: &Path) -> Option<Arc<Library>> {
    let mtime = std::fs::metadata(pdb_path(root)).and_then(|m| m.modified()).ok()?;
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, _, lib)) = cache.iter().find(|(r, m, _)| r == root && *m == mtime) {
        return Some(lib.clone());
    }
    let lib = Arc::new(Library::open(root)?);
    cache.retain(|(r, _, _)| r != root);
    cache.push((root.to_path_buf(), mtime, lib.clone()));
    Some(lib)
}

/// Pendrive do rekordbox que contém `path` (sobe pelas pastas até achar o `PIONEER`).
pub fn find_root(path: &Path) -> Option<PathBuf> {
    path.ancestors().take(12).find(|d| is_export(d)).map(Path::to_path_buf)
}

/// A música em `path`, se ela estiver no banco de um pendrive do rekordbox.
pub fn lookup(path: &Path) -> Option<RbTrack> {
    library(&find_root(path)?)?.by_path(path).cloned()
}

// ------------------------------------------------------------------ banco (export.pdb)

const T_TRACKS: u32 = 0;
const T_ARTISTS: u32 = 2;
const T_KEYS: u32 = 5;
const T_PLAYLIST_TREE: u32 = 7;
const T_PLAYLIST_ENTRIES: u32 = 8;

/// Textos da linha da música (posição na lista de deslocamentos).
const S_ANALYZE_PATH: usize = 14;
const S_TITLE: usize = 17;
const S_FILE_PATH: usize = 20;

struct Pdb<'a> {
    b: &'a [u8],
    page: usize,
    /// (tipo, primeira página, última página)
    tables: Vec<(u32, u32, u32)>,
}

impl<'a> Pdb<'a> {
    fn new(b: &'a [u8]) -> Option<Self> {
        let le = |o: usize| Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?));
        let page = le(4)? as usize;
        let n = le(8)? as usize;
        if !(512..=65536).contains(&page) || n > 64 || le(0)? != 0 {
            return None;
        }
        let tables = (0..n).map(|i| Some((le(0x1C + 16 * i)?, le(0x1C + 16 * i + 8)?, le(0x1C + 16 * i + 12)?))).collect::<Option<_>>()?;
        Some(Pdb { b, page, tables })
    }

    fn u16(&self, o: usize) -> Option<u16> {
        Some(u16::from_le_bytes(self.b.get(o..o + 2)?.try_into().ok()?))
    }

    fn u32(&self, o: usize) -> Option<u32> {
        Some(u32::from_le_bytes(self.b.get(o..o + 4)?.try_into().ok()?))
    }

    /// Posição (no arquivo) de cada linha da tabela.
    fn rows(&self, table: u32) -> Vec<usize> {
        let mut out = Vec::new();
        let max_pages = self.b.len() / self.page;
        for &(_, first, last) in self.tables.iter().filter(|t| t.0 == table) {
            let mut p = first as usize;
            // Cada página só uma vez (um banco estragado não prende num círculo).
            for _ in 0..max_pages {
                let off = p * self.page;
                if off + self.page > self.b.len() {
                    break;
                }
                let (Some(kind), Some(next)) = (self.u32(off + 8), self.u32(off + 12)) else { break };
                let num_rows = (self.u32(off + 0x18).unwrap_or(0) & 0x1FFF) as usize;
                let index_page = self.b[off + 0x1B] & 0x40 != 0;
                if kind == table && !index_page {
                    let end = off + self.page;
                    for g in 0..num_rows.div_ceil(16) {
                        let base = end - g * 36;
                        let present = self.u16(base - 4).unwrap_or(0);
                        for i in 0..16 {
                            if present >> i & 1 == 1 {
                                if let Some(ro) = self.u16(base - 6 - 2 * i) {
                                    let at = off + 0x28 + ro as usize;
                                    if at < end {
                                        out.push(at);
                                    }
                                }
                            }
                        }
                    }
                }
                if p as u32 == last {
                    break;
                }
                p = next as usize;
            }
        }
        out
    }

    /// Texto no formato do banco: curto (ASCII, tamanho no 1º byte), longo ASCII (0x40) ou
    /// longo UTF-16 (0x90).
    fn string(&self, at: usize) -> Option<String> {
        let h = *self.b.get(at)?;
        if h & 1 == 1 {
            let n = (h >> 1) as usize;
            return Some(String::from_utf8_lossy(self.b.get(at + 1..at + n.max(1))?).into_owned());
        }
        let len = (self.u16(at + 1)? as usize).checked_sub(4)?;
        let body = self.b.get(at + 4..at + 4 + len)?;
        match h {
            0x40 => Some(String::from_utf8_lossy(body).into_owned()),
            0x90 => Some(String::from_utf16_lossy(&body.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())),
            _ => None,
        }
    }

    fn track(&self, r: usize, root: &Path, artists: &HashMap<u32, String>, keys: &HashMap<u32, String>) -> Option<RbTrack> {
        // Deslocamentos dos textos em 16 bits (subtipo 0x24, o único que o rekordbox grava).
        if self.u16(r)? & 0x04 == 0 {
            return None;
        }
        let text = |i: usize| self.u16(r + 0x5E + 2 * i).and_then(|o| self.string(r + o as usize)).unwrap_or_default();
        let file = text(S_FILE_PATH);
        if file.is_empty() {
            return None;
        }
        let in_drive = |p: &str| root.join(p.trim_start_matches('/'));
        let analyze = text(S_ANALYZE_PATH);
        let path = in_drive(&file);
        let mut title = text(S_TITLE);
        if title.is_empty() {
            title = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        }
        Some(RbTrack {
            id: self.u32(r + 0x48)?,
            title,
            artist: artists.get(&self.u32(r + 0x44)?).cloned().unwrap_or_default(),
            path,
            bpm: self.u32(r + 0x38)? as f64 / 100.0,
            key: keys.get(&self.u32(r + 0x20)?).map_or(crate::key::RAW_UNKNOWN, |k| key_from_name(k)),
            duration_secs: self.u16(r + 0x54)? as u32,
            anlz: (!analyze.is_empty()).then(|| in_drive(&analyze)),
        })
    }

    fn playlists(&self, tracks: &HashMap<u32, RbTrack>) -> Vec<Node> {
        // (pai, ordem, número, é pasta, nome)
        let tree: Vec<(u32, u32, u32, bool, String)> = self
            .rows(T_PLAYLIST_TREE)
            .into_iter()
            .filter_map(|r| Some((self.u32(r)?, self.u32(r + 8)?, self.u32(r + 12)?, self.u32(r + 16)? != 0, self.string(r + 0x14)?)))
            .collect();
        let mut entries: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
        for r in self.rows(T_PLAYLIST_ENTRIES) {
            if let (Some(i), Some(t), Some(p)) = (self.u32(r), self.u32(r + 4), self.u32(r + 8)) {
                if tracks.contains_key(&t) {
                    entries.entry(p).or_default().push((i, t));
                }
            }
        }
        fn children(tree: &[(u32, u32, u32, bool, String)], entries: &mut HashMap<u32, Vec<(u32, u32)>>, parent: u32, depth: usize) -> Vec<Node> {
            let mut kids: Vec<_> = tree.iter().filter(|n| n.0 == parent && n.2 != parent).collect();
            kids.sort_by_key(|n| n.1);
            kids.into_iter()
                .map(|(_, _, id, folder, name)| {
                    if *folder {
                        let children = if depth < 16 { children(tree, entries, *id, depth + 1) } else { Vec::new() };
                        Node::Folder { id: *id, name: name.clone(), children }
                    } else {
                        let mut e = entries.remove(id).unwrap_or_default();
                        e.sort_by_key(|x| x.0);
                        Node::Playlist { id: *id, name: name.clone(), tracks: e.into_iter().map(|x| x.1).collect() }
                    }
                })
                .collect()
        }
        children(&tree, &mut entries, 0, 0)
    }
}

/// Tom pelo nome que o rekordbox grava: "Am", "C#m", "Emin", "Fmaj", "Db", Camelot ("8A") ou
/// Open Key ("1d" maior, "1m" menor).
pub fn key_from_name(name: &str) -> u8 {
    let s = name.trim();
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if let (Ok(n), Some(l)) = (digits.parse::<u32>(), s[digits.len()..].chars().next()) {
        if (1..=12).contains(&n) && s.len() == digits.len() + 1 {
            let key = match l.to_ascii_uppercase() {
                // Camelot: procura na roda (A = menor, B = maior).
                'A' | 'B' => (0..24u8).find(|&k| crate::key::camelot(k) == (n as u8, l.to_ascii_uppercase())),
                // Open Key: 1d = Dó maior, 1m = Lá menor, cada número sobe uma quinta.
                'D' => Some(((7 * (n - 1)) % 12) as u8),
                'M' => Some(12 + ((9 + 7 * (n - 1)) % 12) as u8),
                _ => None,
            };
            return crate::key::to_raw(key);
        }
    }
    let mut c = s.chars();
    let Some(tonic) = c.next().and_then(|n| match n.to_ascii_uppercase() {
        'C' => Some(0i32),
        'D' => Some(2),
        'E' => Some(4),
        'F' => Some(5),
        'G' => Some(7),
        'A' => Some(9),
        'B' => Some(11),
        _ => None,
    }) else {
        return crate::key::RAW_UNKNOWN;
    };
    let rest = c.as_str();
    let (shift, mode) = match rest.chars().next() {
        Some(a @ ('#' | '♯')) => (1, &rest[a.len_utf8()..]),
        Some(a @ ('b' | '♭')) => (-1, &rest[a.len_utf8()..]),
        _ => (0, rest),
    };
    let minor = matches!(mode.to_ascii_lowercase().as_str(), "m" | "min" | "minor");
    let tonic = (tonic + shift).rem_euclid(12) as u8;
    crate::key::to_raw(Some(if minor { tonic + 12 } else { tonic }))
}

// ------------------------------------------------------------------ análise (ANLZ)

/// Grade e cues de uma música, em milissegundos.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analysis {
    /// 0 = sem grade.
    pub bpm: f64,
    /// Primeira batida "1" do compasso.
    pub first_beat_ms: f64,
    /// Memory cue (o primeiro da música).
    pub cue_ms: Option<f64>,
    pub hotcues_ms: [Option<f64>; HOTCUES],
    /// Primeiro memory cue com loop: entrada e saída.
    pub loop_ms: Option<(f64, f64)>,
}

/// Lê a grade e os cues do `ANLZ0000.DAT` e do `.EXT` ao lado (os cues coloridos ficam nele).
pub fn analysis(dat: &Path) -> Option<Analysis> {
    let mut a = Analysis::default();
    let mut ok = false;
    let mut extended = false;
    for (file, is_ext) in [(dat.to_path_buf(), false), (dat.with_extension("EXT"), true)] {
        let Ok(b) = std::fs::read(&file) else { continue };
        let found = parse_anlz(&b, &mut a, is_ext, &mut extended);
        ok |= found;
    }
    ok.then_some(a)
}

fn be16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn be32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// Uma parte da análise: (tipo, tamanho do cabeçalho, tamanho total).
fn section(b: &[u8], o: usize) -> Option<(&[u8], usize, usize)> {
    Some((b.get(o..o + 4)?, be32(b, o + 4)? as usize, be32(b, o + 8)? as usize))
}

/// Junta em `a` o que achar no arquivo. Os cues do `.EXT` (PCO2) valem mais que os do `.DAT`
/// (PCOB, formato antigo). Devolve se o arquivo era uma análise.
fn parse_anlz(b: &[u8], a: &mut Analysis, is_ext: bool, extended: &mut bool) -> bool {
    let Some((b"PMAI", head, _)) = section(b, 0) else { return false };
    let mut o = head;
    while let Some((tag, head, total)) = section(b, o) {
        if total < 12 || o + total > b.len() {
            break;
        }
        let s = &b[o..o + total];
        match tag {
            b"PQTZ" if a.bpm == 0.0 => grid(s, a),
            b"PCO2" => {
                if !*extended {
                    *extended = true;
                    a.cue_ms = None;
                    a.hotcues_ms = [None; HOTCUES];
                    a.loop_ms = None;
                }
                cues(s, head, true, a);
            }
            b"PCOB" if !*extended && !is_ext => cues(s, head, false, a),
            _ => {}
        }
        o += total;
    }
    true
}

fn grid(s: &[u8], a: &mut Analysis) {
    let n = be32(s, 20).unwrap_or(0) as usize;
    // (número no compasso, BPM × 100, ms)
    let beats: Vec<(u16, u16, u32)> = (0..n).map_while(|i| Some((be16(s, 24 + 8 * i)?, be16(s, 26 + 8 * i)?, be32(s, 28 + 8 * i)?))).collect();
    let Some(first) = beats.iter().find(|b| b.0 == 1) else { return };
    let (Some(f), Some(l)) = (beats.first(), beats.last()) else { return };
    // BPM fixo: o do rekordbox; variável: a média (a grade do DJ OS é de BPM fixo).
    a.bpm = if beats.iter().all(|b| b.1 == f.1) || beats.len() < 2 || l.2 <= f.2 {
        f.1 as f64 / 100.0
    } else {
        60_000.0 * (beats.len() - 1) as f64 / (l.2 - f.2) as f64
    };
    a.first_beat_ms = first.2 as f64;
}

fn cues(s: &[u8], head: usize, extended: bool, a: &mut Analysis) {
    let count = if extended { be16(s, 16) } else { be16(s, 18) }.unwrap_or(0) as usize;
    let mut o = head;
    for _ in 0..count {
        let Some((_, _, total)) = section(s, o) else { break };
        let (hot, kind, time, loop_end) = if extended {
            (be32(s, o + 12), s.get(o + 16).copied(), be32(s, o + 20), be32(s, o + 24))
        } else {
            (be32(s, o + 12), s.get(o + 28).copied(), be32(s, o + 32), be32(s, o + 36))
        };
        if let (Some(hot), Some(kind), Some(time)) = (hot, kind, time) {
            let ms = time as f64;
            match hot as usize {
                h @ 1..=HOTCUES => a.hotcues_ms[h - 1] = Some(ms),
                0 => {
                    if a.cue_ms.is_none_or(|c| ms < c) {
                        a.cue_ms = Some(ms);
                    }
                    if kind == 2 && a.loop_ms.is_none() {
                        if let Some(e) = loop_end.filter(|&e| e as f64 > ms) {
                            a.loop_ms = Some((ms, e as f64));
                        }
                    }
                }
                _ => {}
            }
        }
        if total < 12 {
            break;
        }
        o += total;
    }
}

// ------------------------------------------------------------------ primeira análise

/// Primeira vez que uma música abre: se ela veio de um pendrive do rekordbox, a grade, o tom e
/// os cues são os dele (só a waveform e o volume são calculados aqui). `None` = não é do rekordbox.
pub fn load_track(path: &Path) -> Option<Result<(Track, Saved), String>> {
    let rb = lookup(path)?;
    let an = rb.anlz.as_deref().and_then(analysis);
    Some(crate::track::load(path, false).map(|mut t| {
        let (bpm, first_ms) = match &an {
            Some(a) if a.bpm > 0.0 => (a.bpm, a.first_beat_ms),
            _ => (rb.bpm, 0.0),
        };
        let frames = |ms: f64| ms / 1000.0 * t.sr;
        t.set_grid(bpm, frames(first_ms));
        t.key = if crate::key::from_raw(rb.key).is_some() { rb.key } else { crate::key::to_raw(crate::key::detect(&t.data, t.sr)) };
        t.loudness = crate::loudness::measure(&t.data, t.sr).unwrap_or(f32::NEG_INFINITY);
        if !rb.title.is_empty() {
            t.title = rb.title.clone();
        }
        if !rb.artist.is_empty() {
            t.artist = rb.artist.clone();
        }
        let mut s = Saved::from_track(&t);
        if let Some(a) = an {
            s.cue = a.cue_ms.map(frames);
            s.hotcues = a.hotcues_ms.map(|h| h.map(frames));
            s.loop_ = a.loop_ms.map(|(x, y)| (frames(x), frames(y), (y - x) / 60_000.0 * bpm));
        }
        (t, s)
    }))
}

/// Primeira análise de uma música: a do rekordbox (se ela veio de um pendrive exportado por ele)
/// ou a do DJ OS.
pub fn first_analysis(path: &Path) -> Result<(Track, Saved), String> {
    load_track(path).unwrap_or_else(|| {
        crate::track::load(path, true).map(|t| {
            let s = Saved::from_track(&t);
            (t, s)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/rekordbox/demo")
    }

    #[test]
    fn le_as_musicas_do_pendrive_de_exemplo() {
        let lib = Library::open(&demo()).expect("banco de exemplo");
        assert_eq!(lib.tracks.len(), 2);
        let t = &lib.tracks[&1];
        assert_eq!((t.title.as_str(), t.artist.as_str()), ("Demo Track 1", "Loopmasters"));
        assert_eq!(t.bpm, 128.0);
        assert_eq!(t.duration_secs, 172);
        assert_eq!(crate::key::from_raw(t.key), Some(5 + 12), "Fm = Fá menor");
        assert_eq!(t.path, demo().join("Contents/Loopmasters/UnknownAlbum/Demo Track 1.mp3"));
        assert_eq!(lib.by_path(&t.path).map(|x| x.id), Some(1));
        assert!(lib.playlists.is_empty());
    }

    #[test]
    fn le_a_grade_da_analise() {
        let lib = Library::open(&demo()).unwrap();
        let a = analysis(lib.tracks[&1].anlz.as_ref().unwrap()).expect("análise");
        assert_eq!(a.bpm, 128.0);
        assert_eq!(a.first_beat_ms, 25.0);
        let a = analysis(lib.tracks[&2].anlz.as_ref().unwrap()).unwrap();
        assert_eq!((a.bpm, a.first_beat_ms), (120.0, 25.0));
        assert_eq!(a.hotcues_ms, [None; HOTCUES], "o exemplo não tem cues");
    }

    #[test]
    fn le_os_cues() {
        // Seção PCO2 montada à mão: hot cue B (2) em 1,5 s e memory cue com loop de 2 s a 4 s.
        let cue = |hot: u32, kind: u8, ms: u32, end: u32| {
            let mut c = b"PCP2".to_vec();
            c.extend(0x10u32.to_be_bytes());
            c.extend(0x30u32.to_be_bytes());
            c.extend(hot.to_be_bytes());
            c.extend([kind, 0, 0, 0]);
            c.extend(ms.to_be_bytes());
            c.extend(end.to_be_bytes());
            c.resize(0x30, 0);
            c
        };
        let body = [cue(2, 1, 1500, u32::MAX), cue(0, 2, 2000, 4000)].concat();
        let mut s = b"PCO2".to_vec();
        s.extend(20u32.to_be_bytes());
        s.extend((20 + body.len() as u32).to_be_bytes());
        s.extend(1u32.to_be_bytes());
        s.extend(2u16.to_be_bytes());
        s.extend(0u16.to_be_bytes());
        s.extend(body);
        let mut f = b"PMAI".to_vec();
        f.extend(28u32.to_be_bytes());
        f.extend((28 + s.len() as u32).to_be_bytes());
        f.resize(28, 0);
        f.extend(s);
        let mut a = Analysis::default();
        assert!(parse_anlz(&f, &mut a, true, &mut false));
        assert_eq!(a.hotcues_ms[1], Some(1500.0));
        assert_eq!(a.cue_ms, Some(2000.0));
        assert_eq!(a.loop_ms, Some((2000.0, 4000.0)));
    }

    #[test]
    fn le_paginas_de_playlist() {
        // Páginas soltas de um banco de verdade (exemplos do rekordcrate).
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/rekordbox");
        let page = |name: &str, kind: u32| {
            let p = std::fs::read(dir.join(name)).unwrap();
            // Banco com uma tabela só, apontando para a página (página 1).
            let mut b = vec![0u8; p.len()];
            b[4..8].copy_from_slice(&(p.len() as u32).to_le_bytes());
            b[8..12].copy_from_slice(&1u32.to_le_bytes());
            b[0x1C..0x20].copy_from_slice(&kind.to_le_bytes());
            b[0x24..0x28].copy_from_slice(&1u32.to_le_bytes());
            b[0x28..0x2C].copy_from_slice(&1u32.to_le_bytes());
            let mut p = p;
            p[4..8].copy_from_slice(&1u32.to_le_bytes());
            b.extend(p);
            b
        };
        let tree = page("playlist_tree_page.bin", T_PLAYLIST_TREE);
        let db = Pdb::new(&tree).unwrap();
        let names: Vec<String> = db.rows(T_PLAYLIST_TREE).into_iter().filter_map(|r| db.string(r + 0x14)).collect();
        assert!(!names.is_empty() && names.iter().all(|n| !n.is_empty()), "{names:?}");
        let entries = page("playlist_entries_page.bin", T_PLAYLIST_ENTRIES);
        let db = Pdb::new(&entries).unwrap();
        assert!(!db.rows(T_PLAYLIST_ENTRIES).is_empty());
    }

    #[test]
    fn musica_do_pendrive_abre_com_a_grade_do_rekordbox() {
        // Pendrive de exemplo com um WAV de 3 s no lugar da "Demo Track 1".
        let root = std::env::temp_dir().join(format!("dj-os-rb-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for f in ["rekordbox/export.pdb", "USBANLZ/P016/0000875E/ANLZ0000.DAT"] {
            let to = root.join("PIONEER").join(f);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(demo().join("PIONEER").join(f), &to).unwrap();
        }
        let wav = root.join("Contents/Loopmasters/UnknownAlbum/Demo Track 1.mp3".replace(".mp3", ".wav"));
        std::fs::create_dir_all(wav.parent().unwrap()).unwrap();
        let (sr, n) = (44100u32, 44100u32 * 3);
        let mut b = Vec::new();
        b.extend(b"RIFF");
        b.extend((36 + n * 4).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend([1u8, 0, 2, 0]);
        b.extend(sr.to_le_bytes());
        b.extend((sr * 4).to_le_bytes());
        b.extend([4u8, 0, 16, 0]);
        b.extend(b"data");
        b.extend((n * 4).to_le_bytes());
        for i in 0..n {
            let v = ((i as f32 * 0.05).sin() * 8000.0) as i16;
            b.extend(v.to_le_bytes());
            b.extend(v.to_le_bytes());
        }
        // O banco aponta para o .mp3: grava o WAV com esse nome (o decodificador olha o conteúdo).
        let mp3 = wav.with_extension("mp3");
        std::fs::write(&mp3, &b).unwrap();
        let (t, s) = load_track(&mp3).expect("é do rekordbox").expect("abre");
        assert_eq!(t.bpm(), 128.0);
        assert!((t.first_beat() - 0.025 * 44100.0).abs() < 1e-6, "primeira batida aos 25 ms");
        assert_eq!(crate::key::from_raw(t.key), Some(5 + 12), "tom do rekordbox (Fm)");
        assert_eq!(t.title, "Demo Track 1");
        assert_eq!(s.bpm, 128.0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn nomes_de_tom() {
        let k = |s: &str| crate::key::from_raw(key_from_name(s));
        assert_eq!(k("Am"), Some(9 + 12));
        assert_eq!(k("Amin"), Some(9 + 12));
        assert_eq!(k("C"), Some(0));
        assert_eq!(k("Cmaj"), Some(0));
        assert_eq!(k("F#m"), Some(6 + 12));
        assert_eq!(k("Db"), Some(1));
        assert_eq!(k("Ebmin"), Some(3 + 12));
        assert_eq!(k("8A"), Some(9 + 12), "Camelot 8A = Lá menor");
        assert_eq!(k("8B"), Some(0), "Camelot 8B = Dó maior");
        assert_eq!(k("1d"), Some(0), "Open Key 1d = Dó maior");
        assert_eq!(k("1m"), Some(9 + 12), "Open Key 1m = Lá menor");
        assert_eq!(k("2d"), Some(7), "Open Key 2d = Sol maior");
        assert_eq!(k(""), None);
    }
}

/// Banco grande de verdade (fora do repositório): `DJOS_RB_PDB=<export.pdb> cargo test -- --ignored`.
#[cfg(test)]
#[test]
#[ignore]
fn banco_grande() {
    let p = std::env::var("DJOS_RB_PDB").expect("DJOS_RB_PDB");
    let t0 = std::time::Instant::now();
    let lib = Library::parse(Path::new("/x"), &std::fs::read(p).unwrap()).unwrap();
    fn count(n: &[Node]) -> (usize, usize, usize) {
        n.iter().fold((0, 0, 0), |(f, p, e), n| match n {
            Node::Folder { children, .. } => {
                let (a, b, c) = count(children);
                (f + 1 + a, p + b, e + c)
            }
            Node::Playlist { tracks, .. } => (f, p + 1, e + tracks.len()),
        })
    }
    let (f, p, e) = count(&lib.playlists);
    let sem_tom = lib.tracks.values().filter(|t| crate::key::from_raw(t.key).is_none()).count();
    eprintln!("{} músicas, {f} pastas, {p} playlists, {e} entradas, {sem_tom} sem tom, {:?}", lib.tracks.len(), t0.elapsed());
    for n in lib.playlists.iter().take(3) {
        eprintln!("  {}", n.name());
    }
    assert!(lib.tracks.len() > 3000 && p > 50);
}
