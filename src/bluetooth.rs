//! Saída Bluetooth simultânea: o master ou o fone (retorno) vai também para uma caixa Bluetooth,
//! ao mesmo tempo que a REV1.
//!
//! A caixa precisa estar pareada e conectada no sistema (o pareamento entra na tela de
//! configurações). A cada 5 s este gerenciador confere se há uma caixa e abre/fecha o fluxo.
//! `DJOS_BT_TARGET=<nó do PipeWire>` força outro destino (usado nos testes sem caixa).

use crate::app::SharedApp;
use crate::shared::{Cmd, CmdTx};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

// ---- liga/desliga o adaptador ----

/// O sistema ligou o adaptador (e se ele estava bloqueado antes), para devolver como estava.
static TURNED_ON: AtomicBool = AtomicBool::new(false);
static WAS_BLOCKED: AtomicBool = AtomicBool::new(false);
/// Busca/pareamento em andamento: não desliga o adaptador até este instante (segundos Unix).
static HOLD_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Liga o adaptador e o mantém ligado por `secs` (busca de caixas, pareamento).
pub fn hold_on(secs: u64) -> Result<(), String> {
    HOLD_UNTIL.store(now_secs() + secs, Ordering::Relaxed);
    ensure_on()
}

fn run(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
}

fn powered() -> bool {
    run("bluetoothctl", &["show"]).contains("Powered: yes")
}

/// Bloqueio do rfkill: (por software, por chave do aparelho).
fn rfkill_blocked() -> (bool, bool) {
    let out = run("rfkill", &["list", "bluetooth"]);
    (out.contains("Soft blocked: yes"), out.contains("Hard blocked: yes"))
}

