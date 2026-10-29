//! Saída de áudio falando direto com o PipeWire (sem as pontes ALSA).
//!
//! A biblioteca `libpipewire-0.3.so.0` é carregada em tempo de execução, então não precisa dos
//! pacotes de desenvolvimento para compilar. O formato do fluxo (um "SPA POD") é montado à mão.
//!
//! O sistema aparece no PipeWire como o nó "dj-os". Com a DDJ-REV1 ligada são 4 canais
//! (1-2 master, 3-4 fone) apontados para ela; sem a REV1, 2 canais na saída padrão. Os canais do
//! fluxo usam os mesmos nomes da saída da REV1 (AUX0-3 no perfil "pro audio", FL FR RL RR no
//! "surround 4.0"); com nomes diferentes o PipeWire remisturaria e o fone se perderia.

use crate::engine::Engine;
use crate::shared::{Cmd, SharedSnap};
use crate::track::Track;
use std::ffi::{c_char, c_int, c_void, CString};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ---- constantes do SPA (spa/utils/type.h, spa/param/format.h, spa/param/audio/raw.h) ----
const SPA_TYPE_ID: u32 = 3;
const SPA_TYPE_INT: u32 = 4;
const SPA_TYPE_ARRAY: u32 = 13;
const SPA_TYPE_OBJECT: u32 = 15;
const SPA_TYPE_OBJECT_FORMAT: u32 = 0x40003;
const SPA_PARAM_ENUM_FORMAT: u32 = 3;
const SPA_FORMAT_MEDIA_TYPE: u32 = 1;
const SPA_FORMAT_MEDIA_SUBTYPE: u32 = 2;
const SPA_FORMAT_AUDIO_FORMAT: u32 = 0x10001;
const SPA_FORMAT_AUDIO_RATE: u32 = 0x10003;
const SPA_FORMAT_AUDIO_CHANNELS: u32 = 0x10004;
const SPA_FORMAT_AUDIO_POSITION: u32 = 0x10005;
const SPA_MEDIA_TYPE_AUDIO: u32 = 1;
const SPA_MEDIA_SUBTYPE_RAW: u32 = 1;
/// SPA_AUDIO_FORMAT_F32_LE (conferido com `pw-dump`, que mostra "F32LE" no nó).
const SPA_AUDIO_FORMAT_F32_LE: u32 = 0x11b;
const SPA_AUDIO_CHANNEL_FL: u32 = 3;
const SPA_AUDIO_CHANNEL_FR: u32 = 4;
const SPA_AUDIO_CHANNEL_AUX0: u32 = 0x1000;
const STEREO: [u32; 2] = [SPA_AUDIO_CHANNEL_FL, SPA_AUDIO_CHANNEL_FR];

/// Nome do canal no PipeWire ("FL", "RR", "AUX2"…) → número do SPA (spa/param/audio/raw.h).
fn channel_id(name: &str) -> Option<u32> {
    const NAMES: [&str; 11] = ["FL", "FR", "FC", "LFE", "SL", "SR", "FLC", "FRC", "RC", "RL", "RR"];
    if let Some(n) = name.strip_prefix("AUX") {
        return n.parse::<u32>().ok().map(|n| SPA_AUDIO_CHANNEL_AUX0 + n);
    }
    NAMES.iter().position(|&n| n == name).map(|i| SPA_AUDIO_CHANNEL_FL + i as u32)
}

const SPA_DIRECTION_INPUT: u32 = 0;
const SPA_DIRECTION_OUTPUT: u32 = 1;
const PW_ID_ANY: u32 = 0xffff_ffff;
const PW_STREAM_FLAG_AUTOCONNECT: u32 = 1 << 0;
const PW_STREAM_FLAG_MAP_BUFFERS: u32 = 1 << 2;
const PW_STREAM_FLAG_RT_PROCESS: u32 = 1 << 4;
const PW_STREAM_STATE_ERROR: i32 = -1;
const PW_STREAM_STATE_PAUSED: i32 = 2;
const PW_STREAM_STATE_STREAMING: i32 = 3;

/// Monta o POD do formato: áudio cru F32 intercalado, taxa e canais.
fn format_pod(rate: u32, positions: &[u32]) -> Vec<u64> {
    let channels = positions.len() as u32;
    let mut b: Vec<u32> = Vec::new();
    let prop_id = |b: &mut Vec<u32>, key: u32, v: u32| b.extend_from_slice(&[key, 0, 4, SPA_TYPE_ID, v, 0]);
    let mut body: Vec<u32> = vec![SPA_TYPE_OBJECT_FORMAT, SPA_PARAM_ENUM_FORMAT];
    prop_id(&mut body, SPA_FORMAT_MEDIA_TYPE, SPA_MEDIA_TYPE_AUDIO);
    prop_id(&mut body, SPA_FORMAT_MEDIA_SUBTYPE, SPA_MEDIA_SUBTYPE_RAW);
    prop_id(&mut body, SPA_FORMAT_AUDIO_FORMAT, SPA_AUDIO_FORMAT_F32_LE);
    body.extend_from_slice(&[SPA_FORMAT_AUDIO_RATE, 0, 4, SPA_TYPE_INT, rate, 0]);
    body.extend_from_slice(&[SPA_FORMAT_AUDIO_CHANNELS, 0, 4, SPA_TYPE_INT, channels, 0]);
    let array_size = 8 + 4 * channels;
    body.extend_from_slice(&[SPA_FORMAT_AUDIO_POSITION, 0, array_size, SPA_TYPE_ARRAY, 4, SPA_TYPE_ID]);
    body.extend_from_slice(positions);
    if channels % 2 == 1 {
        body.push(0);
    }
    b.extend_from_slice(&[(body.len() * 4) as u32, SPA_TYPE_OBJECT]);
    b.extend_from_slice(&body);
    if b.len() % 2 == 1 {
        b.push(0);
    }
    // Guarda em u64 para ficar alinhado em 8 bytes, como o PipeWire exige.
    b.chunks(2).map(|c| (c[0] as u64) | ((c[1] as u64) << 32)).collect()
}

