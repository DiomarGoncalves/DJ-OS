//! Mapeamento de controladora por arquivo (`.djosmap`): cada linha liga uma mensagem MIDI a uma
//! ação do DJ OS, e os LEDs a um estado. O formato e todas as ações estão em
//! `docs/mapeamento.md`. Os mapeamentos que vêm com o DJ OS ficam em `mapeamentos/` (dentro do
//! programa); a pessoa põe os dela em `~/DJOS/mapeamentos/` (com o mesmo nome, o dela vale).
//!
//! Exemplo:
//!
//! ```text
//! nome = Pioneer DDJ-400
//! porta = DDJ-400
//! nota 1 0x0B play deck=1
//! cc   1 0x13 volume deck=1 lsb=0x33
//! cc   1 0x22 jog_scratch deck=1
//! led nota 1 0x0B tocando deck=1
//! ```

use crate::app::{Loader, PadMode, SharedApp};
use crate::engine::fx::{beats_label, step_beats};
use crate::shared::{Cmd, CmdTx, SharedSnap, Snapshot, NUM_DECKS};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// Tipo da mensagem MIDI.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tipo {
    Nota,
    Cc,
    /// Pitch bend (14 bits numa mensagem só; alguns faders de pitch).
    Pb,
}

/// Como ler o valor que chega.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Valor {
    /// Apertou (nota ligada ou valor > 0) / soltou.
    Botao,
    /// Posição de 0 a 127 (ou 14 bits com `lsb=`).
    Absoluto,
    /// Encoder: 1..63 horário, 127..65 anti-horário (complemento de dois).
    Relativo,
    /// Encoder com o meio em 64: 65+ horário, 63- anti-horário (jogs da Pioneer).
    Relativo64,
}

/// O que um controle faz.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Acao {
    // ---- deck (precisa de deck=)
    Play,
    Cue,
    Sync,
    PitchZero,
    Stutter,
    Inicio,
    Load,
    Loop,
    LoopSair,
    LoopMetade,
    LoopDobro,
    LoopIn,
    LoopOut,
    LoopAuto,
    Beatjump,
    Pfl,
    Vinil,
    Keylock,
    Slip,
    FaixaPitch,
    Volume,
    Tempo,
    Trim,
    Grave,
    Medio,
    Agudo,
    Filtro,
    JogToque,
    JogScratch,
    JogBend,
    JogBusca,
    Hotcue,
    ApagarHotcue,
    Pad,
    PadModo(PadMode),
    FxLiga,
    FxNivel,
    FxBatidaMenos,
    FxBatidaMais,
    FxProximo,
    DeckAtivo,
    // ---- geral
    Shift,
    Crossfader,
    Master,
    FoneVolume,
    FoneMix,
    BrowseGira,
    BrowseAperta,
    Browser,
    Config,
}

impl Acao {
    /// Pelo nome usado no arquivo.
    pub fn pelo_nome(n: &str, modo: Option<&str>) -> Option<Acao> {
        use Acao::*;
        Some(match n {
            "play" => Play,
            "cue" => Cue,
            "sync" => Sync,
            "pitch_zero" => PitchZero,
            "stutter" => Stutter,
            "inicio" => Inicio,
            "load" => Load,
            "loop" => Loop,
            "loop_sair" => LoopSair,
            "loop_metade" => LoopMetade,
            "loop_dobro" => LoopDobro,
            "loop_in" => LoopIn,
            "loop_out" => LoopOut,
            "loop_auto" => LoopAuto,
            "beatjump" => Beatjump,
            "pfl" => Pfl,
            "vinil" => Vinil,
            "keylock" => Keylock,
            "slip" => Slip,
            "faixa_pitch" => FaixaPitch,
            "volume" => Volume,
            "tempo" => Tempo,
            "trim" => Trim,
            "grave" => Grave,
            "medio" => Medio,
            "agudo" => Agudo,
            "filtro" => Filtro,
            "jog_toque" => JogToque,
            "jog_scratch" => JogScratch,
            "jog_bend" => JogBend,
            "jog_busca" => JogBusca,
            "hotcue" => Hotcue,
            "apagar_hotcue" => ApagarHotcue,
            "pad" => Pad,
            "pad_modo" => PadModo(modo_pelo_nome(modo?)?),
            "fx_liga" => FxLiga,
            "fx_nivel" => FxNivel,
            "fx_batida_menos" => FxBatidaMenos,
            "fx_batida_mais" => FxBatidaMais,
            "fx_proximo" => FxProximo,
            "deck_ativo" => DeckAtivo,
            "shift" => Shift,
            "crossfader" => Crossfader,
            "master" => Master,
            "fone_volume" => FoneVolume,
            "fone_mix" => FoneMix,
            "browse_gira" => BrowseGira,
            "browse_aperta" => BrowseAperta,
            "browser" => Browser,
            "config" => Config,
            _ => return None,
        })
    }

