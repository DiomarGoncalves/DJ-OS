//! Memória por música: BPM, grade, waveform, cue, hotcues e loop ficam salvos em
//! `~/DJOS/dados/`. Na segunda vez a música abre sem analisar de novo e com os cues de antes.
//!
//! A chave é o nome do arquivo + tamanho + data de modificação: continua valendo se o pendrive
//! for montado em outro lugar, e muda se o arquivo for trocado.

use crate::app::data_dir;
use crate::shared::HOTCUES;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// DXJ1: formato antigo (sem o tom). DXJ2: com 1 byte de tom logo depois do BPM. DXJ3: DXJ2 +
/// volume (LUFS, f32) no fim.
const MAGIC_V1: &[u8; 4] = b"DXJ1";
const MAGIC_V2: &[u8; 4] = b"DXJ2";
const MAGIC: &[u8; 4] = b"DXJ3";

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
}

impl Saved {
    /// Só a análise (sem cues), para a primeira vez que a música é aberta.
    pub fn from_track(t: &crate::track::Track) -> Self {
        Saved {
            bpm: t.bpm,
            key: t.key,
            loudness: t.loudness,
            first_beat: t.first_beat,
            low: t.wave.low.clone(),
            mid: t.wave.mid.clone(),
            high: t.wave.high.clone(),
            cue: None,
            hotcues: [None; HOTCUES],
            loop_: None,
        }
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
        m if m == MAGIC => 3,
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
    Some(Saved { bpm, key, loudness, first_beat, low, mid, high, cue, hotcues, loop_: has_loop.then_some((a, z, n)) })
}

fn dir() -> PathBuf {
    data_dir().join("dados")
}

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
    /// O volume (ganho automático) já foi medido (formato DXJ3).
    pub has_loudness: bool,
}

pub fn peek(track: &Path) -> Option<Peek> {
    use std::io::Read;
    let mut f = std::fs::File::open(dir().join(format!("{}.dxj", key(track)?))).ok()?;
    let mut b = [0u8; 13];
    let n = f.read(&mut b).ok()?;
    let m = b.get(..4)?;
    if n < 12 || (m != MAGIC && m != MAGIC_V2 && m != MAGIC_V1) {
        return None;
    }
    let bpm = f64::from_le_bytes(b[4..12].try_into().ok()?);
    let key = if m != MAGIC_V1 && n == 13 { b[12] } else { crate::key::RAW_UNKNOWN };
    bpm.is_finite().then_some(Peek { bpm, key, has_loudness: m == MAGIC })
}

pub fn load(track: &Path) -> Option<Saved> {
    load_from(&dir(), track)
}

pub fn save(track: &Path, s: &Saved) {
    if let Err(e) = save_to(&dir(), track, s) {
        eprintln!("[djos] não salvou os dados de {}: {e}", track.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