// ---- estruturas do PipeWire usadas no callback (layout x86_64) ----
#[repr(C)]
struct SpaChunk {
    offset: u32,
    size: u32,
    stride: i32,
    flags: i32,
}

#[repr(C)]
struct SpaData {
    kind: u32,
    flags: u32,
    fd: i64,
    mapoffset: u32,
    maxsize: u32,
    data: *mut c_void,
    chunk: *mut SpaChunk,
}

#[repr(C)]
struct SpaBuffer {
    n_metas: u32,
    n_datas: u32,
    metas: *mut c_void,
    datas: *mut SpaData,
}

#[repr(C)]
struct PwBuffer {
    buffer: *mut SpaBuffer,
    user_data: *mut c_void,
    size: u64,
    requested: u64,
}

type Cb = Option<unsafe extern "C" fn(*mut c_void)>;

#[repr(C)]
struct PwStreamEvents {
    version: u32,
    destroy: Cb,
    state_changed: Option<unsafe extern "C" fn(*mut c_void, c_int, c_int, *const c_char)>,
    control_info: Cb,
    io_changed: Cb,
    param_changed: Cb,
    add_buffer: Cb,
    remove_buffer: Cb,
    process: Cb,
    drained: Cb,
    command: Cb,
    trigger_done: Cb,
}

/// Funções da libpipewire, carregadas uma vez.
struct Lib {
    init: unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char),
    thread_loop_new: unsafe extern "C" fn(*const c_char, *const c_void) -> *mut c_void,
    thread_loop_get_loop: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    thread_loop_start: unsafe extern "C" fn(*mut c_void) -> c_int,
    thread_loop_lock: unsafe extern "C" fn(*mut c_void),
    thread_loop_unlock: unsafe extern "C" fn(*mut c_void),
    thread_loop_stop: unsafe extern "C" fn(*mut c_void),
    thread_loop_destroy: unsafe extern "C" fn(*mut c_void),
    properties_new_string: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    stream_new_simple:
        unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_void, *const PwStreamEvents, *mut c_void) -> *mut c_void,
    stream_connect: unsafe extern "C" fn(*mut c_void, u32, u32, u32, *mut *const c_void, u32) -> c_int,
    stream_destroy: unsafe extern "C" fn(*mut c_void),
    dequeue: unsafe extern "C" fn(*mut c_void) -> *mut PwBuffer,
    queue: unsafe extern "C" fn(*mut c_void, *mut PwBuffer) -> c_int,
}