    /// Ação que precisa saber o deck.
    fn de_deck(self) -> bool {
        !matches!(self, Acao::Shift | Acao::Crossfader | Acao::Master | Acao::FoneVolume | Acao::FoneMix | Acao::BrowseGira | Acao::BrowseAperta | Acao::Browser | Acao::Config)
    }

    /// Como o valor costuma chegar para esta ação (o arquivo pode trocar com `valor=`).
    fn valor_padrao(self) -> Valor {
        use Acao::*;
        match self {
            Volume | Tempo | Trim | Grave | Medio | Agudo | Filtro | FxNivel | Crossfader | Master | FoneVolume | FoneMix => Valor::Absoluto,
            JogScratch | JogBend | JogBusca => Valor::Relativo64,
            BrowseGira => Valor::Relativo,
            _ => Valor::Botao,
        }
    }
}

/// Modo dos pads pelo nome usado no arquivo.
pub fn modo_pelo_nome(n: &str) -> Option<PadMode> {
    Some(match n {
        "hotcue" => PadMode::HotCue,
        "loop" => PadMode::AutoLoop,
        "tracking" => PadMode::Tracking,
        "sampler" => PadMode::Sampler,
        "beatjump" => PadMode::BeatJump,
        "roll" => PadMode::Roll,
        "trans" => PadMode::Trans,
        "scratch" => PadMode::ScratchBank,
        _ => return None,
    })
}

/// Uma linha de controle do arquivo.
#[derive(Clone, Debug)]
pub struct Entrada {
    pub tipo: Tipo,
    pub canal: u8,
    pub num: u8,
    pub acao: Acao,
    pub deck: usize,
    /// Número do pad / hot cue (0 a 7) ou do lado (deck_ativo).
    pub n: usize,
    /// Batidas (loop_auto, beatjump).
    pub batidas: f64,
    /// CC com a parte fina do valor (14 bits).
    pub lsb: Option<u8>,
    /// Só vale com SHIFT segurado (controladoras que não mandam outra nota com SHIFT).
    pub shift: bool,
    pub valor: Valor,
    pub invertido: bool,
}

/// Estado que acende um LED.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Estado {
    Tocando,
    Cue,
    Sync,
    Pfl,
    Loop,
    Vinil,
    Keylock,
    Slip,
    Hotcue,
    Pad,
    PadModo(PadMode),
    FxLigado,
    Shift,
}

#[derive(Clone, Debug)]
pub struct Led {
    pub tipo: Tipo,
    pub canal: u8,
    pub num: u8,
    pub estado: Estado,
    pub deck: usize,
    pub n: usize,
    pub ligado: u8,
    pub desligado: u8,
}

/// Um mapeamento inteiro (um arquivo).
#[derive(Clone, Debug, Default)]
pub struct Mapa {
    pub nome: String,
    /// Partes do nome da porta MIDI que identificam a controladora.
    pub portas: Vec<String>,
    /// "testado" ou "em teste" (ninguém conferiu com o aparelho ainda).
    pub situacao: String,
    /// De onde vieram os endereços (documentação, referência).
    pub fonte: String,
    pub entradas: Vec<Entrada>,
    pub leds: Vec<Led>,
    /// Arquivo de onde veio (vazio: veio dentro do programa).
    pub arquivo: Option<PathBuf>,
}

