//! Análise das músicas do DJ OS, usada pelo aparelho e pelo DJ OS Preparar (o programa do PC que
//! prepara o pendrive, estilo rekordbox). Os dois usam este mesmo código: a análise feita no PC é
//! idêntica à do aparelho.
//!
//! - `track`: lê o arquivo de áudio e analisa BPM, grade, waveform, tom e volume;
//! - `store`: memória de cada música (no aparelho e no pendrive preparado);
//! - `prepare`: prepara um pendrive inteiro.

pub mod dsp;
pub mod key;
pub mod loudness;
pub mod prepare;
pub mod rekordbox;
pub mod store;
pub mod track;

use std::path::PathBuf;

/// Quantidade de hotcues por música.
pub const HOTCUES: usize = 8;

/// Pasta de dados do DJ OS (`~/DJOS`).
pub fn data_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
        .join("DJOS")
}
