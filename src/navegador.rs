//! Browser novo em tela cheia (estilo cabine): o que ele mostra e como se anda nele. O desenho fica
//! em `ui/principal/navegador.rs`; aqui só a lógica, para dar para testar sem tela.
//!
//! Três áreas: a barra de cima (dispositivo, voltar, preview, tamanho da letra), as categorias à
//! esquerda e a lista. Girar o BROWSE anda na área; subir além da primeira linha chega na barra de
//! cima, onde os pads andam para os lados (como no teclado da pesquisa) e descer volta para a
//! lista. Apertar o BROWSE (ou o LOAD) abre a pasta; "← Voltar" sobe um nível e, no começo da
//! categoria, vai para as categorias.

use crate::app::AppState;
use crate::library::{Library, Node, NodeKind, TrackEntry};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Categorias da coluna da esquerda.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cat {
    Artista,
    Album,
    Musicas,
    Tom,
    Playlist,
    Historico,
    Combinam,
    Pastas,
    Fila,
    Gravacoes,
}

impl Cat {
    pub const ALL: [Cat; 10] =
        [Cat::Artista, Cat::Album, Cat::Musicas, Cat::Tom, Cat::Playlist, Cat::Historico, Cat::Combinam, Cat::Pastas, Cat::Fila, Cat::Gravacoes];

    pub fn icon(self) -> &'static str {
        match self {
            Cat::Artista => "👤",
            Cat::Album => "💿",
            Cat::Musicas => "♪",
            Cat::Tom => "b#",
            Cat::Playlist => "☰",
            Cat::Historico => "🕘",
            Cat::Combinam => "🔗",
            Cat::Pastas => "📁",
            Cat::Fila => "⏭",
            Cat::Gravacoes => "⏺",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Cat::Artista => "ARTISTA",
            Cat::Album => "ÁLBUM",
            Cat::Musicas => "MÚSICAS",
            Cat::Tom => "TOM",
            Cat::Playlist => "PLAYLIST",
            Cat::Historico => "HISTÓRICO",
            Cat::Combinam => "COMBINAM",
            Cat::Pastas => "PASTAS",
            Cat::Fila => "FILA",
            Cat::Gravacoes => "GRAVAÇÕES",
        }
    }

    fn index(self) -> usize {
        Cat::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }
}

/// Onde o BROWSE está agindo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Area {
    Top,
    Cats,
    List,
}

/// Botões da barra de cima, da esquerda para a direita.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TopButton {
    Device,
    Back,
    Preview,
    Font,
}

impl TopButton {
    pub const ALL: [TopButton; 4] = [TopButton::Device, TopButton::Back, TopButton::Preview, TopButton::Font];
}

/// Uma linha da lista.
#[derive(Clone, Debug)]
pub enum Item {
    Back,
    /// Na escolha do dispositivo: `None` = todos.
    Device(Option<PathBuf>, String),
    Folder(Node),
    Artist(String, Vec<TrackEntry>),
    /// Histórico de uma noite (playlist M3U8).
    Night(PathBuf, String),
    Track(TrackEntry),
    Info(String),
}

impl Item {
    /// Apertar abre alguma coisa (pasta, artista, voltar...)?
    pub fn opens(&self) -> bool {
        !matches!(self, Item::Track(_) | Item::Info(_))
    }
}

/// Um nível aberto: a raiz da categoria ou uma pasta/artista/playlist aberta.
#[derive(Clone, Debug)]
pub struct Level {
    pub title: String,
    pub items: Vec<Item>,
    pub sel: usize,
    /// Primeira linha que aparece na tela (a lista só rola quando a seleção sai dela).
    pub first: usize,
    /// O item que abriu este nível (para montar de novo quando algo muda).
    from: Option<Item>,
}

pub struct Nav {
    pub area: Area,
    pub cat: Cat,
    /// Dispositivo escolhido (`None` = todos).
    pub device: Option<PathBuf>,
    pub levels: Vec<Level>,
    pub top: TopButton,
    /// Como estava quando a lista foi montada (para montar de novo se mudar).
    generation: u64,
    scanning: bool,
    live: (usize, usize),
    search: Option<(String, usize)>,
}