fn numero(s: &str) -> Option<u32> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// Lê um arquivo de mapeamento. O erro diz a linha e o que está errado.
pub fn ler(texto: &str) -> Result<Mapa, String> {
    let mut m = Mapa { situacao: "em teste".into(), ..Default::default() };
    for (i, linha) in texto.lines().enumerate() {
        let erro = |e: &str| format!("linha {}: {e} ({})", i + 1, linha.trim());
        let l = linha.split('#').next().unwrap_or("").trim();
        if l.is_empty() {
            continue;
        }
        if let Some((k, v)) = l.split_once('=').filter(|(k, _)| !k.contains(' ') || matches!(k.trim(), "nome" | "porta" | "situacao" | "situação" | "fonte")) {
            let v = v.trim().to_string();
            match k.trim() {
                "nome" => m.nome = v,
                "porta" => m.portas.push(v.to_lowercase()),
                "situacao" | "situação" => m.situacao = v,
                "fonte" => m.fonte = v,
                _ => return Err(erro("campo desconhecido")),
            }
            continue;
        }
        let mut partes = l.split_whitespace();
        let mut primeiro = partes.next().unwrap_or("");
        let led = primeiro == "led";
        if led {
            primeiro = partes.next().ok_or_else(|| erro("falta o tipo (nota ou cc)"))?;
        }
        let tipo = match primeiro {
            "nota" => Tipo::Nota,
            "cc" => Tipo::Cc,
            "pb" => Tipo::Pb,
            _ => return Err(erro("a linha começa com nota, cc, pb ou led")),
        };
        let canal = partes.next().and_then(numero).filter(|c| (1..=16).contains(c)).ok_or_else(|| erro("canal de 1 a 16"))? as u8 - 1;
        let num = if tipo == Tipo::Pb { 0 } else { partes.next().and_then(numero).filter(|n| *n < 128).ok_or_else(|| erro("número de 0 a 127 (ou 0x00 a 0x7F)"))? as u8 };
        let nome = partes.next().ok_or_else(|| erro("falta a ação"))?;
        let mut deck = None;
        let mut n = 0usize;
        let mut batidas = 1.0;
        let mut lsb = None;
        let mut shift = false;
        let mut valor = None;
        let mut invertido = false;
        let mut modo = None;
        let (mut ligado, mut desligado) = (0x7Fu8, 0x00u8);
        for opt in partes {
            let (k, v) = opt.split_once('=').unwrap_or((opt, ""));
            match k {
                "deck" => deck = Some(numero(v).filter(|d| (1..=NUM_DECKS as u32).contains(d)).ok_or_else(|| erro("deck de 1 a 4"))? as usize - 1),
                "n" => n = numero(v).filter(|x| (1..=8).contains(x)).ok_or_else(|| erro("n de 1 a 8"))? as usize - 1,
                "b" => batidas = v.replace(',', ".").parse::<f64>().ok().or_else(|| v.split_once('/').and_then(|(a, b)| Some(a.parse::<f64>().ok()? / b.parse::<f64>().ok()?))).ok_or_else(|| erro("b= batidas (ex.: 4, -1, 1/2)"))?,
                "lsb" => lsb = Some(numero(v).filter(|x| *x < 128).ok_or_else(|| erro("lsb de 0 a 127"))? as u8),
                "shift" => shift = true,
                "invertido" => invertido = true,
                "modo" => modo = Some(v.to_string()),
                "valor" => {
                    valor = Some(match v {
                        "botao" => Valor::Botao,
                        "absoluto" => Valor::Absoluto,
                        "relativo" => Valor::Relativo,
                        "relativo64" => Valor::Relativo64,
                        _ => return Err(erro("valor= botao, absoluto, relativo ou relativo64")),
                    })
                }
                "ligado" => ligado = numero(v).filter(|x| *x < 128).ok_or_else(|| erro("ligado de 0 a 127"))? as u8,
                "desligado" => desligado = numero(v).filter(|x| *x < 128).ok_or_else(|| erro("desligado de 0 a 127"))? as u8,
                _ => return Err(erro(&format!("opção desconhecida \"{k}\""))),
            }
        }
        if led {
            let estado = match nome {
                "tocando" => Estado::Tocando,
                "cue" => Estado::Cue,
                "sync" => Estado::Sync,
                "pfl" => Estado::Pfl,
                "loop" => Estado::Loop,
                "vinil" => Estado::Vinil,
                "keylock" => Estado::Keylock,
                "slip" => Estado::Slip,
                "hotcue" => Estado::Hotcue,
                "pad" => Estado::Pad,
                "pad_modo" => Estado::PadModo(modo.as_deref().and_then(modo_pelo_nome).ok_or_else(|| erro("pad_modo precisa de modo=hotcue|loop|roll|..."))?),
                "fx_ligado" => Estado::FxLigado,
                "shift" => Estado::Shift,
                _ => return Err(erro(&format!("estado de LED desconhecido \"{nome}\""))),
            };
            if estado != Estado::Shift && deck.is_none() {
                return Err(erro("LED de deck precisa de deck="));
            }
            m.leds.push(Led { tipo, canal, num, estado, deck: deck.unwrap_or(0), n, ligado, desligado });
            continue;
        }
        let acao = Acao::pelo_nome(nome, modo.as_deref()).ok_or_else(|| erro(&format!("ação desconhecida \"{nome}\" (lista em docs/mapeamento.md)")))?;
        if acao.de_deck() && deck.is_none() {
            return Err(erro(&format!("a ação {nome} precisa de deck=")));
        }
        m.entradas.push(Entrada { tipo, canal, num, acao, deck: deck.unwrap_or(0), n, batidas, lsb, shift, valor: valor.unwrap_or(acao.valor_padrao()), invertido });
    }
    if m.nome.is_empty() {
        return Err("falta \"nome = ...\"".into());
    }
    if m.portas.is_empty() {
        return Err("falta \"porta = ...\" (parte do nome da porta MIDI)".into());
    }
    Ok(m)
}

