//! Minimal dynamically-loaded libmpv binding (client + OpenGL render API).
use libloading::Library;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;
use std::sync::Arc;

type Handle = *mut c_void;
type RenderCtx = *mut c_void;

#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}
#[repr(C)]
struct GlInit {
    get_proc_address: extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    ctx: *mut c_void,
}
#[repr(C)]
struct GlFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}
#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    userdata: u64,
    data: *mut c_void,
}
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}

const P_API_TYPE: c_int = 1;
const P_GL_INIT: c_int = 2;
const P_GL_FBO: c_int = 3;
const P_FLIP_Y: c_int = 4;
const F_FLAG: c_int = 3;
const F_INT64: c_int = 4;
const F_DOUBLE: c_int = 5;
const EV_END_FILE: c_int = 7;
const EV_FILE_LOADED: c_int = 8;
const UPDATE_FRAME: u64 = 1;

pub type ProcAddr = Arc<dyn Fn(&CStr) -> *const c_void>;

#[derive(Debug, Clone, PartialEq)]
pub enum Event_ {
    Loaded,
    Ended,
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct Track {
    pub id: i64,
    pub audio: bool,
    pub lang: String,
    pub title: String,
    pub selected: bool,
}

impl Track {
    pub fn label(&self) -> String {
        let mut parts = vec![format!("#{}", self.id)];
        if !self.lang.is_empty() {
            parts.push(self.lang.clone());
        }
        if !self.title.is_empty() {
            parts.push(self.title.clone());
        }
        parts.join(" · ")
    }
}

struct Api {
    _lib: Library,
    create: unsafe extern "C" fn() -> Handle,
    initialize: unsafe extern "C" fn(Handle) -> c_int,
    set_option_string: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    set_property_string: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    command: unsafe extern "C" fn(Handle, *const *const c_char) -> c_int,
    get_property: unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int,
    get_property_string: unsafe extern "C" fn(Handle, *const c_char) -> *mut c_char,
    free: unsafe extern "C" fn(*mut c_void),
    wait_event: unsafe extern "C" fn(Handle, f64) -> *mut Event,
    terminate_destroy: unsafe extern "C" fn(Handle),
    error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    rc_create: unsafe extern "C" fn(*mut RenderCtx, Handle, *mut RenderParam) -> c_int,
    rc_render: unsafe extern "C" fn(RenderCtx, *mut RenderParam) -> c_int,
    rc_update: unsafe extern "C" fn(RenderCtx) -> u64,
    rc_free: unsafe extern "C" fn(RenderCtx),
    rc_set_cb: unsafe extern "C" fn(RenderCtx, Option<extern "C" fn(*mut c_void)>, *mut c_void),
}

pub struct Mpv {
    api: Api,
    h: Handle,
    rc: RenderCtx,
    // Kept alive for the lifetime of the render context (raw pointers handed to libmpv).
    _proc: *mut ProcAddr,
    _wake: *mut Box<dyn Fn() + Send + Sync>,
}

extern "C" fn get_proc(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe {
        let f = &*(ctx as *const ProcAddr);
        f(CStr::from_ptr(name)) as *mut c_void
    }
}
extern "C" fn on_update(ctx: *mut c_void) {
    unsafe { (*(ctx as *const Box<dyn Fn() + Send + Sync>))() }
}

fn c(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap()
}

impl Mpv {
    pub fn new(dll: &std::path::Path, proc_addr: ProcAddr, wake: Box<dyn Fn() + Send + Sync>) -> Result<Mpv, String> {
        unsafe {
            let lib = Library::new(dll).map_err(|e| format!("cannot load {}: {e}", dll.display()))?;
            macro_rules! sym {
                ($n:literal) => {
                    *lib.get($n).map_err(|e| format!("missing symbol: {e}"))?
                };
            }
            let api = Api {
                create: sym!(b"mpv_create\0"),
                initialize: sym!(b"mpv_initialize\0"),
                set_option_string: sym!(b"mpv_set_option_string\0"),
                set_property_string: sym!(b"mpv_set_property_string\0"),
                command: sym!(b"mpv_command\0"),
                get_property: sym!(b"mpv_get_property\0"),
                get_property_string: sym!(b"mpv_get_property_string\0"),
                free: sym!(b"mpv_free\0"),
                wait_event: sym!(b"mpv_wait_event\0"),
                terminate_destroy: sym!(b"mpv_terminate_destroy\0"),
                error_string: sym!(b"mpv_error_string\0"),
                rc_create: sym!(b"mpv_render_context_create\0"),
                rc_render: sym!(b"mpv_render_context_render\0"),
                rc_update: sym!(b"mpv_render_context_update\0"),
                rc_free: sym!(b"mpv_render_context_free\0"),
                rc_set_cb: sym!(b"mpv_render_context_set_update_callback\0"),
                _lib: lib,
            };
            let h = (api.create)();
            if h.is_null() {
                return Err("mpv_create failed".into());
            }
            let mut m = Mpv { api, h, rc: ptr::null_mut(), _proc: ptr::null_mut(), _wake: ptr::null_mut() };
            for (k, v) in [
                ("vo", "libmpv"),
                ("hwdec", "auto-safe"),
                ("sid", "no"),
                ("keep-open", "no"),
                ("idle", "yes"),
                ("input-default-bindings", "no"),
                ("input-vo-keyboard", "no"),
                ("osc", "no"),
                ("osd-level", "0"),
                ("terminal", "no"),
                ("audio-display", "no"),
                ("cache", "yes"),
                ("demuxer-max-bytes", "64MiB"),
                ("demuxer-max-back-bytes", "16MiB"),
                ("cache-pause-initial", "no"),
                ("network-timeout", "15"),
                ("user-agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) IPTVPlayer/1.0"),
                ("video-sync", "audio"),
                ("scale", "bilinear"),
                ("cscale", "bilinear"),
                ("dither-depth", "no"),
                ("deband", "no"),
                ("ytdl", "no"),
            ] {
                m.option(k, v);
            }
            let r = (m.api.initialize)(h);
            if r < 0 {
                return Err(format!("mpv_initialize: {}", m.err(r)));
            }

            let proc_box = Box::into_raw(Box::new(proc_addr));
            let mut init = GlInit { get_proc_address: get_proc, ctx: proc_box as *mut c_void };
            let api_type = c("opengl");
            let mut params = [
                RenderParam { kind: P_API_TYPE, data: api_type.as_ptr() as *mut c_void },
                RenderParam { kind: P_GL_INIT, data: &mut init as *mut GlInit as *mut c_void },
                RenderParam { kind: 0, data: ptr::null_mut() },
            ];
            let mut rc: RenderCtx = ptr::null_mut();
            let r = (m.api.rc_create)(&mut rc, h, params.as_mut_ptr());
            if r < 0 {
                return Err(format!("render context: {}", m.err(r)));
            }
            m.rc = rc;
            m._proc = proc_box;
            let wake_box = Box::into_raw(Box::new(wake));
            m._wake = wake_box;
            (m.api.rc_set_cb)(rc, Some(on_update), wake_box as *mut c_void);
            Ok(m)
        }
    }