fn lib() -> Result<&'static Lib, String> {
    static LIB: std::sync::OnceLock<Result<Lib, String>> = std::sync::OnceLock::new();
    LIB.get_or_init(|| {
        let l = unsafe { libloading::Library::new("libpipewire-0.3.so.0") }.map_err(|e| format!("libpipewire não encontrada: {e}"))?;
        let l: &'static libloading::Library = Box::leak(Box::new(l));
        macro_rules! sym {
            ($name:literal) => {
                *unsafe { l.get(concat!($name, "\0").as_bytes()) }.map_err(|e| format!("símbolo {} ausente: {e}", $name))?
            };
        }
        let lib = Lib {
            init: sym!("pw_init"),
            thread_loop_new: sym!("pw_thread_loop_new"),
            thread_loop_get_loop: sym!("pw_thread_loop_get_loop"),
            thread_loop_start: sym!("pw_thread_loop_start"),
            thread_loop_lock: sym!("pw_thread_loop_lock"),
            thread_loop_unlock: sym!("pw_thread_loop_unlock"),
            thread_loop_stop: sym!("pw_thread_loop_stop"),
            thread_loop_destroy: sym!("pw_thread_loop_destroy"),
            properties_new_string: sym!("pw_properties_new_string"),
            stream_new_simple: sym!("pw_stream_new_simple"),
            stream_connect: sym!("pw_stream_connect"),
            stream_destroy: sym!("pw_stream_destroy"),
            dequeue: sym!("pw_stream_dequeue_buffer"),
            queue: sym!("pw_stream_queue_buffer"),
        };
        unsafe { (lib.init)(ptr::null_mut(), ptr::null_mut()) };
        Ok(lib)
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// Quem preenche o áudio de um fluxo: recebe o buffer intercalado e o número de canais.
type Render = Box<dyn FnMut(&mut [f32], usize) + Send>;

struct Ctx {
    stream: *mut c_void,
    channels: usize,
    /// Fluxo que recebe som (a saída virtual do celular): o callback lê em vez de escrever.
    input: bool,
    render: Render,
    state: Arc<AtomicI32>,
    started: Arc<AtomicBool>,
}

unsafe extern "C" fn on_process(data: *mut c_void) {
    let ctx = &mut *(data as *mut Ctx);
    let Ok(l) = lib() else { return };
    let b = (l.dequeue)(ctx.stream);
    if b.is_null() {
        return;
    }
    let buf = &*(*b).buffer;
    if ctx.input && buf.n_datas > 0 && !(*buf.datas).data.is_null() {
        let d = &*buf.datas;
        let chunk = &*d.chunk;
        let offset = (chunk.offset.min(d.maxsize)) as usize;
        let size = (chunk.size.min(d.maxsize - offset as u32)) as usize;
        let frames = size / (4 * ctx.channels);
        let data = std::slice::from_raw_parts_mut((d.data as *mut u8).add(offset) as *mut f32, frames * ctx.channels);
        (ctx.render)(data, ctx.channels);
        ctx.started.store(true, Ordering::Release);
    } else if buf.n_datas > 0 && !(*buf.datas).data.is_null() {
        let d = &mut *buf.datas;
        let stride = 4 * ctx.channels;
        let mut frames = d.maxsize as usize / stride;
        let req = (*b).requested as usize;
        if req > 0 {
            frames = frames.min(req);
        }
        let out = std::slice::from_raw_parts_mut(d.data as *mut f32, frames * ctx.channels);
        (ctx.render)(out, ctx.channels);
        let chunk = &mut *d.chunk;
        chunk.offset = 0;
        chunk.stride = stride as i32;
        chunk.size = (frames * stride) as u32;
        ctx.started.store(true, Ordering::Release);
    }
    (l.queue)(ctx.stream, b);
}

unsafe extern "C" fn on_state(data: *mut c_void, _old: c_int, state: c_int, error: *const c_char) {
    let ctx = &*(data as *mut Ctx);
    ctx.state.store(state, Ordering::Release);
    if state == PW_STREAM_STATE_ERROR && !error.is_null() {
        let msg = std::ffi::CStr::from_ptr(error).to_string_lossy();
        eprintln!("[djos] PipeWire: erro no fluxo: {msg}");
    }
}

/// Um fluxo de saída aberto no PipeWire.
pub struct Stream {
    tloop: *mut c_void,
    stream: *mut c_void,
    ctx: *mut Ctx,
    _events: Box<PwStreamEvents>,
    _pod: Vec<u64>,
}

// O PipeWire chama o callback na thread dele; a estrutura em si só é usada por quem abriu.
unsafe impl Send for Stream {}

impl Stream {
    /// Abre o fluxo e espera o primeiro bloco de áudio (até 3 s).
    fn open(node: &str, description: &str, target: Option<&str>, positions: &[u32], latency: u32, render: Render) -> Result<Stream, String> {
        Self::open_with(node, description, target, positions, latency, None, render)
    }

    /// `sink`: em vez de tocar, aparece no PipeWire como uma saída (caixa de som) com essas
    /// propriedades extras e recebe o som que mandarem para ela.
    fn open_with(
        node: &str,
        description: &str,
        target: Option<&str>,
        positions: &[u32],
        latency: u32,
        sink: Option<&str>,
        render: Render,
    ) -> Result<Stream, String> {
        let channels = positions.len() as u32;
        let input = sink.is_some();
        let l = lib()?;
        let rate = RATE;
        // Sem "media.role": o WirePlumber guarda destino e volume por tipo de som, e os nossos
        // fluxos não podem herdar o que outro app de "Música" fez (nem passar para eles).
        let mut props = format!(
            "media.type=Audio media.category=Playback node.name={node} \
             node.description=\"{description}\" application.name=\"DJ OS\" \
             node.latency={latency}/{rate} node.always-process=true stream.dont-remix=true \
             state.restore-target=false state.restore-props=false"
        );
        if let Some(t) = target {
            props.push_str(&format!(" target.object={t}"));
        }
        if let Some(extra) = sink {
            // Sem "always-process": parada (sem gastar CPU) enquanto nenhum celular manda som.
            props = props.replace("media.category=Playback", "media.category=Capture").replace(" node.always-process=true", "");
            props.push(' ');
            props.push_str(extra);
        }
        let props_c = CString::new(props).map_err(|e| e.to_string())?;
        let name_c = CString::new(node).map_err(|e| e.to_string())?;
        let state = Arc::new(AtomicI32::new(0));
        let started = Arc::new(AtomicBool::new(false));
        let ctx = Box::into_raw(Box::new(Ctx {
            stream: ptr::null_mut(),
            channels: channels as usize,
            input,
            render,
            state: state.clone(),
            started: started.clone(),
        }));
        let events = Box::new(PwStreamEvents {
            version: 2,
            destroy: None,
            state_changed: Some(on_state),
            control_info: None,
            io_changed: None,
            param_changed: None,
            add_buffer: None,
            remove_buffer: None,
            process: Some(on_process),
            drained: None,
            command: None,
            trigger_done: None,
        });
        let pod = format_pod(rate, positions);
        unsafe {
            let tloop = (l.thread_loop_new)(name_c.as_ptr(), ptr::null());
            if tloop.is_null() {
                drop(Box::from_raw(ctx));
                return Err("não criou o laço do PipeWire".into());
            }
            let props_ptr = (l.properties_new_string)(props_c.as_ptr());
            let stream = (l.stream_new_simple)((l.thread_loop_get_loop)(tloop), name_c.as_ptr(), props_ptr, &*events, ctx as *mut c_void);
            if stream.is_null() {
                (l.thread_loop_destroy)(tloop);
                drop(Box::from_raw(ctx));
                return Err("não criou o fluxo do PipeWire".into());
            }
            (*ctx).stream = stream;
            let s = Stream { tloop, stream, ctx, _events: events, _pod: pod };
            let mut params = [s._pod.as_ptr() as *const c_void];
            let flags = PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_MAP_BUFFERS | PW_STREAM_FLAG_RT_PROCESS;
            let dir = if input { SPA_DIRECTION_INPUT } else { SPA_DIRECTION_OUTPUT };
            let rc = (l.stream_connect)(stream, dir, PW_ID_ANY, flags, params.as_mut_ptr(), 1);
            if rc < 0 || (l.thread_loop_start)(tloop) < 0 {
                s.close();
                return Err("o PipeWire recusou o fluxo".into());
            }
            let t0 = Instant::now();
            while t0.elapsed() < Duration::from_secs(3) {
                if started.load(Ordering::Acquire) && state.load(Ordering::Acquire) == PW_STREAM_STATE_STREAMING {
                    return Ok(s);
                }
                // A saída virtual fica parada até alguém mandar som: basta estar pronta.
                if input && state.load(Ordering::Acquire) >= PW_STREAM_STATE_PAUSED {
                    return Ok(s);
                }
                if state.load(Ordering::Acquire) == PW_STREAM_STATE_ERROR {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            s.close();
            Err("o PipeWire não começou a tocar em 3 s".into())
        }
    }

    pub fn close(self) {
        let Ok(l) = lib() else { return };
        unsafe {
            (l.thread_loop_lock)(self.tloop);
            (l.stream_destroy)(self.stream);
            (l.thread_loop_unlock)(self.tloop);
            (l.thread_loop_stop)(self.tloop);
            (l.thread_loop_destroy)(self.tloop);
            drop(Box::from_raw(self.ctx));
        }
    }
}

const RATE: u32 = 48000;

/// Saída principal (motor) aberta no PipeWire. Um vigia troca o fluxo quando a REV1 é ligada ou
/// desligada com o sistema aberto; `status` diz para onde o som está indo.
pub struct PwOutput {
    pub status: Arc<parking_lot::Mutex<String>>,
}

fn pw_dump() -> Vec<serde_json::Value> {
    let Ok(out) = std::process::Command::new("pw-dump").output() else { return Vec::new() };
    serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout).unwrap_or_default()
}

fn props(o: &serde_json::Value) -> &serde_json::Value {
    &o["info"]["props"]
}

fn prop<'a>(o: &'a serde_json::Value, key: &str) -> &'a str {
    props(o)[key].as_str().unwrap_or("")
}

fn is_rev1(text: &str) -> bool {
    let t = text.to_uppercase();
    t.contains("REV1") || t.contains("DDJ")
}

/// Saídas de áudio do PipeWire cujo nome começa com `prefix`.
pub fn find_sinks(prefix: &str) -> Vec<String> {
    pw_dump()
        .iter()
        .filter(|o| prop(o, "media.class") == "Audio/Sink" && prop(o, "node.name").starts_with(prefix))
        .map(|o| prop(o, "node.name").to_string())
        .collect()
}

/// Saída do master no PipeWire: a REV1 (com os nomes dos canais: "AUX0,AUX1,…" ou
/// "FL,FR,RL,RR") ou, sem ela, uma placa de som com fio.
#[derive(Clone, Debug, PartialEq)]
struct Sink {
    name: String,
    positions: Vec<String>,
    rev1: bool,
    label: String,
}

fn find_rev1_in(dump: &[serde_json::Value]) -> Option<Sink> {
    dump.iter()
        .filter(|o| prop(o, "media.class") == "Audio/Sink" && prop(o, "node.name").starts_with("alsa_output") && is_rev1(prop(o, "node.name")))
        .map(|o| Sink {
            name: prop(o, "node.name").to_string(),
            positions: prop(o, "audio.position").split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect(),
            rev1: true,
            label: "DDJ-REV1".into(),
        })
        // Com mais de uma saída (perfil com várias), a de mais canais.
        .max_by_key(|s| s.positions.len())
}

/// Sem a REV1: a placa de som com fio de maior prioridade. Nunca uma caixa Bluetooth: ela recebe
/// o master pelo fluxo próprio dela, e mandar também por aqui duplicaria o som (eco).
fn find_wired_in(dump: &[serde_json::Value]) -> Option<Sink> {
    dump.iter()
        .filter(|o| prop(o, "media.class") == "Audio/Sink" && prop(o, "node.name").starts_with("alsa_output") && !is_rev1(prop(o, "node.name")))
        .max_by_key(|o| props(o)["priority.session"].as_i64().unwrap_or(0))
        .map(|o| Sink {
            name: prop(o, "node.name").to_string(),
            positions: Vec::new(),
            rev1: false,
            label: [prop(o, "node.description"), prop(o, "node.nick")].into_iter().find(|t| !t.is_empty()).unwrap_or("placa de som").to_string(),
        })
}

/// Saída do master: a REV1 ou, sem ela, uma placa de som com fio.
fn pick_main(dump: &[serde_json::Value]) -> Option<Sink> {
    find_rev1_in(dump).or_else(|| find_wired_in(dump))
}

/// Perfil da placa da REV1 com 4 saídas: "pro audio" (canais diretos) ou "surround 4.0".
fn rev1_profile_index(device: &serde_json::Value) -> Option<u64> {
    let profiles = device["info"]["params"]["EnumProfile"].as_array()?;
    let ok = |p: &&serde_json::Value| p["available"].as_str() != Some("no");
    let by = |want: &dyn Fn(&str) -> bool| profiles.iter().filter(ok).find(|p| want(p["name"].as_str().unwrap_or(""))).and_then(|p| p["index"].as_u64());
    by(&|n| n == "pro-audio").or_else(|| by(&|n| n.starts_with("output:analog-surround-40")))
}

/// Se a REV1 está num perfil de 2 canais (ou sem saída), troca para um de 4. Devolve a saída.
fn prepare_rev1() -> Option<Sink> {
    let dump = pw_dump();
    let sink = find_rev1_in(&dump);
    if sink.as_ref().is_some_and(|s| s.positions.len() >= 4) {
        return sink;
    }
    let device = dump.iter().find(|o| {
        prop(o, "media.class") == "Audio/Device" && [prop(o, "device.name"), prop(o, "device.description"), prop(o, "device.product.name")].iter().any(|t| is_rev1(t))
    })?;
    let (Some(id), Some(index)) = (device["id"].as_u64(), rev1_profile_index(device)) else { return sink };
    eprintln!("[djos] REV1 com {} canais: trocando para o perfil {index} (4 canais)", sink.as_ref().map_or(0, |s| s.positions.len()));
    let _ = std::process::Command::new("pw-cli")
        .args(["set-param", &id.to_string(), "Profile", &format!("{{ index: {index}, save: true }}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(4) {
        std::thread::sleep(Duration::from_millis(250));
        if let Some(s) = find_rev1_in(&pw_dump()).filter(|s| s.positions.len() >= 4) {
            return Some(s);
        }
    }
    find_rev1_in(&pw_dump()).or(sink)
}

/// Canais do fluxo para a saída escolhida: os da REV1 (se forem 4 conhecidos) ou estéreo.
fn stream_positions(sink: Option<&Sink>) -> Vec<u32> {
    let ids: Option<Vec<u32>> = sink.filter(|s| s.rev1).and_then(|s| s.positions.iter().map(|c| channel_id(c)).collect::<Option<Vec<u32>>>());
    match ids {
        Some(ids) if ids.len() >= 4 => ids,
        _ => STEREO.to_vec(),
    }
}

fn describe(sink: Option<&Sink>, channels: usize) -> String {
    match sink {
        Some(s) if s.rev1 && channels >= 4 => format!("PipeWire · DDJ-REV1 · master 1-2, fone 3-4 ({})", s.positions.join(",")),
        Some(s) if s.rev1 => format!("DDJ-REV1 com {} canais: FONE SEM SOM (só master)", s.positions.len()),
        Some(s) => format!("PipeWire · {} (sem a REV1)", s.label),
        None => NO_OUTPUT.to_string(),
    }
}

type Parts = (rtrb::Consumer<Cmd>, rtrb::Producer<Arc<Track>>);
type EngineSlot = Arc<parking_lot::Mutex<Option<Engine>>>;

fn open_main(slot: &EngineSlot, sink: Option<&Sink>) -> Result<(Stream, String), String> {
    let positions = stream_positions(sink);
    let s2 = slot.clone();
    let render: Render = Box::new(move |out, ch| match s2.try_lock().as_mut().and_then(|g| g.as_mut()) {
        Some(e) => e.process(out, ch),
        None => out.iter_mut().for_each(|x| *x = 0.0),
    });
    let target = sink.map(|s| s.name.as_str());
    let stream = Stream::open("dj-os", "DJ OS", target, &positions, 256, render)?;
    Ok((stream, describe(sink, positions.len())))
}

/// Texto do estado quando não há nenhuma saída de som.
pub const NO_OUTPUT: &str = "SEM SAÍDA DE SOM · ligue a REV1";

/// Há alguma saída para o master (REV1 ou placa com fio; Bluetooth e celular não contam)?
fn has_output(dump: &[serde_json::Value]) -> bool {
    pick_main(dump).is_some()
}

/// Relógio silencioso: sem nenhuma saída de som, o motor continua rodando no tempo certo (play,
/// hotcues, waveform funcionam), só que o som não vai para lugar nenhum.
struct SilentClock {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}

impl SilentClock {
    fn start(slot: &EngineSlot) -> SilentClock {
        let stop = Arc::new(AtomicBool::new(false));
        let (stop2, slot) = (stop.clone(), slot.clone());
        let thread = std::thread::Builder::new()
            .name("audio-silencio".into())
            .spawn(move || {
                const FRAMES: usize = 256;
                let period = Duration::from_secs_f64(FRAMES as f64 / RATE as f64);
                let mut buf = vec![0.0f32; FRAMES * 2];
                let mut next = Instant::now();
                while !stop2.load(Ordering::Relaxed) {
                    if let Some(e) = slot.lock().as_mut() {
                        e.process(&mut buf, 2);
                    }
                    next += period;
                    let now = Instant::now();
                    if next > now {
                        std::thread::sleep(next - now);
                    } else if now - next > Duration::from_millis(100) {
                        next = now; // atrasou muito (máquina ocupada): não tenta compensar
                    }
                }
            })
            .expect("relógio silencioso");
        SilentClock { stop, thread }
    }

    fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.thread.join();
    }
}

/// Abre a saída principal. Sem o PipeWire rodando, devolve as filas para tentar o ALSA.
pub fn start(parts: Parts, snap: SharedSnap) -> Result<PwOutput, (String, Parts)> {
    if let Err(e) = lib() {
        return Err((e, parts));
    }
    let dump = pw_dump();
    if dump.is_empty() {
        return Err(("o PipeWire não está rodando".into(), parts));
    }
    let (cmds, trash) = parts;
    // O motor fica num "slot": se a abertura falhar, ele volta para tentarmos outra saída.
    let slot: EngineSlot = Arc::new(parking_lot::Mutex::new(Some(Engine::new(RATE as f64, cmds, trash, snap))));
    let mut sink = None;
    let mut stream = None;
    let mut silent = None;
    let desc = if has_output(&dump) {
        sink = prepare_rev1().or_else(|| pick_main(&dump));
        match open_main(&slot, sink.as_ref()) {
            Ok((s, d)) => {
                stream = Some(s);
                d
            }
            Err(e) => {
                let engine = slot.lock().take().expect("motor");
                return Err((e, engine.into_parts()));
            }
        }
    } else {
        silent = Some(SilentClock::start(&slot));
        NO_OUTPUT.to_string()
    };
    let status = Arc::new(parking_lot::Mutex::new(desc));
    let st = status.clone();
    // Vigia: REV1 ligada depois, desligada ou com o perfil trocado → reabre o fluxo para ela.
    // Sem nenhuma saída, o motor roda no relógio silencioso até aparecer uma.
    std::thread::Builder::new()
        .name("audio-vigia".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(3));
            let dump = pw_dump();
            let now = pick_main(&dump);
            let any = now.is_some();
            let changed = match &stream {
                None => any,
                Some(_) => match (&sink, &now) {
                    (None, None) => false,
                    (Some(a), Some(b)) => a.name != b.name || (a.rev1 && a.positions.len() < 4 && b.positions.len() >= 4),
                    _ => true,
                },
            };
            if !changed {
                continue;
            }
            if let Some(old) = stream.take() {
                old.close();
            }
            if !any {
                eprintln!("[djos] nenhuma saída de som: relógio silencioso");
                silent.get_or_insert_with(|| SilentClock::start(&slot));
                *st.lock() = NO_OUTPUT.to_string();
                sink = None;
                continue;
            }
            // Para o relógio silencioso antes de abrir: o motor não pode rodar em dois lugares.
            if let Some(c) = silent.take() {
                c.stop();
            }
            let next = if now.as_ref().is_some_and(|s| s.rev1) { prepare_rev1().or(now) } else { now };
            eprintln!("[djos] saída mudou: {:?} → {:?}", sink.as_ref().map(|s| &s.name), next.as_ref().map(|s| &s.name));
            // Se a REV1 não abrir, fica na placa com fio e tenta de novo na próxima volta.
            let opened = match open_main(&slot, next.as_ref()) {
                Ok(v) => Some((v, next)),
                Err(_) => {
                    let wired = find_wired_in(&pw_dump());
                    wired.as_ref().and_then(|w| open_main(&slot, Some(w)).ok()).map(|v| (v, wired))
                }
            };
            match opened {
                Some(((s, d), used)) => {
                    *st.lock() = d;
                    stream = Some(s);
                    sink = used;
                }
                None => {
                    silent = Some(SilentClock::start(&slot));
                    *st.lock() = NO_OUTPUT.to_string();
                }
            }
        })
        .expect("vigia do áudio");
    Ok(PwOutput { status })
}

