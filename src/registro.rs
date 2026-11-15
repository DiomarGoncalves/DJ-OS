//! Caixa preta: a cada minuto anota no registro da sessão o processador (total e quanto cada
//! parte do DJ OS gastou), a memória, a temperatura, a controladora, a saída de som e o que cada
//! deck está fazendo. Junto com a linha do som (`pw_out`) e a da temperatura (a cada 15 s, pelo
//! sistema), dá para ver depois de um show o que aconteceu minuto a minuto.
//!
//! O item "Copiar registros para o pendrive" (aba FUNÇÕES) leva tudo para
//! `<pendrive>/DJOS-registros/<data e hora>/`.

use crate::app::SharedApp;
use crate::shared::{SharedSnap, Snapshot};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Liga a caixa preta (uma thread que acorda a cada minuto).
pub fn iniciar(app: SharedApp, snap: SharedSnap) {
    let _ = std::thread::Builder::new().name("caixa-preta".into()).spawn(move || {
        let mut antes = Medida::agora();
        loop {
            std::thread::sleep(Duration::from_secs(60));
            let agora = Medida::agora();
            let s = *snap.lock();
            eprintln!("{}", linha(&app, s, &antes, &agora));
            antes = agora;
        }
    });
}

/// Tempo de processador gasto até agora (total da máquina e por thread do DJ OS).
struct Medida {
    quando: Instant,
    /// (ocupado, total) da máquina, em tiques.
    maquina: Option<(u64, u64)>,
    /// Tiques gastos pelo DJ OS inteiro (inclui as threads que já fecharam).
    processo: Option<u64>,
    /// Tiques gastos por nome de thread (threads com o mesmo nome somam).
    threads: HashMap<String, u64>,
}

impl Medida {
    fn agora() -> Self {
        Self { quando: Instant::now(), maquina: cpu_maquina(), processo: tiques(Path::new("/proc/self/stat")).map(|t| t.1), threads: cpu_threads() }
    }
}

/// /proc/stat: tiques ocupados e totais de todos os núcleos somados.
fn cpu_maquina() -> Option<(u64, u64)> {
    let s = std::fs::read_to_string("/proc/stat").ok()?;
    let v: Vec<u64> = s.lines().next()?.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 4 {
        return None;
    }
    let total: u64 = v.iter().sum();
    let parado = v[3] + v.get(4).copied().unwrap_or(0); // idle + iowait
    Some((total - parado, total))
}

/// Nome e tiques (usuário + sistema) de um arquivo `stat` do /proc.
fn tiques(arq: &Path) -> Option<(String, u64)> {
    let s = std::fs::read_to_string(arq).ok()?;
    // "id (nome) estado ...": o nome pode ter espaços, então corta no último ')'.
    let (a, b) = (s.find('(')?, s.rfind(')')?);
    let campos: Vec<&str> = s[b + 1..].split_whitespace().collect();
    // Depois do nome: estado é o campo 0; utime e stime são os campos 11 e 12.
    let u = campos.get(11)?.parse::<u64>().ok()?;
    let k = campos.get(12)?.parse::<u64>().ok()?;
    Some((s[a + 1..b].to_string(), u + k))
}

/// /proc/self/task/*/stat: tiques de cada thread viva, somados por nome.
fn cpu_threads() -> HashMap<String, u64> {
    let mut m = HashMap::new();
    let Ok(rd) = std::fs::read_dir("/proc/self/task") else { return m };
    for t in rd.flatten() {
        if let Some((nome, t)) = tiques(&t.path().join("stat")) {
            *m.entry(nome).or_insert(0) += t;
        }
    }
    m
}

/// Memória do DJ OS (MB) e a livre da máquina (MB).
fn memoria() -> (Option<u64>, Option<u64>) {
    let kb = |arq: &str, chave: &str| {
        std::fs::read_to_string(arq).ok()?.lines().find_map(|l| l.strip_prefix(chave)).and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok()).map(|k| k / 1024)
    };
    (kb("/proc/self/status", "VmRSS:"), kb("/proc/meminfo", "MemAvailable:"))
}