    fn err(&self, code: c_int) -> String {
        unsafe { CStr::from_ptr((self.api.error_string)(code)).to_string_lossy().into_owned() }
    }

    pub fn option(&mut self, k: &str, v: &str) {
        unsafe {
            (self.api.set_option_string)(self.h, c(k).as_ptr(), c(v).as_ptr());
        }
    }

    pub fn set(&self, k: &str, v: &str) {
        unsafe {
            (self.api.set_property_string)(self.h, c(k).as_ptr(), c(v).as_ptr());
        }
    }

    pub fn command(&self, args: &[&str]) {
        let owned: Vec<CString> = args.iter().map(|a| c(a)).collect();
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|s| s.as_ptr()).collect();
        ptrs.push(ptr::null());
        unsafe {
            (self.api.command)(self.h, ptrs.as_ptr());
        }
    }

    pub fn load(&self, url: &str) {
        self.command(&["loadfile", url, "replace"]);
        self.set("pause", "no");
    }

    pub fn stop(&self) {
        self.command(&["stop"]);
    }

    pub fn get_f64(&self, k: &str) -> Option<f64> {
        let mut v: f64 = 0.0;
        let r = unsafe { (self.api.get_property)(self.h, c(k).as_ptr(), F_DOUBLE, &mut v as *mut f64 as *mut c_void) };
        (r >= 0).then_some(v)
    }
    pub fn get_i64(&self, k: &str) -> Option<i64> {
        let mut v: i64 = 0;
        let r = unsafe { (self.api.get_property)(self.h, c(k).as_ptr(), F_INT64, &mut v as *mut i64 as *mut c_void) };
        (r >= 0).then_some(v)
    }
    pub fn get_bool(&self, k: &str) -> Option<bool> {
        let mut v: c_int = 0;
        let r = unsafe { (self.api.get_property)(self.h, c(k).as_ptr(), F_FLAG, &mut v as *mut c_int as *mut c_void) };
        (r >= 0).then_some(v != 0)
    }
    pub fn get_string(&self, k: &str) -> Option<String> {
        unsafe {
            let p = (self.api.get_property_string)(self.h, c(k).as_ptr());
            if p.is_null() {
                return None;
            }
            let s = CStr::from_ptr(p).to_string_lossy().into_owned();
            (self.api.free)(p as *mut c_void);
            Some(s)
        }
    }