/// Fluxo da caixa Bluetooth: toca o que o motor manda pela fila, corrigindo de leve a velocidade
/// para acompanhar o relógio da caixa (diferente do relógio da REV1).
pub fn start_bluetooth(target: &str, mut rx: rtrb::Consumer<[f32; 2]>) -> Result<Stream, String> {
    const TARGET_FILL: f64 = 4800.0; // 100 ms de folga na fila
    let mut primed = false;
    let mut t = 0.0f64;
    let (mut prev, mut cur) = ([0.0f32; 2], [0.0f32; 2]);
    let render: Render = Box::new(move |out, ch| {
        let fill = rx.slots() as f64;
        if !primed {
            if fill < TARGET_FILL {
                out.iter_mut().for_each(|x| *x = 0.0);
                return;
            }
            primed = true;
        }
        // Mais cheia que o alvo: consome um pouco mais rápido (e vice-versa), no máximo 0,5%.
        let ratio = 1.0 + ((fill - TARGET_FILL) / TARGET_FILL * 0.002).clamp(-0.005, 0.005);
        for frame in out.chunks_exact_mut(ch) {
            t += ratio;
            while t >= 1.0 {
                prev = cur;
                match rx.pop() {
                    Ok(f) => cur = f,
                    Err(_) => primed = false, // esvaziou: espera encher de novo
                }
                t -= 1.0;
            }
            let k = t as f32;
            frame[0] = prev[0] + (cur[0] - prev[0]) * k;
            frame[1] = prev[1] + (cur[1] - prev[1]) * k;
            for x in frame.iter_mut().skip(2) {
                *x = 0.0;
            }
        }
    });
    Stream::open("dj-os-bluetooth", "DJ OS Bluetooth", Some(target), &STEREO, 1024, render)
}

