//! Celular por Bluetooth: o sistema vira uma caixa de som "DJ OS". O som que o celular
//! manda (qualquer app) entra no motor num deck (no lugar da música, com EQ, filtro, efeito e
//! fader) ou no master como som ambiente.
//!
//! Como funciona: o `bluetoothctl` deixa o adaptador visível e aceita o pareamento sozinho; o
//! PipeWire cria um fluxo para o celular conectado, e nós o mandamos (pela metadata
//! `target.object` do WirePlumber) para a saída virtual "DJ OS (celular)", que entrega o som
//! na fila do motor.

use crate::app::SharedApp;
use crate::shared::{Cmd, CmdTx};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

/// Para onde vai o som do celular (índice salvo nas configurações).
pub const ROUTES: [&str; 4] = ["DESLIGADO", "DECK 4", "DECK 3", "MASTER (ambiente)"];

/// Destino no motor: deck (0-3) ou `None` = master.
pub fn route_target(route: usize) -> Option<usize> {
    match route {
        1 => Some(3),
        2 => Some(2),
        _ => None,
    }
}

/// Bluetooth do PC visível e aceitando pareamento enquanto a opção estiver ligada.
struct Agent {
    child: Child,
    stdin: Arc<parking_lot::Mutex<ChildStdin>>,
}

fn send(stdin: &parking_lot::Mutex<ChildStdin>, line: &str) {
    let mut s = stdin.lock();
    let _ = s.write_all(format!("{line}\n").as_bytes());
    let _ = s.flush();
}

/// Endereço do aparelho numa linha do bluetoothctl ("Device AA:BB:CC:DD:EE:FF Connected: yes").
fn device_mac(line: &str) -> Option<&str> {
    let i = line.find("Device ")? + 7;
    let mac = line.get(i..i + 17)?;
    (mac.len() == 17 && mac.chars().filter(|c| *c == ':').count() == 5).then_some(mac)
}