    pub fn tracks(&self) -> Vec<Track> {
        let n = self.get_i64("track-list/count").unwrap_or(0);
        let mut out = Vec::new();
        for i in 0..n {
            let kind = self.get_string(&format!("track-list/{i}/type")).unwrap_or_default();
            if kind != "audio" && kind != "sub" {
                continue;
            }
            out.push(Track {
                id: self.get_i64(&format!("track-list/{i}/id")).unwrap_or(0),
                audio: kind == "audio",
                lang: self.get_string(&format!("track-list/{i}/lang")).unwrap_or_default(),
                title: self.get_string(&format!("track-list/{i}/title")).unwrap_or_default(),
                selected: self.get_bool(&format!("track-list/{i}/selected")).unwrap_or(false),
            });
        }
        out
    }

    pub fn poll_events(&self) -> Vec<Event_> {
        let mut out = Vec::new();
        unsafe {
            loop {
                let e = &*(self.api.wait_event)(self.h, 0.0);
                match e.id {
                    0 => break,
                    EV_FILE_LOADED => out.push(Event_::Loaded),
                    EV_END_FILE if !e.data.is_null() => {
                        let ef = &*(e.data as *const EndFile);
                        match ef.reason {
                            0 => out.push(Event_::Ended),
                            4 => out.push(Event_::Failed(self.err(ef.error))),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }

    /// Returns true if a new frame was rendered into `fbo`.
    pub fn render(&self, fbo: u32, w: i32, h: i32, force: bool) -> bool {
        unsafe {
            let flags = (self.api.rc_update)(self.rc);
            if flags & UPDATE_FRAME == 0 && !force {
                return false;
            }
            let mut f = GlFbo { fbo: fbo as c_int, w, h, internal_format: 0 };
            let mut flip: c_int = 0;
            let mut params = [
                RenderParam { kind: P_GL_FBO, data: &mut f as *mut GlFbo as *mut c_void },
                RenderParam { kind: P_FLIP_Y, data: &mut flip as *mut c_int as *mut c_void },
                RenderParam { kind: 0, data: ptr::null_mut() },
            ];
            (self.api.rc_render)(self.rc, params.as_mut_ptr());
            true
        }
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        unsafe {
            if !self.rc.is_null() {
                (self.api.rc_set_cb)(self.rc, None, ptr::null_mut());
                (self.api.rc_free)(self.rc);
            }
            (self.api.terminate_destroy)(self.h);
        }
    }
}
