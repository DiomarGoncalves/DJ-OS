//! DJ OS — sistema de DJ all-in-one para a Pioneer DDJ-REV1.
//!
//! Partes:
//! - `engine`: motor de áudio (decks, efeitos, mixer) rodando na thread de áudio;
//! - `midi`: mapeamento da DDJ-REV1 e LEDs;
//! - `ui`: tela no estilo XDJ;
//! - `library` e `track`: pendrives/SSDs, leitura e análise das músicas.

mod app;
mod audio_out;
mod bluetooth;
mod engine;
mod firmware;
mod grid;
mod history;
mod import;
mod key;
mod library;
mod loudness;
mod midi;
mod phone;
mod record;
mod resume;
mod pw_out;
mod search;
mod store;
mod settings;
mod shared;
mod track;
mod ui;

use parking_lot::Mutex;
use shared::{CmdTx, Snapshot};
use std::sync::Arc;

fn main() -> eframe::Result<()> {
    app::migrate_old_data_dir();
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--listar-audio") {
        for d in audio_out::list_devices() {
            println!("{d}");
        }
        return Ok(());
    }
    if args.len() > 2 && args[1] == "--analisar" {
        for path in &args[2..] {
            let t0 = std::time::Instant::now();
            match track::load(std::path::Path::new(path), true) {
                Ok(t) => println!(
                    "{}: {:.2} BPM, tom {}, volume {:.1} LUFS (ganho automático {:+.1} dB), primeira batida {:.3} s, {:.1} s de música, analisado em {} ms",
                    t.display_name(),
                    t.bpm(),
                    key::from_raw(t.key).map_or("?".to_string(), |k| format!("{} ({})", key::label(k), key::name(k))),
                    t.loudness,
                    loudness::gain_db(t.loudness),
                    t.first_beat() / t.sr,
                    t.duration_secs(),
                    t0.elapsed().as_millis()
                ),
                Err(e) => println!("{path}: erro: {e}"),
            }
        }
        return Ok(());
    }
    if std::env::var_os("DJOS_DEBUG_SNAP").is_some() {
        eprintln!("[djos] início {}", chrono::Local::now().format("%H:%M:%S%.3f"));
    }
    watch_deadlocks();
    let snap = Arc::new(Mutex::new(Snapshot::default()));
    let (cmd_prod, cmd_cons) = rtrb::RingBuffer::new(8192);
    let (trash_prod, trash_cons) = rtrb::RingBuffer::new(64);
    let cmd = CmdTx::new(cmd_prod);
    let app = Arc::new(Mutex::new(app::AppState::new()));

    // O áudio abre em segundo plano: se o driver demorar ou travar, a tela aparece mesmo assim
    // e mostra "ÁUDIO PARADO". A thread guarda a saída aberta enquanto o programa roda.
    app.lock().audio_status = "abrindo o áudio…".into();
    {
        let (app, snap) = (app.clone(), snap.clone());
        std::thread::Builder::new()
            .name("audio-inicio".into())
            .spawn(move || match audio_out::start(cmd_cons, trash_prod, snap) {
                Ok(handle) => loop {
                    let s = handle.status();
                    if app.lock().audio_status != s {
                        app.lock().audio_status = s;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                },
                Err(e) => {
                    eprintln!("[djos] sem áudio: {e}");
                    app.lock().audio_status = format!("SEM ÁUDIO: {e}");
                }
            })
            .expect("thread de áudio");
    }

    // Configurações salvas (vinil, pitch, crossfader, keylock) valem desde o começo.
    let saved = app.lock().settings.clone();
    saved.apply(&cmd);
    // DJOS_KEYLOCK=1 liga o keylock em todos os decks ao abrir.
    if std::env::var_os("DJOS_KEYLOCK").is_some() {
        for deck in 0..shared::NUM_DECKS {
            cmd.send(shared::Cmd::Keylock { deck, on: true });
        }
    }
    // Diagnóstico: DJOS_MUDO zera master e fone (testes sem som saindo).
    if std::env::var_os("DJOS_MUDO").is_some() {
        cmd.send(shared::Cmd::MasterLevel(0.0));
        cmd.send(shared::Cmd::HeadphoneLevel(0.0));
    }

    let loader = app::Loader::start(app.clone(), cmd.clone());
    // Reaberto pela sessão depois de um travamento: as músicas voltam para os decks.
    resume::restore_after_crash(&mut app.lock(), &loader);
    {
        // Análise automática das músicas novas (prioridade baixa).
        let a = app.lock();
        import::start_analyzer(a.lib.index.clone(), a.lib.analyzed.clone());
    }
    midi::start(app.clone(), cmd.clone(), snap.clone(), loader.clone());
    if let Ok(m) = std::env::var("DJOS_BT") {
        app.lock().bt_mode = if m == "retorno" { bluetooth::BtMode::Retorno } else { bluetooth::BtMode::Master };
    }
    bluetooth::start(app.clone(), cmd.clone());
    // Celular por Bluetooth (só liga o Bluetooth se a opção estiver ligada nas configurações).
    phone::start(app.clone(), cmd.clone());
    // Versão do sistema e, depois de 30 s rodando bem, confirma uma atualização em teste.
    firmware::refresh_info(&app);
    firmware::confirm_later(&app);

    let fullscreen = std::env::args().any(|a| a == "--tela-cheia");
    // Em tela cheia (no aparelho) a janela fica do tamanho da tela, sem tamanho fixo e sem barra
    // de título: no Wayland a barra desenhada pelo programa ocuparia 35 px da altura da tela.
    let viewport = eframe::egui::ViewportBuilder::default().with_title("DJ OS");
    let viewport = if fullscreen {
        viewport.with_fullscreen(true).with_decorations(false)
    } else {
        viewport.with_inner_size([1366.0, 768.0])
    };
    let options = eframe::NativeOptions {
        viewport,
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "DJ OS",
        options,
        Box::new(move |cc| Ok(Box::new(ui::DjOsApp::new(cc, app, cmd, snap, loader, trash_cons)))),
    )
}

/// Vigia de travamento: se duas threads ficarem esperando uma pela outra (tela e controladora
/// paradas, só o som seguindo), anota no registro onde cada uma parou e fecha o programa. A sessão
/// do aparelho o reabre com as músicas de volta nos decks, em vez de ficar congelado o resto do set.
fn watch_deadlocks() {
    std::thread::Builder::new()
        .name("vigia".into())
        .spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let stuck = parking_lot::deadlock::check_deadlock();
            if stuck.is_empty() {
                continue;
            }
            eprintln!("[djos] TRAVAMENTO: {} grupo(s) de threads presas esperando uma pela outra", stuck.len());
            for (i, threads) in stuck.iter().enumerate() {
                for t in threads {
                    eprintln!("[djos] grupo {i}, thread {:?}:\n{:?}", t.thread_id(), t.backtrace());
                }
            }
            std::process::abort();
        })
        .expect("thread vigia");
}
