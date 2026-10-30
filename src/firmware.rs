//! Firmware: versão do sistema, atualização pelo pendrive, reiniciar e desligar.
//!
//! Tudo passa pela ferramenta `/usr/local/sbin/dj-os-firmware` (chamada com `sudo -n`; no
//! aparelho só ela é liberada). Fora do aparelho (no PC de desenvolvimento) o sudo recusa e a
//! tela mostra que isso só funciona no aparelho.

use crate::app::SharedApp;
use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

const TOOL: &str = "/usr/local/sbin/dj-os-firmware";

#[derive(Default)]
pub struct FwState {
    /// "Versão X · instalado…" ou "modo de teste".
    pub info: String,
    /// Arquivos de firmware achados nos pendrives.
    pub files: Vec<PathBuf>,
    /// Andamento/resultado da última ação.
    pub msg: String,
    pub busy: bool,
}

fn run(args: &[&str]) -> Option<String> {
    let out = Command::new("sudo").arg("-n").arg(TOOL).args(args).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

/// "0.1.0-20261029.2128" → "0.1.0 · firmware de 29/10/2026 21:28" (a data é quando foi montado).
pub fn pretty_version(v: &str) -> String {
    let Some((base, stamp)) = v.split_once('-') else { return v.to_string() };
    let d: Vec<char> = stamp.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.len() != 12 {
        return v.to_string();
    }
    let s = |a: usize, b: usize| d[a..b].iter().collect::<String>();
    format!("{base} · firmware de {}/{}/{} {}:{}", s(6, 8), s(4, 6), s(0, 4), s(8, 10), s(10, 12))
}

/// Lê a versão e o modo (instalado ou teste).
pub fn refresh_info(shared: &SharedApp) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        let info = match run(&["info"]) {
            None => format!("DJ OS {} · desenvolvimento (firmware só no aparelho)", env!("CARGO_PKG_VERSION")),
            Some(out) => {
                let get = |k: &str| out.lines().find_map(|l| l.strip_prefix(k).map(|v| v.trim().to_string()));
                let versao = pretty_version(&get("VERSAO").unwrap_or_default());
                match get("SLOT") {
                    None => format!("Versão {versao} · modo de teste (ISO): nada é guardado ao desligar"),
                    Some(slot) => {
                        let reserva = get("OUTRA").map_or(String::new(), |o| format!(" · reserva: {}", pretty_version(&o)));
                        format!("Versão {versao} · instalado (parte {}){reserva}", slot.to_uppercase())
                    }
                }
            }
        };
        shared.lock().fw.info = info;
    });
}

/// Procura arquivos de firmware (.djos) nos pendrives.
pub fn search(shared: &SharedApp) {
    shared.lock().fw.msg = "procurando no pendrive…".into();
    let shared = shared.clone();
    std::thread::spawn(move || {
        let files: Vec<PathBuf> = run(&["procurar"]).unwrap_or_default().lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect();
        let mut a = shared.lock();
        a.fw.msg = if files.is_empty() { "nenhum arquivo .djos nos pendrives".into() } else { format!("{} encontrado(s)", files.len()) };
        a.fw.files = files;
    });
}

/// Grava o firmware na versão que não está em uso e reinicia. A ferramenta confere a assinatura.
pub fn install(shared: &SharedApp, file: PathBuf) {
    {
        let mut a = shared.lock();
        if a.fw.busy {
            return;
        }
        a.fw.busy = true;
        a.fw.msg = "começando…".into();
    }
    let shared = shared.clone();
    std::thread::spawn(move || {
        let child = Command::new("sudo").arg("-n").arg(TOOL).arg("atualizar").arg(&file).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
        let Ok(mut child) = child else {
            let mut a = shared.lock();
            a.fw.msg = "só funciona no aparelho".into();
            a.fw.busy = false;
            return;
        };
        let mut ok = None;
        let mut versao = String::new();
        if let Some(out) = child.stdout.take() {
            for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                let mut a = shared.lock();
                if let Some(p) = line.strip_prefix("PASSO ") {
                    a.fw.msg = p.to_string();
                } else if let Some(n) = line.strip_prefix("PROGRESSO ") {
                    a.fw.msg = format!("gravando {}%", n.trim());
                } else if let Some(v) = line.strip_prefix("VERSAO ") {
                    versao = v.to_string();
                } else if let Some(v) = line.strip_prefix("OK ") {
                    ok = Some(v.to_string());
                } else if let Some(w) = line.strip_prefix("AVISO ") {
                    a.toast(format!("Atenção: {w}"));
                } else if let Some(e) = line.strip_prefix("ERRO ") {
                    eprintln!("[djos] atualização de firmware: ERRO {e}");
                    a.fw.msg = format!("ERRO: {e}");
                }
            }
        }
        let _ = child.wait();
        match ok {
            Some(v) => {
                {
                    let mut a = shared.lock();
                    a.fw.msg = format!("versão {v} gravada · reiniciando…");
                    a.toast(format!("Firmware {v} gravado. Reiniciando em 5 s…"));
                }
                std::thread::sleep(Duration::from_secs(5));
                let _ = run(&["reiniciar"]);
            }
            None => {
                let mut a = shared.lock();
                if !a.fw.msg.starts_with("ERRO") {
                    a.fw.msg = format!("ERRO: a atualização {versao} parou");
                }
                a.fw.busy = false;
            }
        }
    });
}

/// Depois de 30 s rodando, confirma a versão em teste (se houver). Se o boot voltou para a versão
/// anterior porque a nova não ligou, avisa.
pub fn confirm_later(shared: &SharedApp) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(30));
        let out = run(&["confirmar"]).unwrap_or_default();
        let mut a = shared.lock();
        // O aviso fica também na seção SISTEMA (o toast some em 3 s).
        if let Some(v) = out.lines().find_map(|l| l.strip_prefix("CONFIRMADO ")) {
            a.fw.msg = format!("versão {v} instalada com sucesso");
            a.toast(format!("Firmware {v} instalado com sucesso"));
        } else if let Some(v) = out.lines().find_map(|l| l.strip_prefix("FALHOU")) {
            a.fw.msg = format!("a atualização{v} não ligou: voltamos para a anterior");
            a.toast(format!("A atualização{v} não ligou direito: voltamos para a versão anterior"));
        }
    });
}

/// "reiniciar" ou "desligar".
pub fn power(shared: &SharedApp, what: &'static str) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        if run(&[what]).is_none() {
            shared.lock().toast("Reiniciar/desligar só funciona no aparelho".to_string());
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn versao_legivel() {
        assert_eq!(super::pretty_version("0.1.0-20261029.2128"), "0.1.0 · firmware de 29/10/2026 21:28");
        assert_eq!(super::pretty_version("0.1.0"), "0.1.0");
    }
}
