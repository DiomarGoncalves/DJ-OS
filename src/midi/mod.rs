//! Conexão com a controladora: reconecta sozinha e atualiza os LEDs.

pub mod rev1;
#[cfg(test)]
mod tests;

use crate::app::{Loader, SharedApp};
use crate::shared::{CmdTx, SharedSnap};
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use parking_lot::Mutex;
use rev1::Rev1;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn is_rev1(name: &str) -> bool {
    let n = name.to_uppercase();
    n.contains("REV1") || n.contains("DDJ-REV")
}

pub fn start(app: SharedApp, cmd: CmdTx, snap: SharedSnap, loader: Loader) {
    let mapping = Arc::new(Mutex::new(Rev1::new(app.clone(), cmd, snap.clone(), loader)));
    let out: Arc<Mutex<Option<MidiOutputConnection>>> = Arc::new(Mutex::new(None));
    let cache: Arc<Mutex<HashMap<(u8, u8), u8>>> = Arc::new(Mutex::new(HashMap::new()));

    // Conexão: procura a REV1 a cada segundo e reconecta se ela for desligada e ligada.
    {
        let (app, out, cache) = (app.clone(), out.clone(), cache.clone());
        std::thread::Builder::new()
            .name("midi".into())
            .spawn(move || {
                let mut conn: Option<MidiInputConnection<()>> = None;
                // Um só cliente MIDI para procurar a REV1 (a lista de portas é relida a cada volta).
                let scanner = MidiInput::new("dj-os-busca").ok();
                loop {
                    let present = scanner
                        .as_ref()
                        .map(|m| m.ports().iter().any(|p| m.port_name(p).is_ok_and(|n| is_rev1(&n))))
                        .unwrap_or(false);
                    if conn.is_some() && !present {
                        conn = None;
                        *out.lock() = None;
                        app.lock().midi_status = "DDJ-REV1 desconectada".into();
                    } else if conn.is_none() && present {
                        match connect(mapping.clone()) {
                            Ok((i, o)) => {
                                conn = Some(i);
                                *out.lock() = Some(o);
                                cache.lock().clear();
                                app.lock().midi_status = "DDJ-REV1 conectada".into();
                                eprintln!("[djos] DDJ-REV1 conectada");
                            }
                            Err(e) => app.lock().midi_status = format!("DDJ-REV1: {e}"),
                        }
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            })
            .expect("thread midi");
    }

    // LEDs: 30 vezes por segundo, só manda o que mudou.
    std::thread::Builder::new()
        .name("leds".into())
        .spawn(move || {
            let start = Instant::now();
            let mut list = Vec::with_capacity(256);
            loop {
                std::thread::sleep(Duration::from_millis(33));
                let mut o = out.lock();
                let Some(conn) = o.as_mut() else { continue };
                let blink = (start.elapsed().as_millis() / 400) % 2 == 0;
                list.clear();
                {
                    let s = *snap.lock();
                    let a = app.lock();
                    rev1::leds(&a, &s, blink, &mut list);
                }
                let mut c = cache.lock();
                for &(st, n, v) in &list {
                    if c.get(&(st, n)) != Some(&v) {
                        if conn.send(&[st, n, v]).is_ok() {
                            c.insert((st, n), v);
                        }
                    }
                }
            }
        })
        .expect("thread leds");
}

fn connect(mapping: Arc<Mutex<Rev1>>) -> Result<(MidiInputConnection<()>, MidiOutputConnection), String> {
    let input = MidiInput::new("dj-os").map_err(|e| e.to_string())?;
    let in_port = input
        .ports()
        .into_iter()
        .find(|p| input.port_name(p).is_ok_and(|n| is_rev1(&n)))
        .ok_or("entrada MIDI não encontrada")?;
    let output = MidiOutput::new("dj-os").map_err(|e| e.to_string())?;
    let out_port = output
        .ports()
        .into_iter()
        .find(|p| output.port_name(p).is_ok_and(|n| is_rev1(&n)))
        .ok_or("saída MIDI não encontrada")?;
    let conn_in = input
        .connect(&in_port, "rev1-in", move |_, msg, _| mapping.lock().handle(msg), ())
        .map_err(|e| e.to_string())?;
    let conn_out = output.connect(&out_port, "rev1-out").map_err(|e| e.to_string())?;
    Ok((conn_in, conn_out))
}
