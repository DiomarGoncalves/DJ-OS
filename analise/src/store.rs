//! Memória por música: BPM, grade, waveform, cue, hotcues e loop ficam salvos em
//! `~/DJOS/dados/`. Na segunda vez a música abre sem analisar de novo e com os cues de antes.
//!
//! A chave é o nome do arquivo + tamanho + data de modificação: continua valendo se o pendrive
//! for montado em outro lugar, e muda se o arquivo for trocado.
//!
//! Pendrive preparado no PC (`dj-os --preparar-pendrive`, estilo rekordbox): a análise de cada
//! música fica no próprio pendrive, em `DJOS/analise/`, com a chave nome + tamanho (a data dos
//! arquivos do pendrive muda conforme o fuso de quem monta). O aparelho lê de lá e não precisa
//! analisar; o que for marcado no aparelho (cues) continua indo para `~/DJOS/dados/`.

use crate::data_dir;
use crate::HOTCUES;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// DXJ1: formato antigo (sem o tom). DXJ2: com 1 byte de tom logo depois do BPM. DXJ3: DXJ2 +
/// volume (LUFS, f32) no fim e, desde 2026-11, mais 1 byte depois dele com a versão do detector
/// de BPM (sem o byte = detector antigo). O byte fica no fim de propósito: um firmware anterior
/// lê o arquivo normalmente e ignora o que sobra, então voltar de firmware não perde os cues.
/// DXJ4: só o firmware de teste de 2026-11-07 gravou (igual ao DXJ3 com o byte); é lido e
/// regravado como DXJ3.
const MAGIC_V1: &[u8; 4] = b"DXJ1";
const MAGIC_V2: &[u8; 4] = b"DXJ2";
const MAGIC: &[u8; 4] = b"DXJ3";
const MAGIC_V4: &[u8; 4] = b"DXJ4";
/// Onde começam as três waveforms (cada uma: tamanho em 4 bytes + os bytes).
const WAVES_AT: u64 = 4 + 8 + 1 + 8 + 9 + 9 * HOTCUES as u64 + 25;

#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    pub bpm: f64,
    /// Tom como em `key::to_raw` (0 = ainda não analisado).
    pub key: u8,
    /// Volume percebido (LUFS): NaN = ainda não analisado.
    pub loudness: f32,
    pub first_beat: f64,
    pub low: Vec<u8>,
    pub mid: Vec<u8>,
    pub high: Vec<u8>,
    pub cue: Option<f64>,
    pub hotcues: [Option<f64>; HOTCUES],
    /// Loop guardado: entrada, saída e tamanho em batidas.
    pub loop_: Option<(f64, f64, f64)>,
    /// Versão do detector que mediu o BPM (`track::BPM_V`; grade ajustada à mão ou vinda do
    /// rekordbox também conta como atual). Menor que a atual: o BPM é medido de novo.
    pub bpm_v: u8,
}

impl Saved {
    /// Só a análise (sem cues), para a primeira vez que a música é aberta.
    pub fn from_track(t: &crate::track::Track) -> Self {
        Saved {
            bpm: t.bpm(),
            key: t.key,
            loudness: t.loudness,
            first_beat: t.first_beat(),
            low: t.wave.low.clone(),
            mid: t.wave.mid.clone(),
            high: t.wave.high.clone(),
            cue: None,
            hotcues: [None; HOTCUES],
            loop_: None,
            bpm_v: crate::track::BPM_V,
        }
    }

    /// O BPM foi medido pelo detector atual (ou acertado à mão).
    pub fn bpm_atual(&self) -> bool {
        self.bpm_v >= crate::track::BPM_V
    }

    /// A música `t` foi medida de novo pelo detector atual. A grade guardada só é trocada quando
    /// a nova conserta o erro que o detector novo veio consertar (o BPM guardado era 2/3 ou 1,5×
    /// o certo, como o funk em 86,66): qualquer outra diferença fica como estava, porque a grade
    /// guardada pode ter sido acertada à mão ou vindo do rekordbox. Devolve o que gravar.
    pub fn com_bpm_remedido(&self, t: &crate::track::Track) -> Saved {
        let conserto = self.bpm <= 0.0 || [1.5, 2.0 / 3.0].iter().any(|x| (t.bpm() / self.bpm / x - 1.0).abs() < 0.02);
        if !conserto {
            t.set_grid(self.bpm, self.first_beat);
        }
        Saved { key: t.key, loudness: t.loudness, bpm: t.bpm(), first_beat: t.first_beat(), bpm_v: crate::track::BPM_V, ..self.clone() }
    }
}