/// Nome da saída virtual que recebe o som do celular.
pub const PHONE_SINK: &str = "dj-os-celular";

/// Cria a saída virtual "DJ OS (celular)": o som que chegar nela vai para a fila do motor.
pub fn open_phone_sink(mut tx: rtrb::Producer<[f32; 2]>) -> Result<Stream, String> {
    let render: Render = Box::new(move |data, ch| {
        for f in data.chunks_exact(ch) {
            // Fila cheia (motor não está lendo): descarta.
            let _ = tx.push([f[0], f[ch.min(2) - 1]]);
        }
    });
    // Prioridade baixa para nunca virar a saída padrão do sistema.
    let extra = "media.class=Audio/Sink priority.session=1 priority.driver=1 node.virtual=true";
    Stream::open_with(PHONE_SINK, "DJ OS (celular)", None, &STEREO, 1024, Some(extra), render)
}

/// Som chegando de um celular pelo Bluetooth.
pub struct Phone {
    pub id: u64,
    pub name: String,
    /// Aparece como "microfone" (Audio/Source) em vez de fluxo tocando.
    pub is_source: bool,
}

pub fn phone_streams() -> Vec<Phone> {
    pw_dump()
        .iter()
        .filter(|o| {
            let class = prop(o, "media.class");
            prop(o, "node.name").starts_with("bluez_input") && (class == "Stream/Output/Audio" || class == "Audio/Source")
        })
        .filter_map(|o| {
            let name = [prop(o, "node.description"), prop(o, "device.description"), prop(o, "node.nick")].into_iter().find(|s| !s.is_empty()).unwrap_or("celular");
            Some(Phone { id: o["id"].as_u64()?, name: name.to_string(), is_source: prop(o, "media.class") == "Audio/Source" })
        })
        .collect()
}