/// Mapeamentos que vêm dentro do programa (pasta `mapeamentos/` do projeto, juntada pelo
/// `build.rs`): (nome do arquivo, texto).
pub const EMBUTIDOS: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/mapeamentos.rs"));

/// Pasta dos mapeamentos da pessoa.
pub fn pasta_usuario() -> PathBuf {
    crate::app::data_dir().join("mapeamentos")
}

/// Todos os mapeamentos: os do programa e os da pessoa (o dela com o mesmo nome vale). Arquivo
/// com erro fica de fora e o erro vai para o registro.
pub fn todos() -> Vec<Arc<Mapa>> {
    let mut v: Vec<Mapa> = Vec::new();
    for (arq, texto) in EMBUTIDOS {
        match ler(texto) {
            Ok(m) => v.push(m),
            Err(e) => eprintln!("[djos] mapeamento {arq}: {e}"),
        }
    }
    let mut proprios: Vec<PathBuf> = std::fs::read_dir(pasta_usuario()).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "djosmap")).collect()).unwrap_or_default();
    proprios.sort();
    for p in proprios {
        match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| ler(&t)) {
            Ok(mut m) => {
                m.arquivo = Some(p);
                v.retain(|x| x.nome != m.nome);
                v.push(m);
            }
            Err(e) => eprintln!("[djos] mapeamento {}: {e}", p.display()),
        }
    }
    v.into_iter().map(Arc::new).collect()
}

impl Mapa {
    /// A porta MIDI é desta controladora?
    pub fn reconhece(&self, porta: &str) -> bool {
        let p = porta.to_lowercase();
        self.portas.iter().any(|x| p.contains(x.as_str()))
    }
}

// ---------------------------------------------------------------------------- tradutor