fn fnv(h: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *h ^= *b as u64;
        *h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

/// Chave estável da música (nome + tamanho + data de modificação).
pub fn key(path: &Path) -> Option<String> {
    let md = std::fs::metadata(path).ok()?;
    let mtime = md.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    fnv(&mut h, path.file_name()?.to_string_lossy().as_bytes());
    fnv(&mut h, &md.len().to_le_bytes());
    fnv(&mut h, &mtime.to_le_bytes());
    Some(format!("{h:016x}"))
}

fn encode(s: &Saved) -> Vec<u8> {
    let mut b = Vec::with_capacity(64 + s.low.len() * 3);
    b.extend_from_slice(MAGIC);
    let f = |b: &mut Vec<u8>, v: f64| b.extend_from_slice(&v.to_le_bytes());
    let opt = |b: &mut Vec<u8>, v: Option<f64>| {
        b.push(v.is_some() as u8);
        b.extend_from_slice(&v.unwrap_or(0.0).to_le_bytes());
    };
    f(&mut b, s.bpm);
    b.push(s.key);
    f(&mut b, s.first_beat);
    opt(&mut b, s.cue);
    for h in s.hotcues {
        opt(&mut b, h);
    }
    b.push(s.loop_.is_some() as u8);
    let (a, z, n) = s.loop_.unwrap_or((0.0, 0.0, 0.0));
    for v in [a, z, n] {
        f(&mut b, v);
    }
    for w in [&s.low, &s.mid, &s.high] {
        b.extend_from_slice(&(w.len() as u32).to_le_bytes());
        b.extend_from_slice(w);
    }
    b.extend_from_slice(&s.loudness.to_le_bytes());
    b.push(s.bpm_v);
    b
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.b.get(self.i..self.i + n)?;
        self.i += n;
        Some(s)
    }
    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn opt(&mut self) -> Option<Option<f64>> {
        let has = self.take(1)?[0] != 0;
        let v = self.f64()?;
        Some(has.then_some(v))
    }
    fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize;
        Some(self.take(n)?.to_vec())
    }
}

fn decode(b: &[u8]) -> Option<Saved> {
    let mut r = Reader { b, i: 0 };
    let version = match r.take(4)? {
        m if m == MAGIC || m == MAGIC_V4 => 3,
        m if m == MAGIC_V2 => 2,
        m if m == MAGIC_V1 => 1,
        _ => return None,
    };
    let v2 = version >= 2;
    let bpm = r.f64()?;
    let key = if v2 { r.take(1)?[0] } else { crate::key::RAW_UNKNOWN };
    let first_beat = r.f64()?;
    let cue = r.opt()?;
    let mut hotcues = [None; HOTCUES];
    for h in hotcues.iter_mut() {
        *h = r.opt()?;
    }
    let has_loop = r.take(1)?[0] != 0;
    let (a, z, n) = (r.f64()?, r.f64()?, r.f64()?);
    let low = r.bytes()?;
    let mid = r.bytes()?;
    let high = r.bytes()?;
    if !bpm.is_finite() || low.len() != mid.len() || mid.len() != high.len() {
        return None;
    }
    let loudness = if version >= 3 { f32::from_le_bytes(r.take(4)?.try_into().ok()?) } else { f32::NAN };
    // Byte da versão do detector de BPM, se o arquivo tiver.
    let bpm_v = r.take(1).map_or(1, |b| b[0]);
    Some(Saved { bpm, key, loudness, first_beat, low, mid, high, cue, hotcues, loop_: has_loop.then_some((a, z, n)), bpm_v })
}

fn dir() -> PathBuf {
    data_dir().join("dados")
}

/// Pasta da análise dentro do pendrive preparado.
pub const PENDRIVE_DIR: &str = "DJOS/analise";

/// Chave da análise no pendrive: nome + tamanho (sem a data).
fn portable_key(track: &Path) -> Option<String> {
    let md = std::fs::metadata(track).ok()?;
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    fnv(&mut h, b"pendrive");
    fnv(&mut h, track.file_name()?.to_string_lossy().as_bytes());
    fnv(&mut h, &md.len().to_le_bytes());
    Some(format!("{h:016x}"))
}

