//! Minimal libmpv binding: the client API for playback control plus the
//! OpenGL render API, so mpv draws straight into a texture egui shows.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;

#[repr(C)]
struct MpvHandle {
    _p: [u8; 0],
}
#[repr(C)]
struct MpvRenderContext {
    _p: [u8; 0],
}

#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct MpvEventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

/// `mpv_event_end_file` from client.h; only the leading fields we read.
#[repr(C)]
struct MpvEventEndFile {
    reason: c_int, // mpv_end_file_reason (a C enum)
    error: c_int,
    // playlist_entry_id, playlist_insert_id, ... follow; only read through a pointer.
}

#[repr(C)]
struct MpvRenderParam {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct MpvOpenglInitParams {
    get_proc_address: extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    get_proc_address_ctx: *mut c_void,
}

#[repr(C)]
struct MpvOpenglFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

#[link(name = "mpv")]
unsafe extern "C" {
    fn mpv_create() -> *mut MpvHandle;
    fn mpv_initialize(ctx: *mut MpvHandle) -> c_int;
    fn mpv_terminate_destroy(ctx: *mut MpvHandle);
    fn mpv_set_option_string(ctx: *mut MpvHandle, name: *const c_char, data: *const c_char) -> c_int;
    fn mpv_command_async(ctx: *mut MpvHandle, reply: u64, args: *mut *const c_char) -> c_int;
    fn mpv_set_property_async(ctx: *mut MpvHandle, reply: u64, name: *const c_char, format: c_int, data: *mut c_void) -> c_int;
    fn mpv_get_property(ctx: *mut MpvHandle, name: *const c_char, format: c_int, data: *mut c_void) -> c_int;
    fn mpv_observe_property(ctx: *mut MpvHandle, reply: u64, name: *const c_char, format: c_int) -> c_int;
    fn mpv_wait_event(ctx: *mut MpvHandle, timeout: f64) -> *mut MpvEvent;
    fn mpv_error_string(error: c_int) -> *const c_char;
    fn mpv_set_wakeup_callback(ctx: *mut MpvHandle, cb: extern "C" fn(*mut c_void), d: *mut c_void);

    fn mpv_render_context_create(res: *mut *mut MpvRenderContext, mpv: *mut MpvHandle, params: *mut MpvRenderParam) -> c_int;
    fn mpv_render_context_set_update_callback(ctx: *mut MpvRenderContext, cb: extern "C" fn(*mut c_void), d: *mut c_void);
    fn mpv_render_context_update(ctx: *mut MpvRenderContext) -> u64;
    fn mpv_render_context_render(ctx: *mut MpvRenderContext, params: *mut MpvRenderParam) -> c_int;
    fn mpv_render_context_free(ctx: *mut MpvRenderContext);
}

const FORMAT_FLAG: c_int = 3;
const FORMAT_INT64: c_int = 4;
const FORMAT_DOUBLE: c_int = 5;

const EVENT_NONE: c_int = 0;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_PROPERTY_CHANGE: c_int = 22;

const END_FILE_REASON_ERROR: c_int = 4;

const RENDER_API_TYPE: c_int = 1;
const RENDER_OPENGL_INIT_PARAMS: c_int = 2;
const RENDER_OPENGL_FBO: c_int = 3;
const RENDER_FLIP_Y: c_int = 4;
const RENDER_UPDATE_FRAME: u64 = 1;

/// Something mpv told us.
#[derive(Debug, Clone)]
pub enum Event {
    FileLoaded,
    /// The file could not be played; mpv's error text.
    LoadFailed(String),
    Double(&'static str, f64),
    Int(&'static str, i64),
    Flag(&'static str, bool),
}

/// Properties delivered as [`Event`]s; the index is the reply id.
const OBSERVED: &[(&str, c_int)] = &[
    ("time-pos", FORMAT_DOUBLE),
    ("duration", FORMAT_DOUBLE),
    ("pause", FORMAT_FLAG),
    ("container-fps", FORMAT_DOUBLE),
    ("estimated-vf-fps", FORMAT_DOUBLE),
    // Pixel aspect applied, rotation not (see `Review::refresh_size`).
    ("video-params/dw", FORMAT_INT64),
    ("video-params/dh", FORMAT_INT64),
    ("video-params/rotate", FORMAT_INT64),
];

type Waker = Box<dyn Fn() + Send + Sync>;

pub struct Mpv {
    handle: *mut MpvHandle,
    render: *mut MpvRenderContext,
    // Boxed twice so the callbacks get a stable thin pointer.
    _wakeup: Box<Waker>,
    _redraw: Option<Box<Waker>>,
}

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap_or_default()
}

extern "C" fn call_waker(d: *mut c_void) {
    // SAFETY: d points at the Box<Waker> owned by Mpv, alive until mpv is destroyed.
    let w = unsafe { &*(d as *const Waker) };
    w();
}

impl Mpv {
    /// `wakeup` runs on an mpv thread whenever events are waiting.
    pub fn new(wakeup: impl Fn() + Send + Sync + 'static) -> Result<Self, String> {
        // SAFETY: plain C API calls on a handle we own.
        unsafe {
            let handle = mpv_create();
            if handle.is_null() {
                return Err("mpv_create failed".into());
            }
            for (k, v) in [
                ("vo", "libmpv"),
                ("hr-seek", "yes"),    // exact seeks, not keyframe-snapped
                ("keep-open", "yes"),  // stay on the last frame at the end
                ("pause", "yes"),
                ("idle", "yes"),
                ("osc", "no"),
                ("input-default-bindings", "no"),
                ("hwdec", "auto-safe"),
                ("audio-display", "no"),
            ] {
                mpv_set_option_string(handle, cstr(k).as_ptr(), cstr(v).as_ptr());
            }
            if mpv_initialize(handle) < 0 {
                mpv_terminate_destroy(handle);
                return Err("mpv_initialize failed".into());
            }
            for (i, (name, fmt)) in OBSERVED.iter().enumerate() {
                mpv_observe_property(handle, i as u64, cstr(name).as_ptr(), *fmt);
            }
            let wakeup: Box<Waker> = Box::new(Box::new(wakeup));
            mpv_set_wakeup_callback(handle, call_waker, &*wakeup as *const Waker as *mut c_void);
            Ok(Self { handle, render: ptr::null_mut(), _wakeup: wakeup, _redraw: None })
        }
    }

    /// Creates the OpenGL renderer. Call with the GL context current.
    /// `redraw` runs (on an mpv thread) when a new frame is ready.
    pub fn init_gl(
        &mut self,
        get_proc_address: &dyn Fn(&CStr) -> *const c_void,
        redraw: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        extern "C" fn gpa(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
            // SAFETY: ctx is the &dyn Fn passed below, valid for the duration of create().
            let f = unsafe { &*(ctx as *const &dyn Fn(&CStr) -> *const c_void) };
            f(unsafe { CStr::from_ptr(name) }) as *mut c_void
        }
        let api = cstr("opengl");
        let mut init = MpvOpenglInitParams {
            get_proc_address: gpa,
            get_proc_address_ctx: &get_proc_address as *const _ as *mut c_void,
        };
        let mut params = [
            MpvRenderParam { kind: RENDER_API_TYPE, data: api.as_ptr() as *mut c_void },
            MpvRenderParam { kind: RENDER_OPENGL_INIT_PARAMS, data: &mut init as *mut _ as *mut c_void },
            MpvRenderParam { kind: 0, data: ptr::null_mut() },
        ];
        // SAFETY: params outlive the call; handle is valid.
        unsafe {
            let mut rc = ptr::null_mut();
            if mpv_render_context_create(&mut rc, self.handle, params.as_mut_ptr()) < 0 {
                return Err("mpv_render_context_create failed".into());
            }
            self.render = rc;
            let redraw: Box<Waker> = Box::new(Box::new(redraw));
            mpv_render_context_set_update_callback(rc, call_waker, &*redraw as *const Waker as *mut c_void);
            self._redraw = Some(redraw);
        }
        Ok(())
    }

    /// Renders the current frame into `fbo` if mpv has a new one. GL context must be current.
    pub fn render_if_needed(&self, fbo: u32, w: i32, h: i32, force: bool) -> bool {
        if self.render.is_null() {
            return false;
        }
        // SAFETY: render context is valid; params live across the call.
        unsafe {
            let flags = mpv_render_context_update(self.render);
            if flags & RENDER_UPDATE_FRAME == 0 && !force {
                return false;
            }
            let mut target = MpvOpenglFbo { fbo: fbo as c_int, w, h, internal_format: 0 };
            let mut flip: c_int = 0; // FBO texture: mpv's row order already matches egui's sampling
            let mut params = [
                MpvRenderParam { kind: RENDER_OPENGL_FBO, data: &mut target as *mut _ as *mut c_void },
                MpvRenderParam { kind: RENDER_FLIP_Y, data: &mut flip as *mut _ as *mut c_void },
                MpvRenderParam { kind: 0, data: ptr::null_mut() },
            ];
            mpv_render_context_render(self.render, params.as_mut_ptr());
        }
        true
    }

    pub fn command(&self, args: &[&str]) {
        let owned: Vec<CString> = args.iter().map(|a| cstr(a)).collect();
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(ptr::null());
        // SAFETY: mpv copies the arguments before returning.
        unsafe { mpv_command_async(self.handle, 0, ptrs.as_mut_ptr()) };
    }

    pub fn set_flag(&self, name: &str, v: bool) {
        let mut d: c_int = v as c_int;
        // SAFETY: data copied by mpv.
        unsafe { mpv_set_property_async(self.handle, 0, cstr(name).as_ptr(), FORMAT_FLAG, &mut d as *mut _ as *mut c_void) };
    }

    pub fn set_double(&self, name: &str, v: f64) {
        let mut d = v;
        // SAFETY: data copied by mpv.
        unsafe { mpv_set_property_async(self.handle, 0, cstr(name).as_ptr(), FORMAT_DOUBLE, &mut d as *mut _ as *mut c_void) };
    }

    pub fn get_double(&self, name: &str) -> Option<f64> {
        let mut d = 0f64;
        // SAFETY: synchronous read into a local.
        let r = unsafe { mpv_get_property(self.handle, cstr(name).as_ptr(), FORMAT_DOUBLE, &mut d as *mut _ as *mut c_void) };
        (r >= 0).then_some(d)
    }

    pub fn get_int(&self, name: &str) -> Option<i64> {
        let mut d = 0i64;
        // SAFETY: synchronous read into a local.
        let r = unsafe { mpv_get_property(self.handle, cstr(name).as_ptr(), FORMAT_INT64, &mut d as *mut _ as *mut c_void) };
        (r >= 0).then_some(d)
    }

    /// Drains the event queue without blocking.
    pub fn poll(&self) -> Vec<Event> {
        let mut out = Vec::new();
        loop {
            // SAFETY: the returned event is valid until the next wait_event call.
            unsafe {
                let e = &*mpv_wait_event(self.handle, 0.0);
                match e.event_id {
                    EVENT_NONE => break,
                    EVENT_FILE_LOADED => out.push(Event::FileLoaded),
                    EVENT_END_FILE if !e.data.is_null() => {
                        let f = &*(e.data as *const MpvEventEndFile);
                        if f.reason == END_FILE_REASON_ERROR {
                            let msg = CStr::from_ptr(mpv_error_string(f.error)).to_string_lossy().into_owned();
                            out.push(Event::LoadFailed(msg));
                        }
                    }
                    EVENT_PROPERTY_CHANGE => {
                        let Some(&(name, _)) = OBSERVED.get(e.reply_userdata as usize) else { continue };
                        let p = &*(e.data as *const MpvEventProperty);
                        if p.data.is_null() {
                            continue;
                        }
                        match p.format {
                            FORMAT_DOUBLE => out.push(Event::Double(name, *(p.data as *const f64))),
                            FORMAT_INT64 => out.push(Event::Int(name, *(p.data as *const i64))),
                            FORMAT_FLAG => out.push(Event::Flag(name, *(p.data as *const c_int) != 0)),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        // SAFETY: render context must go before the core handle.
        unsafe {
            if !self.render.is_null() {
                mpv_render_context_free(self.render);
            }
            mpv_terminate_destroy(self.handle);
        }
    }
}