/// O tradutor de uma controladora mapeada por arquivo.
pub struct Mapeado {
    mapa: Arc<Mapa>,
    app: SharedApp,
    cmd: CmdTx,
    snap: SharedSnap,
    loader: Loader,
    /// Últimas partes (grossa e fina) dos valores de 14 bits: [canal][cc].
    msb: [[u8; 128]; 16],
    lsb: [[u8; 128]; 16],
    shift: bool,
}

impl Mapeado {
    pub fn new(mapa: Arc<Mapa>, app: SharedApp, cmd: CmdTx, snap: SharedSnap, loader: Loader) -> Self {
        Self { mapa, app, cmd, snap, loader, msb: [[0; 128]; 16], lsb: [[0; 128]; 16], shift: false }
    }

    fn snap(&self) -> Snapshot {
        *self.snap.lock()
    }

    /// A entrada desta mensagem (com SHIFT segurado, as marcadas com `shift` primeiro) e se a
    /// mensagem é a parte fina de um valor de 14 bits.
    fn achar(&self, tipo: Tipo, canal: u8, num: u8) -> Option<(Entrada, bool)> {
        let bate = |e: &&Entrada, fina: bool| e.tipo == tipo && e.canal == canal && if fina { e.lsb == Some(num) } else { e.num == num };
        let escolher = |fina: bool| {
            let todos: Vec<&Entrada> = self.mapa.entradas.iter().filter(|e| bate(e, fina)).collect();
            todos.iter().find(|e| e.shift == self.shift).or_else(|| todos.iter().find(|e| !e.shift)).map(|e| (*e).clone())
        };
        escolher(false).map(|e| (e, false)).or_else(|| if tipo == Tipo::Cc { escolher(true).map(|e| (e, true)) } else { None })
    }

    pub fn handle(&mut self, msg: &[u8]) {
        if super::log_left() > 0 && msg.len() >= 3 {
            eprintln!("[midi] {:02X} {:02X} {:02X}", msg[0], msg[1], msg[2]);
        }
        self.trata(msg);
    }

    /// Trata a mensagem se ela está no mapeamento (devolve `false` se não está).
    pub fn trata(&mut self, msg: &[u8]) -> bool {
        if msg.len() < 3 {
            return false;
        }
        let (st, d1, d2) = (msg[0], msg[1], msg[2]);
        let canal = st & 0x0F;
        let (tipo, num, v) = match st & 0xF0 {
            0x90 => (Tipo::Nota, d1, d2),
            0x80 => (Tipo::Nota, d1, 0),
            0xB0 => (Tipo::Cc, d1, d2),
            0xE0 => (Tipo::Pb, 0, d2),
            _ => return false,
        };
        let Some((e, fina)) = self.achar(tipo, canal, num) else { return false };
        // Valor de 0 a 1 (absoluto) e a diferença (encoders).
        let x = match (tipo, e.lsb) {
            (Tipo::Pb, _) => (((d2 as u32) << 7) | d1 as u32) as f32 / 16383.0,
            (_, Some(_)) => {
                let i = e.num as usize;
                if fina {
                    self.lsb[canal as usize][i] = v;
                } else {
                    self.msb[canal as usize][i] = v;
                }
                (((self.msb[canal as usize][i] as u32) << 7) | self.lsb[canal as usize][i] as u32) as f32 / 16383.0
            }
            _ => v as f32 / 127.0,
        };
        let x = if e.invertido { 1.0 - x } else { x };
        let delta = match e.valor {
            Valor::Relativo => {
                if v < 64 {
                    v as f64
                } else {
                    v as f64 - 128.0
                }
            }
            Valor::Relativo64 => v as f64 - 64.0,
            _ => 0.0,
        };
        let down = v > 0;
        self.executar(&e, down, x, delta);
        true
    }

