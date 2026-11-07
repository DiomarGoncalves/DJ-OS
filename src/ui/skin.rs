//! Skins: as cores e o layout da tela principal e do browser num arquivo de texto (`.djosskin`).
//! O formato está em `docs/skins.md`. As skins que vêm com o DJ OS ficam em `skins/` (dentro do
//! programa); as da pessoa em `~/DJOS/skins/` (com o mesmo nome, a dela vale).

use eframe::egui::Color32;
use parking_lot::{Mutex, RwLock};
use std::path::PathBuf;

/// Cores e medidas da tela.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skin {
    pub fundo: Color32,
    pub painel: Color32,
    pub painel2: Color32,
    pub linha: Color32,
    pub texto: Color32,
    pub apagado: Color32,
    pub destaque: Color32,
    pub laranja: Color32,
    pub vermelho: Color32,
    pub verde: Color32,
    /// Linha selecionada no browser (com o BROWSE nela / do outro lado).
    pub selecao: Color32,
    pub selecao_fraca: Color32,
    pub decks: [Color32; 4],
    /// Waveform: grave, médio, agudo.
    pub bandas: [Color32; 3],
    pub cues: [Color32; 8],
    /// Largura do cartão de cada deck à esquerda das waveforms (0 = esconde).
    pub painel_deck: f32,
    /// Largura do painel de efeitos à direita (0 = esconde).
    pub painel_efeitos: f32,
    pub hotcues: bool,
    pub decks_embaixo: bool,
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// A skin de fábrica (preta, estilo aparelho de cabine).
pub const PADRAO: Skin = Skin {
    fundo: c(0, 0, 0),
    painel: c(14, 15, 17),
    painel2: c(26, 27, 31),
    linha: c(52, 54, 60),
    texto: c(236, 236, 236),
    apagado: c(128, 130, 136),
    destaque: c(30, 140, 255),
    laranja: c(255, 140, 20),
    vermelho: c(230, 30, 40),
    verde: c(60, 210, 90),
    selecao: c(20, 110, 220),
    selecao_fraca: c(38, 58, 92),
    decks: [c(30, 140, 255), c(255, 140, 20), c(170, 90, 255), c(40, 200, 150)],
    bandas: [c(25, 100, 240), c(235, 140, 30), c(250, 238, 218)],
    cues: [c(235, 60, 60), c(40, 190, 230), c(70, 205, 80), c(140, 80, 245), c(130, 220, 60), c(255, 130, 30), c(50, 95, 255), c(240, 205, 40)],
    painel_deck: 176.0,
    painel_efeitos: 236.0,
    hotcues: true,
    decks_embaixo: true,
};

static ATUAL: RwLock<Skin> = parking_lot::const_rwlock(PADRAO);
static NOME: Mutex<String> = parking_lot::const_mutex(String::new());

/// A skin em uso agora.
pub fn atual() -> Skin {
    *ATUAL.read()
}