impl Default for Nav {
    fn default() -> Self {
        Self { area: Area::List, cat: Cat::Pastas, device: None, levels: Vec::new(), top: TopButton::Device, generation: 0, scanning: false, live: (0, 0), search: None }
    }
}

/// Pasta de uma unidade (pelo caminho), como item da árvore.
fn source_node(lib_sources: &[Node], path: &Path) -> Node {
    lib_sources.iter().find(|n| n.path == path).cloned().unwrap_or_else(|| Node {
        name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        path: path.to_path_buf(),
        depth: 0,
        kind: NodeKind::Source,
        expanded: false,
        has_children: true,
    })
}

/// Subpastas e músicas de uma pasta, numa lista só (pastas em cima).
fn folder_items(lib: &Library, n: &Node) -> Vec<Item> {
    Library::children(n).into_iter().map(Item::Folder).chain(lib.node_tracks(n).into_iter().map(Item::Track)).collect()
}

/// Músicas de uma playlist M3U8 (as linhas que não são comentário), as que ainda existem.
fn read_m3u(path: &Path) -> Vec<TrackEntry> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(Path::new).filter(|p| p.exists()).map(TrackEntry::from_path).collect()
}

/// Noites anteriores no histórico (a mais recente em cima), menos a de hoje.
fn past_nights(current: Option<&Path>) -> Vec<Item> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(crate::history::dir())
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "m3u8") && Some(p.as_path()) != current).collect())
        .unwrap_or_default();
    v.sort();
    v.into_iter()
        .rev()
        .map(|p| {
            // 2026-10-31-2215.m3u8 → "31/10/2026 22:15"
            let s = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let parts: Vec<&str> = s.split('-').collect();
            let name = match parts.as_slice() {
                [y, m, d, hm] if hm.len() == 4 => format!("🕘 {d}/{m}/{y} {}:{}", &hm[..2], &hm[2..]),
                _ => format!("🕘 {s}"),
            };
            Item::Night(p, name)
        })
        .collect()
}

impl Level {
    /// Acerta a primeira linha da tela para a seleção aparecer (com `rows` linhas cabendo).
    pub fn scroll_into_view(&mut self, rows: usize) {
        let n = self.items.len();
        if self.sel < self.first {
            self.first = self.sel;
        } else if self.sel >= self.first + rows {
            self.first = self.sel + 1 - rows;
        }
        self.first = self.first.min(n.saturating_sub(rows));
    }
}

impl Nav {
    pub fn level(&self) -> Option<&Level> {
        self.levels.last()
    }

    pub fn selected(&self) -> Option<&Item> {
        let l = self.levels.last()?;
        l.items.get(l.sel)
    }

    pub fn selected_track(&self) -> Option<TrackEntry> {
        match self.selected()? {
            Item::Track(t) => Some(t.clone()),
            _ => None,
        }
    }

    /// Pasta de verdade ou música selecionada (para copiar para as músicas internas ou apagar).
    pub fn selected_path(&self) -> Option<PathBuf> {
        match self.selected()? {
            Item::Folder(n) if !n.kind.is_rekordbox() => Some(n.path.clone()),
            Item::Track(t) => Some(t.path.clone()),
            _ => None,
        }
    }

    /// "PASTAS › Pendrive › Funk".
    pub fn breadcrumb(&self) -> String {
        let mut parts = vec![self.cat.name().to_string()];
        parts.extend(self.levels.iter().skip(1).filter(|l| !l.title.is_empty()).map(|l| l.title.clone()));
        parts.join("  ›  ")
    }

    /// Quantas músicas tem o nível aberto.
    pub fn track_count(&self) -> usize {
        self.level().map_or(0, |l| l.items.iter().filter(|i| matches!(i, Item::Track(_))).count())
    }