/// Número do nó pelo nome (o WirePlumber 0.4 desvia fluxos pela metadata `target.node`).
pub fn node_id(name: &str) -> Option<u64> {
    pw_dump()
        .iter()
        .find(|o| prop(o, "node.name") == name && o["type"].as_str().is_some_and(|t| t.ends_with("Node")))
        .and_then(|o| o["id"].as_u64())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O tamanho declarado no cabeçalho do POD tem que bater com o conteúdo, para 2 e 4 canais.
    #[test]
    fn pod_do_formato_tem_tamanho_coerente() {
        for ch in [2u32, 4] {
            let positions: Vec<u32> = (0..ch).map(|i| SPA_AUDIO_CHANNEL_AUX0 + i).collect();
            let pod = format_pod(48000, &positions);
            let words: Vec<u32> = pod.iter().flat_map(|w| [*w as u32, (*w >> 32) as u32]).collect();
            assert_eq!(words[1], SPA_TYPE_OBJECT);
            let body = words[0] as usize;
            assert_eq!(body % 8, 0, "corpo alinhado em 8 bytes");
            assert_eq!(8 + body, pod.len() * 8, "tamanho declarado = tamanho real ({ch} canais)");
            assert_eq!(&words[2..4], &[SPA_TYPE_OBJECT_FORMAT, SPA_PARAM_ENUM_FORMAT]);
            assert!(words.contains(&SPA_AUDIO_FORMAT_F32_LE));
        }
    }

    fn sink(name: &str, pos: &str) -> serde_json::Value {
        serde_json::json!({"id": 70, "info": {"props": {"media.class": "Audio/Sink", "node.name": name, "audio.position": pos}}})
    }

    #[test]
    fn canais_do_fluxo_seguem_os_da_rev1() {
        let pro = [sink("alsa_output.pci-0000.analog-stereo", "FL,FR"), sink("alsa_output.usb-Pioneer_DDJ-REV1-00.pro-output-0", "AUX0,AUX1,AUX2,AUX3")];
        let s = find_rev1_in(&pro).expect("acha a REV1");
        assert_eq!(stream_positions(Some(&s)), vec![0x1000, 0x1001, 0x1002, 0x1003]);
        let sur = [sink("alsa_output.usb-Pioneer_DDJ-REV1-00.analog-surround-40", "FL,FR,RL,RR")];
        let s = find_rev1_in(&sur).unwrap();
        assert_eq!(stream_positions(Some(&s)), vec![3, 4, 12, 13], "fone nos canais traseiros (3-4)");
        let st = [sink("alsa_output.usb-Pioneer_DDJ-REV1-00.analog-stereo", "FL,FR")];
        let s = find_rev1_in(&st).unwrap();
        assert_eq!(stream_positions(Some(&s)), STEREO.to_vec());
        assert!(describe(Some(&s), 2).contains("FONE SEM SOM"));
        assert!(find_rev1_in(&[sink("alsa_output.pci-0000.analog-stereo", "FL,FR")]).is_none());
    }

    #[test]
    fn escolhe_perfil_de_4_canais() {
        let dev = serde_json::json!({"info": {"params": {"EnumProfile": [
            {"index": 0, "name": "off", "available": "yes"},
            {"index": 1, "name": "output:analog-stereo", "available": "yes"},
            {"index": 3, "name": "output:analog-surround-40", "available": "yes"},
            {"index": 4, "name": "pro-audio", "available": "unknown"}
        ]}}});
        assert_eq!(rev1_profile_index(&dev), Some(4), "pro audio primeiro");
        let dev = serde_json::json!({"info": {"params": {"EnumProfile": [
            {"index": 1, "name": "output:analog-stereo", "available": "yes"},
            {"index": 3, "name": "output:analog-surround-40+input:analog-stereo", "available": "yes"}
        ]}}});
        assert_eq!(rev1_profile_index(&dev), Some(3));
    }
}