    fn executar(&mut self, e: &Entrada, down: bool, x: f32, delta: f64) {
        use Acao::*;
        let d = e.deck;
        let send = |c: Cmd| self.cmd.send(c);
        match e.acao {
            Play if down => send(Cmd::PlayToggle(d)),
            Cue => send(Cmd::Cue { deck: d, down }),
            Sync if down => send(Cmd::Sync(d)),
            PitchZero if down => send(Cmd::PitchReset(d)),
            Stutter if down => send(Cmd::Stutter(d)),
            Inicio if down => send(Cmd::GotoStart(d)),
            Load if down => carregar(&self.app, &self.loader, d),
            Loop if down => send(Cmd::LoopToggle(d)),
            LoopSair if down => send(Cmd::ReloopExit(d)),
            LoopMetade if down => send(Cmd::LoopHalve(d)),
            LoopDobro if down => send(Cmd::LoopDouble(d)),
            LoopIn if down => send(Cmd::LoopIn(d)),
            LoopOut if down => send(Cmd::LoopOut(d)),
            LoopAuto if down => send(Cmd::LoopAuto { deck: d, beats: e.batidas.abs() }),
            Beatjump if down => send(Cmd::BeatJump { deck: d, beats: e.batidas }),
            Pfl if down => {
                let on = !self.snap().decks[d].pfl;
                send(Cmd::Pfl { deck: d, on });
            }
            Vinil if down => {
                let on = !self.snap().decks[d].vinyl;
                send(Cmd::Vinyl { deck: d, on });
                self.app.lock().toast(format!("Deck {}: {}", d + 1, if on { "VINIL" } else { "CDJ" }));
            }
            Keylock if down => {
                let on = !self.snap().decks[d].keylock;
                send(Cmd::Keylock { deck: d, on });
            }
            Slip if down => {
                let on = !self.snap().decks[d].slip;
                send(Cmd::Slip { deck: d, on });
            }
            FaixaPitch if down => send(Cmd::PitchRangeCycle(d)),
            Volume => send(Cmd::Fader { deck: d, v: x }),
            Tempo => send(Cmd::Tempo { deck: d, v: x as f64 }),
            Trim => send(Cmd::Trim { deck: d, v: x }),
            Grave => send(Cmd::Eq { deck: d, band: 0, v: x }),
            Medio => send(Cmd::Eq { deck: d, band: 1, v: x }),
            Agudo => send(Cmd::Eq { deck: d, band: 2, v: x }),
            Filtro => send(Cmd::Filter { deck: d, v: x }),
            JogToque => send(Cmd::JogTouch { deck: d, down }),
            JogScratch if delta != 0.0 => send(Cmd::JogScratch { deck: d, ticks: delta }),
            JogBend if delta != 0.0 => send(Cmd::JogBend { deck: d, ticks: delta }),
            JogBusca if delta != 0.0 => send(Cmd::JogSearch { deck: d, ticks: delta }),
            Hotcue => send(Cmd::Hotcue { deck: d, idx: e.n, down }),
            ApagarHotcue if down => send(Cmd::HotcueDelete { deck: d, idx: e.n }),
            Pad => {
                let mode = self.app.lock().pad_mode[d];
                let snap = self.snap();
                super::rev1::pad_action(&self.app, &self.cmd, &snap, d, mode, e.n, self.shift, down);
            }
            PadModo(m) if down => self.app.lock().pad_mode[d] = m,
            FxLiga if down => {
                let f = self.snap().decks[d].fx[0];
                if f.level < 0.01 {
                    send(Cmd::FxLevel { deck: d, slot: 0, v: 0.6 });
                }
                send(Cmd::FxActive { deck: d, slot: 0, on: !f.active });
            }
            FxNivel => send(Cmd::FxLevel { deck: d, slot: 0, v: x }),
            FxBatidaMenos | FxBatidaMais if down => {
                let cur = self.snap().decks[d].fx[0];
                let beats = step_beats(cur.beats, if e.acao == FxBatidaMenos { -1 } else { 1 });
                send(Cmd::FxBeats { deck: d, slot: 0, beats });
                self.app.lock().toast(format!("Deck {} · {} {}", d + 1, cur.kind.name(), beats_label(beats)));
            }
            FxProximo if down => {
                let current = self.snap().decks[d].fx[0].kind;
                let mut a = self.app.lock();
                let lado = d % 2;
                let shown = a.fx_pick[lado].map_or(current, |(k, _)| k);
                a.fx_pick[lado] = Some((shown.next(1), Instant::now()));
            }
            DeckAtivo if down => self.app.lock().side[d % 2] = d,
            Shift => {
                self.shift = down;
                self.app.lock().shift = down;
            }
            Crossfader => send(Cmd::Crossfader(x)),
            Master => send(Cmd::MasterLevel(x)),
            FoneVolume => send(Cmd::HeadphoneLevel(x)),
            FoneMix => send(Cmd::HeadphoneMix(x)),
            BrowseGira if delta != 0.0 => browse_gira(&self.app, delta as i32),
            BrowseAperta => browse_aperta(&self.app, &self.cmd, down),
            Browser if down => {
                let mut a = self.app.lock();
                a.big_browser = !a.big_browser;
                if !a.big_browser {
                    crate::search::close(&mut a);
                }
            }
            Config if down => crate::settings::toggle(&mut self.app.lock()),
            _ => {}
        }
    }
}