    /// Chamado a cada quadro com o browser aberto: monta a lista na primeira vez e de novo quando
    /// as unidades são relidas (pendrive entrou ou saiu, cópia terminou), a leitura das músicas termina, o histórico ou a fila mudam, ou
    /// uma pesquisa termina (os resultados abrem como um nível).
    pub fn tick(&mut self, lib: &Library) {
        let scanning = lib.index.lock().scanning;
        if self.generation != lib.generation {
            self.generation = lib.generation;
            if self.device.as_ref().is_some_and(|d| !lib.sources_sig().contains(d)) {
                self.device = None;
            }
            self.refresh(lib);
        }
        if self.scanning && !scanning && matches!(self.cat, Cat::Musicas | Cat::Artista) {
            self.refresh(lib);
        }
        self.scanning = scanning;
        let live = (lib.history.len(), lib.prepare.len());
        if live != self.live {
            self.live = live;
            if matches!(self.cat, Cat::Historico | Cat::Fila) {
                self.refresh(lib);
            }
        }
        let search = lib.results.as_ref().map(|(q, r)| (q.clone(), r.len()));
        if search != self.search {
            if let Some((q, r)) = lib.results.as_ref().filter(|_| self.search.is_some() || !self.levels.is_empty()) {
                self.push(format!("🔍 {q}"), r.iter().cloned().map(Item::Track).collect(), None);
                self.area = Area::List;
            }
            self.search = search;
        }
        if self.levels.is_empty() {
            self.levels.push(self.root(lib));
        }
    }

    /// Monta tudo de novo, mantendo o caminho aberto e a seleção (o que der).
    pub fn refresh(&mut self, lib: &Library) {
        let old = std::mem::take(&mut self.levels);
        let mut root = self.root(lib);
        if let Some(l) = old.first() {
            root.sel = l.sel.min(root.items.len().saturating_sub(1));
        }
        self.levels.push(root);
        for l in old.into_iter().skip(1) {
            let items = match &l.from {
                Some(Item::Folder(n)) if !n.path.exists() => break,
                Some(from) => self.items_of(lib, from).unwrap_or(l.items[1..].to_vec()),
                None => l.items[1..].to_vec(),
            };
            self.push(l.title, items, l.from);
            if let Some(last) = self.levels.last_mut() {
                last.sel = l.sel.min(last.items.len().saturating_sub(1));
            }
        }
    }

    /// O que tem dentro de um item que abre.
    fn items_of(&self, lib: &Library, item: &Item) -> Option<Vec<Item>> {
        Some(match item {
            Item::Folder(n) => folder_items(lib, n),
            Item::Artist(_, tracks) => tracks.iter().cloned().map(Item::Track).collect(),
            Item::Night(p, _) => read_m3u(p).into_iter().map(Item::Track).collect(),
            _ => return None,
        })
    }

    /// Músicas de todas as unidades (do índice da pesquisa), só as do dispositivo escolhido.
    fn indexed(&self, lib: &Library) -> Vec<TrackEntry> {
        let ix = lib.index.lock();
        ix.entries.iter().map(|(_, t)| t).filter(|t| self.device.as_ref().is_none_or(|d| t.path.starts_with(d))).cloned().collect()
    }