/// Uma linha da caixa preta, comparando com a medida anterior.
fn linha(app: &SharedApp, s: Snapshot, antes: &Medida, agora: &Medida) -> String {
    let segs = agora.quando.duration_since(antes.quando).as_secs_f64().max(0.001);
    let mut partes = Vec::new();
    // Processador da máquina toda (100% = todos os núcleos ocupados).
    if let (Some((o0, t0)), Some((o1, t1))) = (antes.maquina, agora.maquina) {
        if t1 > t0 {
            partes.push(format!("processador {:.0}%", (o1 - o0) as f64 / (t1 - t0) as f64 * 100.0));
        }
    }
    // Cada parte do DJ OS (100% = um núcleo inteiro). Só as que gastaram alguma coisa.
    let tique = 100.0; // tiques por segundo no Linux (USER_HZ)
    let mut gasto: Vec<(String, f64)> = agora
        .threads
        .iter()
        .map(|(n, t)| (n.clone(), t.saturating_sub(*antes.threads.get(n).unwrap_or(&0)) as f64 / tique / segs * 100.0))
        .filter(|(_, p)| *p >= 0.5)
        .collect();
    let vivas: f64 = agora.threads.iter().map(|(n, t)| t.saturating_sub(*antes.threads.get(n).unwrap_or(&0)) as f64).sum::<f64>() / tique / segs * 100.0;
    // O total do processo inclui as threads que abriram e fecharam no meio do minuto (abrir uma
    // música no LOAD, analisar): o que sobra depois das vivas é delas.
    let total = match (antes.processo, agora.processo) {
        (Some(a), Some(b)) => b.saturating_sub(a) as f64 / tique / segs * 100.0,
        _ => vivas,
    };
    if total - vivas >= 0.5 {
        gasto.push(("threads que já fecharam".into(), total - vivas));
    }
    gasto.sort_by(|a, b| b.1.total_cmp(&a.1));
    if total >= 0.5 || !gasto.is_empty() {
        let lista: Vec<String> = gasto.iter().take(6).map(|(n, p)| format!("{n} {p:.0}%")).collect();
        partes.push(format!("dj-os {total:.0}% ({})", lista.join(", ")));
    }
    let (rss, livre) = memoria();
    partes.push(format!("memória dj-os {} MB, livre {} MB", rss.map_or("?".into(), |v| v.to_string()), livre.map_or("?".into(), |v| v.to_string())));
    let (t, mhz) = crate::pw_out::temperatura_e_mhz();
    partes.push(format!("{} · {}", t.map_or("temperatura ?".into(), |t| format!("{t} °C")), mhz.map_or("? MHz".into(), |m| format!("{m} MHz"))));
    let a = app.lock();
    partes.push(format!("controladora: {} · saída: {}", a.midi_status, a.audio_status));
    for (d, ds) in s.decks.iter().enumerate() {
        if !ds.loaded {
            continue;
        }
        let nome = a.decks.get(d).and_then(|x| x.track.as_ref()).map(|t| t.display_name()).unwrap_or_default();
        let mut e = vec![if ds.playing { "tocando" } else { "parado" }.to_string()];
        if ds.playing && ds.no_ar {
            e.push("no ar".into());
        }
        e.push(format!("{:.1} BPM", ds.bpm));
        if (ds.rate - 1.0).abs() > 0.0005 {
            e.push(format!("pitch {:+.1}%", (ds.rate - 1.0) * 100.0));
        }
        if ds.keylock {
            e.push("keylock".into());
        }
        let fx: Vec<String> = ds.fx.iter().filter(|f| f.active).map(|f| format!("{:?}", f.kind)).collect();
        if !fx.is_empty() {
            e.push(format!("efeito {}", fx.join("+")));
        }
        if ds.loop_on {
            e.push("loop".into());
        }
        partes.push(format!("deck {}: {} ({nome})", d + 1, e.join(" ")));
    }
    format!("[djos] minuto: {}", partes.join(" · "))
}