/// LOAD: numa pasta abre; numa música carrega no deck e volta para as waveforms.
pub fn carregar(app: &SharedApp, loader: &Loader, d: usize) {
    let mut a = app.lock();
    if a.search.is_some() {
        return;
    }
    if a.lib.focus == crate::library::Focus::Folders {
        a.lib.open_selected();
        return;
    }
    if let Some(t) = a.lib.selected_track() {
        loader.load_deck(&mut a, d, t.path);
        a.big_browser = false;
    }
}

/// Girar o BROWSE: configurações, zoom (segurando ou no modo zoom), teclado da pesquisa ou lista.
pub fn browse_gira(app: &SharedApp, d: i32) {
    let mut a = app.lock();
    if a.search.is_none() && a.settings_ui.is_some() {
        crate::settings::scroll(&mut a, d);
        return;
    }
    let segurando = a.browse_down.is_some() && a.search.is_none();
    let modo_zoom = a.settings.browse_gira == 1 && !a.big_browser && a.search.is_none();
    if segurando || modo_zoom {
        if segurando {
            a.browse_long = true;
        }
        let z = a.wave_zoom as i32 - d.signum();
        a.wave_zoom = z.clamp(0, crate::ui::ZOOMS.len() as i32 - 1) as usize;
        return;
    }
    match a.search.as_mut() {
        Some(ui) => ui.step(d),
        None => {
            a.big_browser = true;
            a.lib.scroll(d);
        }
    }
}

/// Apertar o BROWSE: toque curto alterna pastas/músicas (ou aciona o item das configurações);
/// segurar abre a pesquisa (conferido pela tela).
pub fn browse_aperta(app: &SharedApp, cmd: &CmdTx, down: bool) {
    let mut a = app.lock();
    if down {
        a.browse_down = Some(Instant::now());
        a.browse_long = false;
    } else if a.browse_down.is_some() {
        if a.settings_ui.is_some() && a.search.is_none() {
            a.browse_down = None;
            drop(a);
            crate::settings::activate(app, cmd);
        } else {
            a.browse_released();
        }
    }
}

/// LEDs pelo estado de cada deck.
pub fn leds(mapa: &Mapa, app: &crate::app::AppState, snap: &Snapshot, out: &mut Vec<(u8, u8, u8)>) {
    for l in &mapa.leds {
        let s = &snap.decks[l.deck.min(NUM_DECKS - 1)];
        let on = match l.estado {
            Estado::Tocando => s.playing,
            Estado::Cue => s.at_cue || s.cue_preview,
            Estado::Sync => s.sync_ok,
            Estado::Pfl => s.pfl,
            Estado::Loop => s.loop_on,
            Estado::Vinil => s.vinyl,
            Estado::Keylock => s.keylock,
            Estado::Slip => s.slip,
            Estado::Hotcue => s.hotcues.get(l.n).is_some_and(|h| h.is_some()),
            Estado::Pad => match app.pad_mode[l.deck] {
                PadMode::HotCue => s.hotcues.get(l.n).is_some_and(|h| h.is_some()),
                _ => false,
            },
            Estado::PadModo(m) => app.pad_mode[l.deck] == m,
            Estado::FxLigado => s.fx[0].active,
            Estado::Shift => app.shift,
        };
        let status = match l.tipo {
            Tipo::Nota => 0x90,
            Tipo::Cc => 0xB0,
            Tipo::Pb => continue,
        } | l.canal;
        out.push((status, l.num, if on { l.ligado } else { l.desligado }));
    }
}