/// Liga o Bluetooth (tirando o bloqueio do rfkill, se houver) e confere se ligou de verdade.
pub fn ensure_on() -> Result<(), String> {
    if powered() {
        return Ok(());
    }
    let (soft, hard) = rfkill_blocked();
    if hard {
        return Err("desligado na chave/botão do aparelho".into());
    }
    if soft {
        let _ = Command::new("rfkill").args(["unblock", "bluetooth"]).status();
    }
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(5) {
        if run("bluetoothctl", &["show"]).contains("No default controller") {
            std::thread::sleep(Duration::from_millis(300)); // adaptador voltando depois do rfkill
            continue;
        }
        let _ = Command::new("bluetoothctl").args(["power", "on"]).output();
        if powered() {
            if !TURNED_ON.swap(true, Ordering::Relaxed) {
                WAS_BLOCKED.store(soft, Ordering::Relaxed);
            }
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(match (run("bluetoothctl", &["show"]).contains("No default controller"), rfkill_blocked().0) {
        (true, _) => "nenhum adaptador Bluetooth encontrado".into(),
        (_, true) => "bloqueado pelo sistema (rfkill)".into(),
        _ => "o adaptador não ligou".into(),
    })
}

/// Nada mais usa o Bluetooth: se fomos nós que ligamos, desliga e bloqueia de novo como estava.
fn release() {
    if TURNED_ON.swap(false, Ordering::Relaxed) {
        let _ = Command::new("bluetoothctl").args(["power", "off"]).output();
        if WAS_BLOCKED.load(Ordering::Relaxed) {
            let _ = Command::new("rfkill").args(["block", "bluetooth"]).status();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BtMode {
    #[default]
    Off,
    Master,
    Retorno,
}

impl BtMode {
    pub fn name(self) -> &'static str {
        match self {
            BtMode::Off => "DESLIGADO",
            BtMode::Master => "MASTER",
            BtMode::Retorno => "RETORNO",
        }
    }

    pub fn next(self) -> BtMode {
        match self {
            BtMode::Off => BtMode::Master,
            BtMode::Master => BtMode::Retorno,
            BtMode::Retorno => BtMode::Off,
        }
    }

    fn source(self) -> u8 {
        if self == BtMode::Retorno {
            2
        } else {
            1
        }
    }
}

/// Caixa Bluetooth conectada (nó de saída "bluez_output…" no PipeWire).
fn find_speaker() -> Option<String> {
    if let Ok(t) = std::env::var("DJOS_BT_TARGET") {
        return Some(t);
    }
    crate::pw_out::find_sinks("bluez_output").into_iter().next()
}

pub fn start(app: SharedApp, cmd: CmdTx) {
    std::thread::Builder::new()
        .name("bluetooth".into())
        .spawn(move || {
            let mut open: Option<(crate::pw_out::Stream, String, BtMode)> = None;
            loop {
                let (mode, delay, phone) = {
                    let a = app.lock();
                    (a.bt_mode, a.bt_delay_ms, a.settings.phone_in != 0)
                };
                // Adaptador ligado enquanto a caixa ou o celular estiverem em uso.
                let held = now_secs() < HOLD_UNTIL.load(Ordering::Relaxed);
                let power = if mode != BtMode::Off || phone { ensure_on() } else if held { Ok(()) } else {
                    release();
                    Ok(())
                };
                if let Err(e) = &power {
                    if mode != BtMode::Off {
                        app.lock().bt_status = format!("BT {} · {e}", mode.name());
                        std::thread::sleep(Duration::from_secs(5));
                        continue;
                    }
                }
                let speaker = if mode == BtMode::Off { None } else { find_speaker() };
                let status;
                match (&mut open, speaker) {
                    // Continua tocando na mesma caixa: só acompanha a troca master/retorno.
                    (Some((_, name, m)), Some(s)) if *name == s => {
                        if *m != mode {
                            cmd.send(Cmd::BtSource(mode.source()));
                            *m = mode;
                        }
                        status = format!("BT {} · {}", mode.name(), short(name));
                    }
                    (_, speaker) => {
                        if let Some((stream, _, _)) = open.take() {
                            cmd.send(Cmd::BtDetach);
                            stream.close();
                        }
                        status = match speaker {
                            None if mode == BtMode::Off => "BT desligado".to_string(),
                            None => format!("BT {} · nenhuma caixa conectada", mode.name()),
                            Some(s) => {
                                let (tx, rx) = rtrb::RingBuffer::new(16384);
                                cmd.send(Cmd::BtSource(mode.source()));
                                cmd.send(Cmd::BtAttach(tx));
                                match crate::pw_out::start_bluetooth(&s, rx) {
                                    Ok(stream) => {
                                        eprintln!("[djos] Bluetooth ({}): {s}", mode.name());
                                        let st = format!("BT {} · {}", mode.name(), short(&s));
                                        open = Some((stream, s, mode));
                                        st
                                    }
                                    Err(e) => {
                                        cmd.send(Cmd::BtDetach);
                                        format!("BT: não abriu ({e})")
                                    }
                                }
                            }
                        };
                    }
                }
                // Atraso do master com fio só faz sentido com a caixa tocando o master.
                let d = if open.as_ref().is_some_and(|o| o.2 == BtMode::Master) { delay } else { 0.0 };
                cmd.send(Cmd::MasterDelay(d));
                app.lock().bt_status = status;
                std::thread::sleep(Duration::from_secs(5));
            }
        })
        .expect("thread do bluetooth");
}

/// Nome curto da caixa a partir do nó ("bluez_output.AA_BB_CC….1" → "AA:BB:CC…").
fn short(node: &str) -> String {
    let s = node.strip_prefix("bluez_output.").unwrap_or(node);
    let s = s.split('.').next().unwrap_or(s).replace('_', ":");
    s.chars().take(17).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modos_e_nome_curto() {
        assert_eq!(BtMode::Off.next(), BtMode::Master);
        assert_eq!(BtMode::Retorno.next(), BtMode::Off);
        assert_eq!(BtMode::Retorno.source(), 2);
        assert_eq!(short("bluez_output.11_22_33_44_55_66.1"), "11:22:33:44:55:66");
    }
}