/// Arquivo da análise da música no pendrive dela, se o pendrive foi preparado.
fn pendrive_file(track: &Path) -> Option<PathBuf> {
    let root = track.ancestors().skip(1).find(|a| a.join(PENDRIVE_DIR).is_dir())?;
    Some(root.join(PENDRIVE_DIR).join(format!("{}.dxj", portable_key(track)?)))
}

/// Grava a análise de uma música no pendrive preparado em `root`.
pub fn save_portable(root: &Path, track: &Path, s: &Saved) -> std::io::Result<()> {
    let k = portable_key(track).ok_or_else(|| std::io::Error::other("sem chave"))?;
    let dir = root.join(PENDRIVE_DIR);
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!("{k}.tmp"));
    std::fs::write(&tmp, encode(s))?;
    std::fs::rename(tmp, dir.join(format!("{k}.dxj")))
}

/// A música já tem análise completa (com tom e volume) no pendrive preparado em `root`.
pub fn has_portable(root: &Path, track: &Path) -> bool {
    portable_key(track)
        .and_then(|k| peek_file(&root.join(PENDRIVE_DIR).join(format!("{k}.dxj"))))
        .is_some_and(|p| p.completa())
}

#[cfg(test)]
fn load_from(dir: &Path, track: &Path) -> Option<Saved> {
    let bytes = std::fs::read(dir.join(format!("{}.dxj", key(track)?))).ok()?;
    decode(&bytes)
}

fn save_to(dir: &Path, track: &Path, s: &Saved) -> std::io::Result<()> {
    let k = key(track).ok_or_else(|| std::io::Error::other("sem chave"))?;
    std::fs::create_dir_all(dir)?;
    // Grava num arquivo temporário e troca: um desligamento no meio não corrompe o salvo.
    let tmp = dir.join(format!("{k}.tmp"));
    std::fs::write(&tmp, encode(s))?;
    std::fs::rename(tmp, dir.join(format!("{k}.dxj")))
}

/// O que já foi analisado de uma música (lê só o começo do arquivo, sem a waveform).
pub struct Peek {
    pub bpm: f64,
    pub key: u8,
    /// O volume (ganho automático) já foi medido (formato DXJ3 ou mais novo).
    pub has_loudness: bool,
    /// O BPM foi medido pelo detector atual (formato DXJ4).
    pub bpm_atual: bool,
}

impl Peek {
    /// Análise completa e atual: tom, volume e BPM do detector atual.
    pub fn completa(&self) -> bool {
        self.key != crate::key::RAW_UNKNOWN && self.has_loudness && self.bpm_atual
    }
}

pub fn peek(track: &Path) -> Option<Peek> {
    let own = key(track).and_then(|k| peek_file(&dir().join(format!("{k}.dxj"))));
    own.or_else(|| peek_file(&pendrive_file(track)?))
}