impl super::Controller for Mapeado {
    fn handle(&mut self, msg: &[u8]) {
        Mapeado::handle(self, msg);
    }

    fn leds(&self, app: &crate::app::AppState, snap: &Snapshot, _blink: bool, out: &mut Vec<(u8, u8, u8)>) {
        leds(&self.mapa, app, snap, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXEMPLO: &str = "\
# Exemplo
nome = Teste DDJ
porta = DDJ-TESTE
nota 1 0x0B play deck=1
nota 1 0x0C cue deck=1
cc 1 0x13 volume deck=1 lsb=0x33
cc 1 0x22 jog_scratch deck=1
nota 1 0x00 pad deck=1 n=1
nota 7 0x3F shift
nota 1 0x0B pitch_zero deck=1 shift
cc 7 0x40 browse_gira
nota 1 0x14 loop_auto deck=1 b=4
pb 2 tempo deck=2 invertido
led nota 1 0x0B tocando deck=1
led nota 1 0x00 hotcue deck=1 n=1 ligado=0x7F desligado=0x01
";

    #[test]
    fn le_o_arquivo() {
        let m = ler(EXEMPLO).unwrap();
        assert_eq!(m.nome, "Teste DDJ");
        assert!(m.reconhece("DDJ-TESTE:DDJ-TESTE MIDI 1 24:0"));
        assert_eq!(m.entradas.len(), 10);
        assert_eq!(m.leds.len(), 2);
        let vol = &m.entradas[2];
        assert_eq!((vol.acao, vol.lsb, vol.valor), (Acao::Volume, Some(0x33), Valor::Absoluto));
        assert_eq!(m.entradas[3].valor, Valor::Relativo64, "jog: meio em 64");
        assert_eq!(m.entradas[7].valor, Valor::Relativo, "browse: complemento de dois");
        assert_eq!(m.entradas[8].batidas, 4.0);
        assert_eq!(m.situacao, "em teste");
    }

    #[test]
    fn erros_dizem_a_linha() {
        let e = ler("nome = X\nporta = X\nnota 1 0x0B voar deck=1\n").unwrap_err();
        assert!(e.starts_with("linha 3:") && e.contains("voar"), "{e}");
        assert!(ler("nome = X\nporta = X\nnota 1 0x0B play\n").unwrap_err().contains("deck="));
        assert!(ler("nome = X\nporta = X\nnota 17 0x0B play deck=1\n").unwrap_err().contains("canal"));
        assert!(ler("porta = X\n").unwrap_err().contains("nome"));
    }

    #[test]
    fn uma_controladora_nao_pega_a_porta_de_outra() {
        let mapas: Vec<Mapa> = EMBUTIDOS.iter().map(|(_, t)| ler(t).unwrap()).collect();
        for a in &mapas {
            assert!(!a.reconhece("DDJ-REV1:DDJ-REV1 MIDI 1 28:0"), "{} pega a porta da REV1", a.nome);
            for b in mapas.iter().filter(|b| b.nome != a.nome) {
                for p in &b.portas {
                    // A porta de b (o nome que ela teria) não pode ser reconhecida por a.
                    assert!(!a.reconhece(p), "{} pega a porta de {} ({p})", a.nome, b.nome);
                }
            }
        }
    }

    #[test]
    fn embutidos_estao_certos() {
        for (arq, texto) in EMBUTIDOS {
            let m = ler(texto).unwrap_or_else(|e| panic!("{arq}: {e}"));
            assert!(!m.entradas.is_empty(), "{arq}: sem controles");
            assert!(m.entradas.iter().any(|e| e.acao == Acao::Play), "{arq}: sem play");
        }
    }
}