impl Agent {
    fn start() -> Result<Agent, String> {
        crate::bluetooth::ensure_on()?;
        let mut child = Command::new("bluetoothctl")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("bluetoothctl não abriu: {e}"))?;
        let stdin = Arc::new(parking_lot::Mutex::new(child.stdin.take().expect("stdin")));
        // O bluetoothctl já abre com um agente que pede confirmação de código: troca pelo que aceita
        // sem perguntar. O nome vai entre aspas (com espaço, o comando recusa).
        for c in ["agent off", "agent NoInputNoOutput", "default-agent", "system-alias \"DJ OS\"", "pairable-timeout 180", "pairable on", "discoverable-timeout 180", "discoverable on"] {
            send(&stdin, c);
            std::thread::sleep(Duration::from_millis(150));
        }
        // Responde "sim" às perguntas de pareamento/autorização e confia no celular conectado.
        if let Some(mut out) = child.stdout.take() {
            let stdin = stdin.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 2048];
                let mut text = String::new();
                while let Ok(n) = out.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    text.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if text.contains("(yes/no)") {
                        send(&stdin, "yes");
                        text.clear();
                        continue;
                    }
                    while let Some(nl) = text.find('\n') {
                        let line: String = text.drain(..=nl).collect();
                        if line.contains("Connected: yes") || line.contains("Paired: yes") || line.contains("Bonded: yes") {
                            if let Some(mac) = device_mac(&line) {
                                send(&stdin, &format!("trust {mac}"));
                            }
                        }
                    }
                }
            });
        }
        Ok(Agent { child, stdin })
    }

    /// Para de aceitar celulares. Desligar o adaptador fica com o gerente do Bluetooth.
    /// Renova a visibilidade (ela vence em 3 min). Se o sistema fechar de repente, o PC deixa de
    /// ficar visível sozinho em vez de ficar aberto para sempre.
    fn renew(&self) {
        send(&self.stdin, "pairable on");
        send(&self.stdin, "discoverable on");
    }

    fn stop(mut self) {
        for c in ["discoverable off", "pairable off", "reset-alias"] {
            send(&self.stdin, c);
            std::thread::sleep(Duration::from_millis(150));
        }
        send(&self.stdin, "quit");
        std::thread::sleep(Duration::from_millis(200));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Mantém o Bluetooth de entrada funcionando enquanto a opção estiver ligada.
pub fn start(app: SharedApp, cmd: CmdTx) {
    std::thread::Builder::new()
        .name("celular".into())
        .spawn(move || {
            let mut agent: Option<Agent> = None;
            let mut sink: Option<crate::pw_out::Stream> = None;
            let mut moved: HashSet<u64> = HashSet::new();
            let mut last_renew = std::time::Instant::now();
            loop {
                let route = app.lock().settings.phone_in;
                if route == 0 {
                    // Desfaz o desvio com o celular ainda conectado (senão o WirePlumber guarda a regra).
                    for id in moved.drain() {
                        let _ = Command::new("pw-metadata").args(["--", &id.to_string(), "target.node", "-1", "Spa:Id"]).stdout(Stdio::null()).status();
                    }
                    if let Some(s) = sink.take() {
                        cmd.send(Cmd::LineInDetach);
                        s.close();
                    }
                    if let Some(ag) = agent.take() {
                        ag.stop();
                        app.lock().phone_status = "desligado".into();
                    }
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }
                if agent.is_none() {
                    match Agent::start() {
                        Ok(ag) => agent = Some(ag),
                        Err(e) => {
                            app.lock().phone_status = format!("Bluetooth: {e}");
                            std::thread::sleep(Duration::from_secs(5));
                            continue;
                        }
                    }
                }
                if sink.is_none() {
                    let (tx, rx) = rtrb::RingBuffer::new(48000);
                    match crate::pw_out::open_phone_sink(tx) {
                        Ok(s) => {
                            cmd.send(Cmd::LineInAttach(rx));
                            sink = Some(s);
                        }
                        Err(e) => {
                            app.lock().phone_status = format!("saída virtual não abriu: {e}");
                            std::thread::sleep(Duration::from_secs(5));
                            continue;
                        }
                    }
                }
                // Celular conectado: manda o fluxo dele para a nossa saída virtual.
                let phones = crate::pw_out::phone_streams();
                let sink_id = crate::pw_out::node_id(crate::pw_out::PHONE_SINK);
                for p in &phones {
                    if moved.contains(&p.id) {
                        continue;
                    }
                    let ok = if p.is_source {
                        // Perfil em que o celular aparece como "microfone": liga direto.
                        Command::new("pw-link").args([&p.id.to_string(), crate::pw_out::PHONE_SINK]).status().is_ok_and(|s| s.success())
                    } else if let Some(sink_id) = sink_id {
                        let (id, to) = (p.id.to_string(), sink_id.to_string());
                        Command::new("pw-metadata").args([&id, "target.node", &to, "Spa:Id"]).stdout(Stdio::null()).status().is_ok_and(|s| s.success())
                    } else {
                        false
                    };
                    if ok {
                        eprintln!("[djos] celular \"{}\" ligado na entrada", p.name);
                        moved.insert(p.id);
                    }
                }
                // Celular desconectou: apaga o desvio (o número do nó pode ser reaproveitado).
                moved.retain(|id| {
                    let alive = phones.iter().any(|p| p.id == *id);
                    if !alive {
                        let _ = Command::new("pw-metadata").args(["-d", &id.to_string(), "target.node"]).stdout(Stdio::null()).status();
                    }
                    alive
                });
                app.lock().phone_status = match phones.first() {
                    Some(p) => format!("recebendo de {} → {}", p.name, ROUTES[route]),
                    None => "aguardando: no celular, conecte no Bluetooth \"DJ OS\"".into(),
                };
                if last_renew.elapsed() > Duration::from_secs(60) {
                    if let Some(ag) = agent.as_ref() {
                        ag.renew();
                    }
                    last_renew = std::time::Instant::now();
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .expect("thread do celular");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_o_endereco_do_aparelho() {
        assert_eq!(device_mac("[CHG] Device 7C:2A:DB:11:22:33 Connected: yes"), Some("7C:2A:DB:11:22:33"));
        assert_eq!(device_mac("[CHG] Controller 00:11:22:33:44:55 Discoverable: yes"), None);
    }

    #[test]
    fn destinos() {
        assert_eq!(route_target(1), Some(3));
        assert_eq!(route_target(2), Some(2));
        assert_eq!(route_target(3), None);
    }
}