/// Teste manual (precisa do PipeWire rodando): `cargo test -- --ignored saida_virtual`.
#[cfg(test)]
mod tests_pipewire {
    #[test]
    #[ignore]
    fn saida_virtual_recebe_som() {
        let (tx, mut rx) = rtrb::RingBuffer::new(48000 * 4);
        let s = super::open_phone_sink(tx).expect("abre a saída virtual");
        let wav = std::env::var("DJOS_WAV_TESTE").expect("DJOS_WAV_TESTE");
        let ok = std::process::Command::new("pw-play").args(["--target", super::PHONE_SINK, "--volume", "1.0", "-P", "media.role=DjOsTeste", &wav]).status().unwrap();
        assert!(ok.success());
        let mut n = 0;
        let mut peak = 0.0f32;
        while let Ok(f) = rx.pop() {
            n += 1;
            peak = peak.max(f[0].abs());
        }
        s.close();
        eprintln!("recebidos {n} quadros, pico {peak}");
        assert!(n > 48000 && peak > 0.05);
    }

    /// Um fluxo tocando na saída padrão é desviado pela metadata, como o do celular.
    #[test]
    #[ignore]
    fn desvia_fluxo_pela_metadata() {
        let (tx, rx) = rtrb::RingBuffer::new(48000 * 4);
        let s = super::open_phone_sink(tx).expect("abre a saída virtual");
        let wav = std::env::var("DJOS_WAV_TESTE").expect("DJOS_WAV_TESTE");
        let props = "node.name=teste-desvio media.role=DjOsTeste";
        let mut play = std::process::Command::new("pw-play").args(["--volume", "0", "-P", props, &wav]).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(600));
        let id = super::pw_dump().iter().find(|o| super::prop(o, "node.name") == "teste-desvio").and_then(|o| o["id"].as_u64()).expect("fluxo de teste");
        let before = rx.slots();
        let sink_id = super::node_id(super::PHONE_SINK).expect("nó da saída virtual");
        std::process::Command::new("pw-metadata").args([&id.to_string(), "target.node", &sink_id.to_string(), "Spa:Id"]).output().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let got = rx.slots() - before;
        // Desfaz o desvio com o fluxo ainda vivo, para o WirePlumber não guardar a regra.
        std::process::Command::new("pw-metadata").args(["--", &id.to_string(), "target.node", "-1", "Spa:Id"]).output().ok();
        let _ = play.wait();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        std::process::Command::new("pw-metadata").args(["-d", &id.to_string(), "target.node"]).output().ok();
        s.close();
        eprintln!("antes do desvio {before} quadros, depois {got}");
        assert!(got > 48000, "o fluxo passou para a saída virtual");
    }

}