    /// Lista do começo da categoria.
    fn root(&self, lib: &Library) -> Level {
        let scanning = lib.index.lock().scanning;
        let sources = Library::source_nodes();
        let mut items: Vec<Item> = match self.cat {
            Cat::Pastas => match &self.device {
                None => sources.into_iter().map(Item::Folder).collect(),
                Some(d) => folder_items(lib, &source_node(&sources, d)),
            },
            Cat::Musicas => {
                let mut v = self.indexed(lib);
                v.sort_by_cached_key(|t| (t.title.to_lowercase(), t.artist.to_lowercase()));
                v.into_iter().map(Item::Track).collect()
            }
            Cat::Artista => {
                let mut by: BTreeMap<String, (String, Vec<TrackEntry>)> = BTreeMap::new();
                for t in self.indexed(lib) {
                    let name = if t.artist.trim().is_empty() { "(sem artista)".to_string() } else { t.artist.trim().to_string() };
                    by.entry(crate::library::normalize(&name)).or_insert_with(|| (name, Vec::new())).1.push(t);
                }
                by.into_values()
                    .map(|(name, mut tracks)| {
                        tracks.sort_by_cached_key(|t| t.title.to_lowercase());
                        Item::Artist(name, tracks)
                    })
                    .collect()
            }
            Cat::Playlist => {
                let rb: Vec<Node> = sources
                    .iter()
                    .filter(|n| self.device.as_ref().is_none_or(|d| n.path == *d) && crate::rekordbox::is_export(&n.path))
                    .map(|n| Node {
                        name: format!("📀 {}", n.name.trim_start_matches(|c: char| !c.is_alphanumeric())),
                        path: n.path.clone(),
                        depth: 0,
                        kind: NodeKind::RbRoot,
                        expanded: false,
                        has_children: true,
                    })
                    .collect();
                // Um pendrive só: as playlists dele direto.
                match rb.as_slice() {
                    [one] => Library::children(one).into_iter().map(Item::Folder).collect(),
                    _ => rb.into_iter().map(Item::Folder).collect(),
                }
            }
            Cat::Historico => lib.history.iter().rev().cloned().map(Item::Track).chain(past_nights(None)).collect(),
            Cat::Fila => lib.prepare.iter().cloned().map(Item::Track).collect(),
            Cat::Gravacoes => {
                let dir = crate::record::dir();
                let n = Node { name: "Gravações".into(), path: dir, depth: 0, kind: NodeKind::Dir, expanded: false, has_children: true };
                folder_items(lib, &n)
            }
            Cat::Album => vec![Item::Info("Em breve: o álbum vem da etiqueta da música (ID3), que o DJ OS ainda não lê".into())],
            Cat::Tom => vec![Item::Info("Em breve: as músicas separadas pelo tom".into())],
            Cat::Combinam => vec![Item::Info("Em breve: as músicas que combinam com o tom da que está tocando".into())],
        };
        if items.is_empty() {
            let msg = match self.cat {
                Cat::Musicas | Cat::Artista if scanning => "Lendo as músicas dos pendrives…",
                Cat::Playlist => "Nenhuma playlist: um pendrive exportado pelo rekordbox mostra as playlists dele aqui",
                Cat::Historico => "Nenhuma música tocada ainda",
                Cat::Fila => "Fila vazia: SHIFT + LOAD numa música coloca ela aqui",
                Cat::Gravacoes => "Nenhuma gravação ainda (botão GRAVAR ou Ctrl+R)",
                _ => "Nenhuma música aqui",
            };
            items.push(Item::Info(msg.into()));
        }
        Level { title: String::new(), items, sel: 0, first: 0, from: None }
    }

    /// Abre um nível novo, com "← Voltar" em cima e a primeira coisa de verdade selecionada.
    fn push(&mut self, title: String, items: Vec<Item>, from: Option<Item>) {
        let mut v = vec![Item::Back];
        if items.is_empty() {
            v.push(Item::Info("Nada aqui".into()));
        }
        v.extend(items);
        self.levels.push(Level { title, items: v, sel: 1, first: 0, from });
    }

    /// Volta um nível; no começo da categoria, vai para as categorias.
    pub fn back(&mut self) {
        if self.levels.len() > 1 {
            self.levels.pop();
            self.area = Area::List;
        } else {
            self.area = Area::Cats;
        }
    }

    /// Mostra uma categoria (do começo).
    pub fn set_cat(&mut self, lib: &Library, cat: Cat) {
        if cat != self.cat || self.levels.len() != 1 {
            self.cat = cat;
            self.levels = vec![self.root(lib)];
        }
    }

    /// Lista dos dispositivos para escolher (abre como um nível).
    pub fn choose_device(&mut self) {
        let mut items = vec![Item::Device(None, "Todos os dispositivos".into())];
        items.extend(Library::source_nodes().into_iter().map(|n| Item::Device(Some(n.path), n.name)));
        let sel = items.iter().position(|i| matches!(i, Item::Device(d, _) if *d == self.device)).unwrap_or(0) + 1;
        self.push("DISPOSITIVO".into(), items, None);
        if let Some(l) = self.levels.last_mut() {
            l.sel = sel;
        }
        self.area = Area::List;
    }