/// Copia os registros para o primeiro pendrive (em segundo plano; o resultado aparece na tela).
pub fn copiar_para_pendrive(app: &SharedApp) {
    let Some(drive) = crate::record::first_pendrive() else {
        app.lock().toast("Coloque um pendrive para copiar os registros");
        return;
    };
    app.lock().toast("Copiando os registros para o pendrive…");
    // Anota o estado de agora, para o registro copiado terminar com ele.
    let m = Medida::agora();
    let s = app.lock().snap_tela;
    eprintln!("{}", linha(app, s, &m, &m));
    eprintln!("[djos] copiando os registros para o pendrive {}", drive.display());
    let app = app.clone();
    std::thread::spawn(move || {
        crate::import::low_priority();
        let msg = match copiar(&drive) {
            Ok(pasta) => format!("Registros copiados: pasta {pasta} do pendrive"),
            Err(e) => format!("Não copiou os registros: {e}"),
        };
        eprintln!("[djos] {msg}");
        app.lock().toast(msg);
    });
}

fn copiar(drive: &Path) -> Result<String, String> {
    if crate::app::aparelho() {
        // No aparelho o pendrive é só leitura e o registro do sistema é do administrador: quem
        // copia é a ferramenta dj-os-pendrive (pelo sudo).
        let out = std::process::Command::new("sudo")
            .args(["-n", "/usr/local/sbin/dj-os-pendrive", "registros"])
            .arg(drive)
            .output()
            .map_err(|e| e.to_string())?;
        let texto = String::from_utf8_lossy(&out.stdout);
        let ultima = texto.lines().last().unwrap_or("").trim();
        return match ultima.strip_prefix("OK ") {
            Some(pasta) => Ok(pasta.to_string()),
            None => Err(ultima.strip_prefix("ERRO ").unwrap_or("falhou").to_string()),
        };
    }
    // No PC: os arquivos do DJ OS que existirem (o registro fica no terminal).
    let pasta = format!("DJOS-registros/{}", chrono::Local::now().format("%Y-%m-%d_%H%M%S"));
    let destino: PathBuf = drive.join(&pasta);
    std::fs::create_dir_all(&destino).map_err(|e| format!("pendrive não aceita gravação ({e})"))?;
    let dados = crate::app::data_dir();
    for nome in ["sessao.log", "sessao.log.1", "config.txt"] {
        let arq = dados.join(nome);
        if arq.is_file() {
            std::fs::copy(&arq, destino.join(nome)).map_err(|e| format!("pendrive cheio ou retirado ({e})"))?;
        }
    }
    Ok(pasta)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A linha da caixa preta mostra o processador por parte do DJ OS, a memória e os decks.
    #[test]
    fn linha_da_caixa_preta() {
        let app: SharedApp = std::sync::Arc::new(parking_lot::Mutex::new(crate::app::AppState::new()));
        let antes = Medida::agora();
        // Gasta um pouco de processador numa thread com nome, para ela aparecer na conta.
        std::thread::Builder::new()
            .name("teste-gasta".into())
            .spawn(|| {
                let t = Instant::now();
                let mut x = 0u64;
                while t.elapsed() < Duration::from_millis(300) {
                    x = std::hint::black_box(x.wrapping_mul(31).wrapping_add(7));
                }
            })
            .unwrap()
            .join()
            .unwrap();
        let mut s = Snapshot::default();
        s.decks[0].loaded = true;
        s.decks[0].playing = true;
        s.decks[0].no_ar = true;
        s.decks[0].bpm = 128.0;
        s.decks[0].rate = 1.02;
        s.decks[0].keylock = true;
        s.decks[0].fx[0].active = true;
        let l = linha(&app, s, &antes, &Medida::agora());
        println!("{l}");
        assert!(l.starts_with("[djos] minuto: "));
        assert!(l.contains("deck 1: tocando no ar 128.0 BPM pitch +2.0% keylock efeito"), "{l}");
        assert!(!l.contains("deck 2"), "deck vazio fica de fora: {l}");
        #[cfg(target_os = "linux")]
        {
            assert!(l.contains("processador "), "{l}");
            assert!(l.contains("threads que já fecharam"), "a thread que gastou e fechou entra na conta: {l}");
            assert!(l.contains("memória dj-os ") && !l.contains("memória dj-os ?"), "{l}");
        }
    }
}