fn peek_file(path: &Path) -> Option<Peek> {
    use std::io::Read;
    use std::io::{Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let mut b = [0u8; 13];
    let n = f.read(&mut b).ok()?;
    let m = b.get(..4)?;
    let v3 = m == MAGIC || m == MAGIC_V4;
    if n < 12 || (!v3 && m != MAGIC_V2 && m != MAGIC_V1) {
        return None;
    }
    let bpm = f64::from_le_bytes(b[4..12].try_into().ok()?);
    let key = if m != MAGIC_V1 && n == 13 { b[12] } else { crate::key::RAW_UNKNOWN };
    // O byte da versão do detector vem depois do volume: pelo tamanho das waveforms dá para
    // saber se o arquivo tem ele (1 byte a mais que o DXJ3 antigo) e ler só esse byte.
    let bpm_atual = v3 && (|| {
        let mut w = [0u8; 4];
        f.seek(SeekFrom::Start(WAVES_AT)).ok()?;
        f.read_exact(&mut w).ok()?;
        let sem_byte = WAVES_AT + 3 * (4 + u32::from_le_bytes(w) as u64) + 4;
        if f.metadata().ok()?.len() <= sem_byte {
            return None;
        }
        let mut v = [0u8; 1];
        f.seek(SeekFrom::Start(sem_byte)).ok()?;
        f.read_exact(&mut v).ok()?;
        Some(v[0] >= crate::track::BPM_V)
    })()
    .unwrap_or(false);
    bpm.is_finite().then_some(Peek { bpm, key, has_loudness: v3, bpm_atual })
}

/// O que o aparelho guardou da música ou a análise do pendrive preparado. Se tem os dois, vale o
/// gravado por último: cues marcados no Preparar depois de tocar a música chegam ao aparelho, e os
/// marcados no aparelho depois continuam valendo.
pub fn load(track: &Path) -> Option<Saved> {
    let own = key(track).map(|k| dir().join(format!("{k}.dxj")));
    newest(own.into_iter().chain(pendrive_file(track)))
}

/// Lê o mais recente que existir (e der para ler) entre os arquivos.
fn newest(files: impl Iterator<Item = PathBuf>) -> Option<Saved> {
    let mut v: Vec<(std::time::SystemTime, PathBuf)> = files.filter_map(|f| Some((std::fs::metadata(&f).ok()?.modified().ok()?, f))).collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.iter().find_map(|(_, f)| decode(&std::fs::read(f).ok()?))
}

/// A memória da música no aparelho está no formato do firmware de teste (DXJ4), que os firmwares
/// anteriores não leem: regravar (como DXJ3) para poder voltar de firmware sem perder os cues.
pub fn formato_de_teste(track: &Path) -> bool {
    use std::io::Read;
    let mut m = [0u8; 4];
    key(track).and_then(|k| std::fs::File::open(dir().join(format!("{k}.dxj"))).ok()).is_some_and(|mut f| f.read_exact(&mut m).is_ok() && &m == MAGIC_V4)
}

/// Só a análise gravada no pendrive preparado em `root` (o Preparar edita os cues nela).
pub fn load_portable(root: &Path, track: &Path) -> Option<Saved> {
    decode(&std::fs::read(root.join(PENDRIVE_DIR).join(format!("{}.dxj", portable_key(track)?))).ok()?)
}

pub fn save(track: &Path, s: &Saved) {
    if let Err(e) = save_to(&dir(), track, s) {
        eprintln!("[djos] não salvou os dados de {}: {e}", track.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vale_o_gravado_por_ultimo() {
        let base = std::env::temp_dir().join(format!("djos-store-novo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let (a, b) = (base.join("a.dxj"), base.join("b.dxj"));
        let mut s = sample();
        s.cue = Some(1.0);
        std::fs::write(&a, encode(&s)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        s.cue = Some(2.0);
        std::fs::write(&b, encode(&s)).unwrap();
        assert_eq!(newest([a.clone(), b.clone()].into_iter()).unwrap().cue, Some(2.0));
        assert_eq!(newest([b.clone(), a.clone()].into_iter()).unwrap().cue, Some(2.0), "a ordem não importa");
        assert_eq!(newest([base.join("nao-existe.dxj"), a].into_iter()).unwrap().cue, Some(1.0));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn analise_antiga_mede_o_bpm_de_novo_e_mantem_os_cues() {
        let s = sample();
        // A mesma análise como os firmwares anteriores gravavam (sem o byte do detector de BPM).
        let mut antigo = encode(&s);
        antigo.pop();
        // O arquivo novo continua sendo DXJ3: um firmware anterior lê e não perde os cues.
        assert_eq!(&encode(&s)[..4], b"DXJ3");
        // O formato do firmware de teste (DXJ4) continua sendo lido.
        let mut v4 = encode(&s);
        v4[..4].copy_from_slice(MAGIC_V4);
        assert_eq!(decode(&v4), Some(s.clone()));
        let d = decode(&antigo).unwrap();
        assert!(!d.bpm_atual(), "DXJ3: BPM do detector antigo");
        assert_eq!((d.cue, d.hotcues, d.loop_), (s.cue, s.hotcues, s.loop_), "cues continuam");
        let dir = std::env::temp_dir().join(format!("djos-store-bpmv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.dxj");
        std::fs::write(&f, &antigo).unwrap();
        let p = peek_file(&f).unwrap();
        assert!(p.has_loudness && !p.bpm_atual && !p.completa());
        std::fs::write(&f, encode(&s)).unwrap();
        assert!(peek_file(&f).unwrap().completa());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn remedir_so_conserta_o_dois_tercos() {
        let antigo = |bpm: f64| Saved { bpm, first_beat: 777.0, bpm_v: 1, ..sample() };
        let medida = |bpm: f64| crate::track::synthetic(1.0, 48000.0, bpm, 0.5);
        // Funk guardado em 86,66 e medido agora em 130: troca a grade, mantém os cues.
        let n = antigo(86.66).com_bpm_remedido(&medida(130.0));
        assert_eq!((n.bpm, n.first_beat, n.cue, n.bpm_v), (130.0, 24000.0, Some(1234.0), crate::track::BPM_V));
        // Grade acertada à mão (126,5) e o detector novo acha 128: fica a guardada, na música também.
        let t = medida(128.0);
        let n = antigo(126.5).com_bpm_remedido(&t);
        assert_eq!((n.bpm, n.first_beat), (126.5, 777.0));
        assert_eq!((t.bpm(), t.first_beat()), (126.5, 777.0));
        assert!(n.bpm_atual(), "não mede de novo no próximo LOAD");
    }

    fn sample() -> Saved {
        let mut hotcues = [None; HOTCUES];
        hotcues[0] = Some(12345.5);
        hotcues[7] = Some(99.0);
        Saved {
            bpm: 128.0,
            key: crate::key::to_raw(Some(21)),
            loudness: -8.5,
            first_beat: 1234.0,
            low: vec![1, 2, 3],
            mid: vec![4, 5, 6],
            high: vec![7, 8, 9],
            cue: Some(1234.0),
            bpm_v: crate::track::BPM_V,
            hotcues,
            loop_: Some((1000.0, 2000.0, 4.0)),
        }
    }

    #[test]
    fn le_o_formato_antigo_sem_tom() {
        let s = sample();
        // DXJ1 = DXJ2 sem o byte do tom (logo depois do BPM).
        let mut old = encode(&s);
        old[..4].copy_from_slice(MAGIC_V1);
        old.remove(12);
        old.truncate(old.len() - 4); // sem o volume
        let lido = decode(&old).expect("lê o formato antigo");
        assert_eq!(lido.key, crate::key::RAW_UNKNOWN, "tom ainda não analisado");
        assert!(lido.loudness.is_nan(), "volume ainda não analisado");
        assert_eq!((lido.bpm, lido.cue, lido.hotcues, lido.loop_), (s.bpm, s.cue, s.hotcues, s.loop_), "cues mantidos");
    }

    #[test]
    fn grava_e_le_igual() {
        let s = sample();
        assert_eq!(decode(&encode(&s)), Some(s.clone()));
        let mut t = s;
        t.cue = None;
        t.loop_ = None;
        assert_eq!(decode(&encode(&t)), Some(t));
    }

    #[test]
    fn arquivo_estragado_nao_quebra() {
        let b = encode(&sample());
        assert_eq!(decode(&b[..b.len() - 2]), None, "cortado");
        assert_eq!(decode(b"XXXX"), None, "outro formato");
        assert_eq!(decode(&[]), None);
    }

    #[test]
    fn chave_muda_quando_o_arquivo_muda() {
        let base = std::env::temp_dir().join(format!("dj-os-store-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let f = base.join("musica.mp3");
        std::fs::write(&f, b"abc").unwrap();
        let k1 = key(&f).unwrap();
        assert_eq!(key(&f).unwrap(), k1, "mesma música, mesma chave");
        std::fs::write(&f, b"abcd").unwrap();
        assert_ne!(key(&f).unwrap(), k1, "arquivo trocado, chave nova");

        let dir = base.join("dados");
        save_to(&dir, &f, &sample()).unwrap();
        assert_eq!(load_from(&dir, &f), Some(sample()));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn pendrive_preparado() {
        let root = std::env::temp_dir().join(format!("dj-os-pendrive-{}", std::process::id()));
        let f = root.join("house antigo").join("Artista - Música.mp3");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, b"abc").unwrap();
        assert_eq!(pendrive_file(&f), None, "pendrive sem preparar");
        assert!(!has_portable(&root, &f));
        save_portable(&root, &f, &sample()).unwrap();
        assert!(has_portable(&root, &f));
        // O aparelho acha a análise subindo as pastas a partir da música.
        let arq = pendrive_file(&f).expect("acha a pasta DJOS/analise na raiz");
        assert_eq!(decode(&std::fs::read(&arq).unwrap()), Some(sample()));
        let p = peek_file(&arq).unwrap();
        assert_eq!((p.bpm, p.has_loudness), (128.0, true));
        // A data do arquivo não entra na chave (muda com o fuso de quem monta o pendrive).
        let k = portable_key(&f).unwrap();
        std::fs::File::options().write(true).open(&f).unwrap().set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000)).unwrap();
        assert_eq!(portable_key(&f).unwrap(), k);
        let _ = std::fs::remove_dir_all(root);
    }
}