    /// Nome do dispositivo escolhido (para o botão da barra de cima).
    pub fn device_name(&self) -> String {
        match &self.device {
            None => "TODOS".into(),
            Some(d) => Library::source_nodes()
                .into_iter()
                .find(|n| n.path == *d)
                .map_or_else(|| d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), |n| n.name)
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .to_uppercase(),
        }
    }

    /// Abre o item selecionado (pasta, artista, noite, dispositivo ou voltar).
    pub fn open(&mut self, lib: &Library) {
        let Some(item) = self.selected().cloned() else { return };
        match &item {
            Item::Back => self.back(),
            Item::Device(d, _) => {
                self.device = d.clone();
                self.levels = vec![self.root(lib)];
                self.area = Area::List;
            }
            Item::Folder(n) => {
                let title = n.name.trim_start_matches(|c: char| !c.is_alphanumeric()).to_string();
                let items = self.items_of(lib, &item).unwrap_or_default();
                self.push(title, items, Some(item.clone()));
            }
            Item::Artist(name, _) | Item::Night(_, name) => {
                let title = name.trim_start_matches(|c: char| !c.is_alphanumeric()).to_string();
                let items = self.items_of(lib, &item).unwrap_or_default();
                self.push(title, items, Some(item.clone()));
            }
            Item::Track(_) | Item::Info(_) => {}
        }
    }

    /// Seleciona uma linha da lista (clique do mouse).
    pub fn select(&mut self, i: usize) {
        self.area = Area::List;
        if let Some(l) = self.levels.last_mut() {
            l.sel = i.min(l.items.len().saturating_sub(1));
        }
    }

    /// Abre as pastas até `target` (em PASTAS, todos os dispositivos) e deixa ela aberta.
    pub fn reveal(&mut self, lib: &Library, target: &Path) {
        self.cat = Cat::Pastas;
        self.device = None;
        self.area = Area::List;
        self.levels = vec![self.root(lib)];
        for _ in 0..32 {
            let Some(l) = self.levels.last() else { return };
            let Some(i) = l.items.iter().position(|i| matches!(i, Item::Folder(n) if !n.kind.is_rekordbox() && target.starts_with(&n.path))) else { return };
            let done = matches!(&l.items[i], Item::Folder(n) if n.path == target);
            self.select(i);
            self.open(lib);
            if done {
                return;
            }
        }
    }

    /// Girar o BROWSE (ou ↑↓): anda na área. Subir além da primeira linha chega na barra de cima;
    /// descer na barra de cima volta para a lista.
    pub fn scroll(&mut self, lib: &Library, delta: i32) {
        match self.area {
            Area::Top => {
                if delta > 0 {
                    self.area = Area::List;
                }
            }
            Area::Cats => {
                let i = self.cat.index() as i32;
                if i == 0 && delta < 0 {
                    self.area = Area::Top;
                    return;
                }
                let to = (i + delta).clamp(0, Cat::ALL.len() as i32 - 1) as usize;
                self.set_cat(lib, Cat::ALL[to]);
            }
            Area::List => {
                let Some(l) = self.levels.last_mut() else { return };
                if l.sel == 0 && delta < 0 {
                    self.area = Area::Top;
                    return;
                }
                l.sel = (l.sel as i64 + delta as i64).clamp(0, l.items.len().saturating_sub(1) as i64) as usize;
            }
        }
    }

    /// ←/→ (teclado ou pads): na barra de cima troca o botão; na lista volta/abre; nas
    /// categorias → entra na lista.
    pub fn side(&mut self, lib: &Library, dx: i32) {
        match self.area {
            Area::Top => {
                let i = TopButton::ALL.iter().position(|b| *b == self.top).unwrap_or(0) as i32;
                self.top = TopButton::ALL[(i + dx).clamp(0, TopButton::ALL.len() as i32 - 1) as usize];
            }
            Area::Cats => {
                if dx > 0 {
                    self.area = Area::List;
                }
            }
            Area::List => {
                if dx < 0 {
                    self.back();
                } else if self.selected().is_some_and(|i| i.opens() && !matches!(i, Item::Back)) {
                    self.open(lib);
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------ no aparelho

/// O browser novo está na tela? (CONFIG → TELA, ou DJOS_NAVEGADOR=1 para testar.)
pub fn is_on(a: &AppState) -> bool {
    a.big_browser && enabled(a)
}

pub fn enabled(a: &AppState) -> bool {
    a.settings.browser_novo || std::env::var_os("DJOS_NAVEGADOR").is_some()
}

/// Apertar um botão da barra de cima.
pub fn activate(a: &mut AppState, b: TopButton) {
    a.nav.top = b;
    match b {
        TopButton::Device => a.nav.choose_device(),
        TopButton::Back => a.nav.back(),
        // PREVIEW: mostra/esconde a waveform das músicas na lista.
        TopButton::Preview => {
            a.settings.browser_wave = !a.settings.browser_wave;
            a.settings.save();
        }
        TopButton::Font => {
            a.settings.browser_font = (a.settings.browser_font + 1) % crate::settings::BROWSER_FONTS.len();
            a.settings.save();
        }
    }
}

/// Apertar o BROWSE (ou Enter): aciona o botão de cima, entra na lista ou abre o item.
pub fn press(a: &mut AppState) {
    match a.nav.area {
        Area::Top => {
            let b = a.nav.top;
            activate(a, b);
        }
        Area::Cats => a.nav.area = Area::List,
        Area::List => a.nav.open(&a.lib),
    }
}

pub fn scroll(a: &mut AppState, delta: i32) {
    a.nav.scroll(&a.lib, delta);
}

pub fn side(a: &mut AppState, dx: i32) {
    a.nav.side(&a.lib, dx);
}

/// Pads com a barra de cima em foco (como no teclado da pesquisa): 1 ↑, 2 ↓, 3 ←, 4 →, 5 aperta,
/// 6 volta, 8 fecha o browser. Fora da barra de cima os pads continuam sendo pads (`false`).
pub fn pad(a: &mut AppState, idx: usize) -> bool {
    if !is_on(a) || a.nav.area != Area::Top {
        return false;
    }
    match idx {
        0 => scroll(a, -1),
        1 => scroll(a, 1),
        2 => side(a, -1),
        3 => side(a, 1),
        4 => press(a),
        5 => a.nav.back(),
        7 => a.big_browser = false,
        _ => {}
    }
    true
}

/// LOAD: na música, devolve ela para carregar; no resto, abre (pasta, voltar, dispositivo...).
pub fn load(a: &mut AppState) -> Option<TrackEntry> {
    if a.nav.area != Area::List {
        a.nav.area = Area::List;
        return None;
    }
    match a.nav.selected()? {
        Item::Track(t) => Some(t.clone()),
        _ => {
            a.nav.open(&a.lib);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("dj-os-nav-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for (dir, files) in [("Funk", vec!["MC Kevin - Evoluiu.mp3", "DJ Avila - Batidao.mp3"]), ("Funk/Antigas", vec!["MC Creu - Danca.wav"]), ("House", vec!["Artista - Noite.flac"])] {
            std::fs::create_dir_all(base.join(dir)).unwrap();
            for f in files {
                std::fs::write(base.join(dir).join(f), b"x").unwrap();
            }
        }
        std::fs::write(base.join("Solta - Na Raiz.mp3"), b"x").unwrap();
        base
    }

    fn wait(lib: &Library) {
        for _ in 0..300 {
            if !lib.index.lock().scanning {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("índice não terminou");
    }

    /// Browser já montado e com a pasta de teste como dispositivo (ela não está entre as unidades
    /// de verdade, então só depois da primeira montagem).
    fn on_device(lib: &Library, base: &Path, cat: Cat) -> Nav {
        let mut nav = Nav { cat, ..Default::default() };
        nav.tick(lib);
        nav.device = Some(base.to_path_buf());
        nav.refresh(lib);
        nav
    }

    fn titles(nav: &Nav) -> Vec<String> {
        nav.level()
            .unwrap()
            .items
            .iter()
            .map(|i| match i {
                Item::Back => "←".into(),
                Item::Folder(n) => format!("[{}]", n.name),
                Item::Track(t) => t.title.clone(),
                Item::Artist(a, t) => format!("{a} ({})", t.len()),
                Item::Device(_, n) => format!("<{n}>"),
                Item::Night(_, n) => n.clone(),
                Item::Info(s) => format!("({s})"),
            })
            .collect()
    }

    #[test]
    fn pastas_e_musicas_numa_lista_so() {
        let base = fixture("pastas");
        let lib = Library::with_roots(vec![base.clone()]);
        let mut nav = on_device(&lib, &base, Cat::Pastas);
        assert_eq!(titles(&nav), ["[Funk]", "[House]", "Na Raiz"], "pastas em cima, músicas da raiz embaixo");
        nav.open(&lib); // Funk
        assert_eq!(titles(&nav), ["←", "[Antigas]", "Batidao", "Evoluiu"]);
        assert_eq!(nav.level().unwrap().sel, 1, "abre na primeira coisa depois do voltar");
        assert_eq!(nav.breadcrumb(), "PASTAS  ›  Funk");
        nav.scroll(&lib, 2); // Antigas → Evoluiu
        assert_eq!(nav.selected_track().unwrap().title, "Evoluiu");
        nav.scroll(&lib, -10);
        assert_eq!(nav.level().unwrap().sel, 0, "para no voltar");
        assert_eq!(nav.area, Area::List);
        nav.scroll(&lib, -1);
        assert_eq!(nav.area, Area::Top, "subir além do começo chega na barra de cima");
        nav.scroll(&lib, 1);
        assert_eq!(nav.area, Area::List);
        nav.open(&lib); // ← voltar
        assert_eq!(nav.levels.len(), 1);
        nav.back();
        assert_eq!(nav.area, Area::Cats, "voltar no começo da categoria vai para as categorias");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn categorias_musicas_e_artista() {
        let base = fixture("cats");
        let lib = Library::with_roots(vec![base.clone()]);
        wait(&lib);
        let mut nav = on_device(&lib, &base, Cat::Musicas);
        nav.area = Area::Cats;
        assert_eq!(titles(&nav), ["Batidao", "Danca", "Evoluiu", "Na Raiz", "Noite"], "todas, de todas as pastas, por nome");
        nav.scroll(&lib, -1); // ARTISTA (anda nas categorias)
        nav.scroll(&lib, -1);
        assert_eq!(nav.cat, Cat::Artista);
        assert_eq!(titles(&nav), ["Artista (1)", "DJ Avila (1)", "MC Creu (1)", "MC Kevin (1)", "Solta (1)"]);
        nav.area = Area::List;
        nav.open(&lib);
        assert_eq!(titles(&nav), ["←", "Noite"]);
        // Outro dispositivo: nenhuma música dele.
        nav.device = Some(PathBuf::from("/nao/existe"));
        nav.refresh(&lib);
        assert_eq!(nav.levels.len(), 2, "mantém o caminho aberto");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn escolher_o_dispositivo_pela_barra_de_cima() {
        let lib = Library::with_roots(Vec::new());
        let mut nav = Nav::default();
        nav.tick(&lib);
        nav.area = Area::Top;
        nav.side(&lib, 5);
        assert_eq!(nav.top, TopButton::Font, "para no último botão");
        nav.side(&lib, -5);
        assert_eq!(nav.top, TopButton::Device);
        nav.choose_device();
        assert_eq!(nav.area, Area::List);
        let t = titles(&nav);
        assert_eq!(t[1], "<Todos os dispositivos>");
        assert!(t.iter().any(|n| n.contains("Músicas internas")), "{t:?}");
        let internal = crate::app::data_dir().join("musicas");
        let i = nav.level().unwrap().items.iter().position(|i| matches!(i, Item::Device(Some(p), _) if *p == internal)).unwrap();
        nav.levels.last_mut().unwrap().sel = i;
        nav.open(&lib);
        assert_eq!(nav.device, Some(internal));
        assert_eq!(nav.levels.len(), 1, "volta para o começo da categoria");
        assert_eq!(nav.device_name(), "MÚSICAS INTERNAS");
    }

    #[test]
    fn lista_so_rola_quando_a_selecao_sai_da_tela() {
        let mut l = Level { title: String::new(), items: vec![Item::Back; 30], sel: 0, first: 0, from: None };
        l.sel = 5;
        l.scroll_into_view(10);
        assert_eq!(l.first, 0, "clicar numa linha que aparece não mexe na lista");
        l.sel = 12;
        l.scroll_into_view(10);
        assert_eq!(l.first, 3, "desceu além da tela: a seleção fica na última linha");
        l.sel = 1;
        l.scroll_into_view(10);
        assert_eq!(l.first, 1);
    }

    #[test]
    fn fila_e_historico_atualizam_sozinhos() {
        let base = fixture("fila");
        let mut lib = Library::with_roots(vec![base.clone()]);
        let mut nav = Nav { cat: Cat::Fila, ..Default::default() };
        nav.tick(&lib);
        assert!(matches!(nav.selected(), Some(Item::Info(_))), "fila vazia avisa");
        lib.prepare.push(TrackEntry::from_path(&base.join("House/Artista - Noite.flac")));
        nav.tick(&lib);
        assert_eq!(titles(&nav), ["Noite"]);
        let _ = std::fs::remove_dir_all(base);
    }
}