fn cor(v: &str) -> Option<Color32> {
    let h = v.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some(c((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

/// Lê um arquivo de skin (o que não estiver no arquivo fica como na skin de fábrica). Devolve o
/// nome e a skin; o erro diz a linha.
pub fn ler(texto: &str) -> Result<(String, Skin), String> {
    let mut s = PADRAO;
    let mut nome = String::new();
    for (i, linha) in texto.lines().enumerate() {
        // Comentário: a linha começa com # (as cores também têm #, então no meio da linha o # só
        // é comentário depois do valor, separado por espaço).
        let l = linha.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let erro = |e: &str| format!("linha {}: {e} ({})", i + 1, l);
        let (k, resto) = l.split_once('=').ok_or_else(|| erro("use chave = valor"))?;
        let k = k.trim();
        let v = resto.split_whitespace().next().unwrap_or("");
        let texto_livre = resto.split(" #").next().unwrap_or("").trim();
        let px = || v.parse::<f32>().ok().filter(|x| (0.0..=600.0).contains(x)).ok_or_else(|| erro("número de 0 a 600"));
        let sim = || match v {
            "sim" => Ok(true),
            "nao" | "não" => Ok(false),
            _ => Err(erro("sim ou nao")),
        };
        let co = || cor(v).ok_or_else(|| erro("cor no formato #RRGGBB"));
        match k {
            "nome" => nome = texto_livre.to_string(),
            "fundo" => s.fundo = co()?,
            "painel" => s.painel = co()?,
            "painel2" => s.painel2 = co()?,
            "linha" => s.linha = co()?,
            "texto" => s.texto = co()?,
            "apagado" => s.apagado = co()?,
            "destaque" => s.destaque = co()?,
            "laranja" => s.laranja = co()?,
            "vermelho" => s.vermelho = co()?,
            "verde" => s.verde = co()?,
            "selecao" => s.selecao = co()?,
            "selecao_fraca" => s.selecao_fraca = co()?,
            "deck1" | "deck2" | "deck3" | "deck4" => s.decks[k[4..].parse::<usize>().unwrap() - 1] = co()?,
            "grave" => s.bandas[0] = co()?,
            "medio" => s.bandas[1] = co()?,
            "agudo" => s.bandas[2] = co()?,
            "cue_a" | "cue_b" | "cue_c" | "cue_d" | "cue_e" | "cue_f" | "cue_g" | "cue_h" => s.cues[(k.as_bytes()[4] - b'a') as usize] = co()?,
            "painel_deck" => s.painel_deck = px()?,
            "painel_efeitos" => s.painel_efeitos = px()?,
            "hotcues" => s.hotcues = sim()?,
            "decks_embaixo" => s.decks_embaixo = sim()?,
            _ => return Err(erro("chave desconhecida (lista em docs/skins.md)")),
        }
    }
    if nome.is_empty() {
        return Err("falta \"nome = ...\"".into());
    }
    Ok((nome, s))
}

/// Skins que vêm dentro do programa (pasta `skins/` do projeto, juntada pelo `build.rs`).
pub const EMBUTIDAS: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/skins.rs"));

pub fn pasta_usuario() -> PathBuf {
    crate::app::data_dir().join("skins")
}

/// Todas as skins (nome, skin): as do programa e as da pessoa. Arquivo com erro fica de fora.
pub fn todas() -> Vec<(String, Skin)> {
    let mut v: Vec<(String, Skin)> = Vec::new();
    for (arq, texto) in EMBUTIDAS {
        match ler(texto) {
            Ok(x) => v.push(x),
            Err(e) => eprintln!("[djos] skin {arq}: {e}"),
        }
    }
    let mut proprias: Vec<PathBuf> = std::fs::read_dir(pasta_usuario()).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "djosskin")).collect()).unwrap_or_default();
    proprias.sort();
    for p in proprias {
        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| ler(&t)) {
            Ok((n, s)) => {
                v.retain(|(x, _)| *x != n);
                v.push((n, s));
            }
            Err(e) => eprintln!("[djos] skin {}: {e}", p.display()),
        }
    }
    v
}

/// Usa a skin `nome` (vazio ou inexistente: a de fábrica). Chamado a cada quadro: só relê os
/// arquivos quando o nome muda.
pub fn garantir(nome: &str) {
    let mut atual = NOME.lock();
    if *atual == nome && !nome.is_empty() {
        return;
    }
    if atual.is_empty() && nome.is_empty() {
        *atual = " ".into(); // marca "de fábrica já aplicada"
        *ATUAL.write() = PADRAO;
        return;
    }
    *atual = nome.to_string();
    *ATUAL.write() = todas().into_iter().find(|(n, _)| n == nome).map_or(PADRAO, |(_, s)| s);
}

/// Força reler os arquivos na próxima vez (a pessoa editou a skin).
pub fn recarregar() {
    NOME.lock().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_a_skin() {
        let (n, s) = ler("nome = Teste\nfundo = #FFFFFF\ntexto = #101010 # escuro\ncue_c = #00FF00\npainel_efeitos = 0\nhotcues = nao\n").unwrap();
        assert_eq!(n, "Teste");
        assert_eq!(s.fundo, Color32::WHITE);
        assert_eq!(s.texto, c(16, 16, 16));
        assert_eq!(s.cues[2], c(0, 255, 0));
        assert_eq!((s.painel_efeitos, s.hotcues, s.painel_deck), (0.0, false, 176.0), "o que não está no arquivo fica de fábrica");
        assert!(ler("nome = X\nfundo = branco\n").unwrap_err().starts_with("linha 2:"));
        assert!(ler("fundo = #000000\n").unwrap_err().contains("nome"));
    }

    #[test]
    fn embutidas_estao_certas() {
        assert!(!EMBUTIDAS.is_empty());
        for (arq, texto) in EMBUTIDAS {
            ler(texto).unwrap_or_else(|e| panic!("{arq}: {e}"));
        }
    }
}
