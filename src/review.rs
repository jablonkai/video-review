//! All review state plus playback.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::glow::{self, HasContext};

use crate::i18n::{self, Lang, tr, trf};
use crate::model::scenes::Detection;
use crate::model::{self, Area, Change, Comment, CommentStore, Selection};
use crate::mpv::{Event, Mpv};
use crate::theme;
use crate::thumbs::Thumbnails;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScenesStatus {
    Pending,
    Ready,
    Error,
    /// ffmpeg is not installed (or not where a GUI app can find it).
    NoFfmpeg,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SpanMode {
    Moment,
    Scene,
    Range,
}

/// The texture mpv renders into, shown by egui as an ordinary image.
struct VideoTarget {
    fbo: glow::Framebuffer,
    tex: glow::Texture,
    id: egui::TextureId,
    w: i32,
    h: i32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Prefs {
    #[serde(default)]
    paths: Vec<PathBuf>,
    #[serde(default)]
    video: Option<PathBuf>,
    #[serde(default = "full_volume")]
    volume: f64,
    #[serde(default)]
    muted: bool,
    /// UI language code; unset means the system language.
    #[serde(default)]
    lang: Option<String>,
}

fn full_volume() -> f64 {
    100.0
}

impl Default for Prefs {
    fn default() -> Self {
        Self { paths: Vec::new(), video: None, volume: full_volume(), muted: false, lang: None }
    }
}

fn prefs_file() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("video-review").join("prefs.json")
}

fn load_prefs() -> Prefs {
    std::fs::read_to_string(prefs_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_prefs(p: &Prefs) {
    let f = prefs_file();
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(f, serde_json::to_string_pretty(p).unwrap_or_default());
}

/// The last delete or done/undone, and the change that takes it back.
pub struct Undo {
    /// Catalog key of the notice (`undo.*`), translated when shown.
    pub label: &'static str,
    change: Change,
    pub at: Instant,
}

pub struct Review {
    ctx: egui::Context,
    mpv: Mpv,
    gl: Option<Arc<glow::Context>>,
    target: Option<VideoTarget>,
    pub thumbs: Thumbnails,

    // ---- loaded media ----
    pub videos: Vec<PathBuf>,
    pub current: Option<PathBuf>,
    store: Option<CommentStore>,
    pub fps: f64,
    pub duration: f64,
    /// Displayed picture size: pixel aspect applied, rotation swapped.
    pub video_w: i64,
    pub video_h: i64,
    pub playing: bool,
    pub rate: f64,
    pub error: Option<String>,

    // ---- review state ----
    pub comments: Vec<Comment>,
    /// Changes not yet on disk (the file was unreadable); applied again once it reads.
    pending: Vec<Change>,
    pub undo: Option<Undo>,
    pub cuts: Vec<f64>,
    pub scenes_status: ScenesStatus,
    /// Why detection failed, for the tooltip.
    pub scenes_error: String,
    /// The one running detection; replacing or dropping it kills its ffmpeg.
    detection: Option<Detection>,
    /// No ffmpeg at start: no scene detection and no thumbnails.
    pub ffmpeg_missing: bool,
    pub sel: Option<Selection>,
    pub pending_in: Option<f64>,
    pub area: Option<Area>,
    pub focus_id: Option<String>,
    pub hover_id: Option<String>,
    pub hide_done: bool,
    /// Text shown in the transport row while hovering the timeline.
    pub hover_info: String,
    /// 0..100, as mpv counts it.
    pub volume: f64,
    pub muted: bool,

    // ---- playhead ----
    pos: f64,
    pos_at: Instant,
    last_check: Instant,
    fps_known: bool,
}

impl Review {
    pub fn new(ctx: &egui::Context) -> Result<Self, String> {
        let wake = ctx.clone();
        let mpv = Mpv::new(move || wake.request_repaint())?;
        let prefs = load_prefs();
        mpv.set_double("volume", prefs.volume);
        mpv.set_flag("mute", prefs.muted);
        i18n::set(prefs.lang.as_deref().and_then(Lang::from_code).unwrap_or_else(i18n::system));
        Ok(Self {
            ctx: ctx.clone(),
            mpv,
            gl: None,
            target: None,
            thumbs: Thumbnails::new(ctx.clone()),
            videos: Vec::new(),
            current: None,
            store: None,
            fps: 24.0,
            duration: 0.0,
            video_w: 0,
            video_h: 0,
            playing: false,
            rate: 1.0,
            error: None,
            comments: Vec::new(),
            pending: Vec::new(),
            undo: None,
            cuts: Vec::new(),
            scenes_status: ScenesStatus::Pending,
            scenes_error: String::new(),
            detection: None,
            ffmpeg_missing: model::find_ffmpeg().is_none(),
            sel: None,
            pending_in: None,
            area: None,
            focus_id: None,
            hover_id: None,
            hide_done: false,
            hover_info: String::new(),
            volume: prefs.volume,
            muted: prefs.muted,
            pos: 0.0,
            pos_at: Instant::now(),
            last_check: Instant::now(),
            fps_known: false,
        })
    }

    /// Hooks mpv up to eframe's OpenGL context. Call from the app creator.
    pub fn init_gl(&mut self, cc: &eframe::CreationContext<'_>) -> Result<(), String> {
        let gpa = cc.get_proc_address.clone().ok_or("no OpenGL context (start with the glow renderer)")?;
        let ctx = cc.egui_ctx.clone();
        self.mpv.init_gl(&*gpa, move || ctx.request_repaint())?;
        self.gl = cc.gl.clone();
        Ok(())
    }

    // ---- per frame ---------------------------------------------------------

    /// Events, background results, the external-change check. Once per frame.
    pub fn update(&mut self) {
        for e in self.mpv.poll() {
            match e {
                Event::Double("time-pos", t) => {
                    self.pos = t;
                    self.pos_at = Instant::now();
                }
                Event::Double("duration", d) => self.duration = d,
                Event::Flag("pause", p) => {
                    self.playing = !p;
                    if p && self.rate != 1.0 {
                        self.set_rate(1.0);
                    }
                    self.pos_at = Instant::now();
                }
                Event::Double("container-fps", f) if f > 1.0 && f < 1000.0 => {
                    self.fps = f;
                    self.fps_known = true;
                }
                Event::Double("estimated-vf-fps", f) if !self.fps_known && f > 1.0 && f < 1000.0 => self.fps = f,
                Event::Int("video-params/dw" | "video-params/dh", v) if v > 0 => self.refresh_size(),
                Event::Int("video-params/rotate", _) => self.refresh_size(),
                Event::FileLoaded => {
                    // Demuxer values are known as soon as the file is open; decoder ones may lag.
                    if let (Some(w), Some(h)) =
                        (self.mpv.get_int("current-tracks/video/demux-w"), self.mpv.get_int("current-tracks/video/demux-h"))
                        && w > 0 && h > 0 {
                            let par = self.mpv.get_double("current-tracks/video/demux-par").filter(|p| *p > 0.0).unwrap_or(1.0);
                            let rotate = self.mpv.get_int("current-tracks/video/demux-rotation").unwrap_or(0);
                            (self.video_w, self.video_h) = display_size(w, h, par, rotate);
                        }
                    if let Some(f) = self.mpv.get_double("container-fps").filter(|f| *f > 1.0 && *f < 1000.0) {
                        self.fps = f;
                        self.fps_known = true;
                    }
                }
                Event::LoadFailed(why) => {
                    self.error = Some(trf("error.open_video", &[("name", &self.current_name()), ("why", &why)]));
                }
                _ => {}
            }
        }
        self.thumbs.pump();
        if let Some(result) = self.detection.as_ref().and_then(|d| d.poll()) {
            // Only the current video's detection is ever kept, so the result is for it.
            self.detection = None;
            match result {
                Ok(cuts) => {
                    self.cuts = cuts;
                    self.scenes_status = ScenesStatus::Ready;
                }
                Err(e) if e == model::scenes::NO_FFMPEG => self.scenes_status = ScenesStatus::NoFfmpeg,
                Err(e) => {
                    self.scenes_error = e;
                    self.scenes_status = ScenesStatus::Error;
                }
            }
        }
        if self.last_check.elapsed() > Duration::from_secs(4) {
            self.last_check = Instant::now();
            // An unreadable file is tried again on every poll until it parses, and
            // pending changes until they are written.
            if self.store.as_ref().is_some_and(|s| s.read_only() || s.changed_externally()) || !self.pending.is_empty() {
                self.reload();
            }
        }
    }

    /// `video-params/dw`/`dh` have the pixel aspect applied but not the
    /// rotation, which the renderer applies after them.
    fn refresh_size(&mut self) {
        if let (Some(w), Some(h)) = (self.mpv.get_int("video-params/dw"), self.mpv.get_int("video-params/dh"))
            && w > 0 && h > 0 {
                let rotate = self.mpv.get_int("video-params/rotate").unwrap_or(0);
                (self.video_w, self.video_h) = display_size(w, h, 1.0, rotate);
            }
    }

    fn reload(&mut self) {
        let Some(store) = &mut self.store else { return };
        let was_read_only = store.read_only();
        match store.load() {
            Ok(mut list) => {
                for ch in &self.pending {
                    ch.apply(&mut list);
                }
                self.comments = list;
                if was_read_only {
                    self.error = None;
                }
                if !self.pending.is_empty() {
                    self.flush();
                }
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Lets mpv draw the newest frame into our texture; returns it for egui.
    pub fn video_texture(&mut self, frame: &mut eframe::Frame) -> Option<egui::TextureId> {
        let gl = self.gl.clone()?;
        let (w, h) = (self.video_w as i32, self.video_h as i32);
        if w <= 0 || h <= 0 {
            return None;
        }
        let mut fresh = false;
        // SAFETY: eframe keeps its GL context current while the UI runs.
        unsafe {
            if self.target.is_none() {
                let tex = gl.create_texture().ok()?;
                let fbo = gl.create_framebuffer().ok()?;
                let id = frame.register_native_glow_texture(tex);
                self.target = Some(VideoTarget { fbo, tex, id, w: 0, h: 0 });
            }
            let t = self.target.as_mut()?;
            if t.w != w || t.h != h {
                gl.bind_texture(glow::TEXTURE_2D, Some(t.tex));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    w,
                    h,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(t.fbo));
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(t.tex), 0);
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.bind_texture(glow::TEXTURE_2D, None);
                t.w = w;
                t.h = h;
                fresh = true;
            }
            let (fbo, id) = (t.fbo, t.id);
            self.mpv.render_if_needed(fbo.0.get(), w, h, fresh);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            Some(id)
        }
    }

    // ---- playhead ----------------------------------------------------------

    /// Interpolates between mpv's position events so the playhead moves smoothly.
    pub fn position(&self) -> f64 {
        if !self.playing {
            return self.pos;
        }
        let t = self.pos + self.pos_at.elapsed().as_secs_f64() * self.rate;
        if self.duration > 0.0 { t.min(self.duration) } else { t }
    }

    pub fn tc(&self, t: f64) -> String {
        model::timecode(t, self.fps)
    }

    // ---- opening -----------------------------------------------------------

    pub fn open_paths(&mut self, paths: Vec<PathBuf>) {
        let found = model::collect_videos(&paths);
        if found.is_empty() {
            self.error = Some(tr("error.no_video"));
            return;
        }
        self.error = None;
        self.videos = found;
        let mut prefs = load_prefs();
        prefs.paths = paths;
        let remembered = prefs.video.clone();
        save_prefs(&prefs);
        let pick = remembered.filter(|r| self.videos.contains(r)).unwrap_or_else(|| self.videos[0].clone());
        self.load_video(&pick);
    }

    pub fn restore_last_session(&mut self) {
        let prefs = load_prefs();
        if !prefs.paths.is_empty() {
            self.open_paths(prefs.paths);
            if self.current.is_none() {
                self.error = None; // a folder that went away is not worth an error on start
            }
        }
    }

    pub fn load_video(&mut self, path: &Path) {
        // One last try with this video's fps; what still can't be written waits in the store's pending file.
        self.save_pending();
        self.current = Some(path.to_owned());
        let mut prefs = load_prefs();
        prefs.video = Some(path.to_owned());
        save_prefs(&prefs);
        self.fps = 24.0;
        self.fps_known = false;
        self.duration = 0.0;
        self.video_w = 0;
        self.video_h = 0;
        self.pos = 0.0;
        self.pos_at = Instant::now();
        self.cuts.clear();
        self.scenes_status = ScenesStatus::Pending;
        self.sel = None;
        self.pending_in = None;
        self.area = None;
        self.focus_id = None;
        self.hover_id = None;
        self.undo = None;
        let mut store = CommentStore::new(path);
        self.comments = match store.load() {
            Ok(list) => list,
            Err(e) => {
                self.error = Some(e);
                Vec::new()
            }
        };
        // Left over from an earlier session; the poll writes them once the fps is known.
        self.pending = store.load_pending();
        for ch in &self.pending {
            ch.apply(&mut self.comments);
        }
        self.store = Some(store);
        self.thumbs.set_video(path);
        self.mpv.set_flag("pause", true);
        self.mpv.command(&["loadfile", &path.to_string_lossy()]);
        self.load_scenes(path, false);
    }

    pub fn current_name(&self) -> String {
        self.current.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// Cached cuts, or a fresh detection. `fresh` skips (and drops) the cache.
    fn load_scenes(&mut self, path: &Path, fresh: bool) {
        self.scenes_error.clear();
        if fresh {
            model::scenes::forget(path);
        } else if let Some(cuts) = model::scenes::cached(path) {
            self.detection = None;
            self.cuts = cuts;
            self.scenes_status = ScenesStatus::Ready;
            return;
        }
        self.scenes_status = ScenesStatus::Pending;
        // Reopening the same video keeps its running detection instead of starting a second one.
        if !fresh && self.detection.as_ref().is_some_and(|d| d.video == path) {
            return;
        }
        // Stop the old ffmpeg first, so only one decode runs at a time.
        self.detection = None;
        let ctx = self.ctx.clone();
        self.detection = Some(Detection::start(path, move || ctx.request_repaint()));
    }

    /// Share of the video the running detection has got through, if it is known.
    pub fn scenes_progress(&self) -> Option<f32> {
        let d = self.detection.as_ref()?;
        (self.duration > 0.0).then(|| (d.progress() / self.duration).clamp(0.0, 1.0) as f32)
    }

    /// Detects the cuts of the current video again, ignoring the cache.
    pub fn redetect_scenes(&mut self) {
        let Some(path) = self.current.clone() else { return };
        self.cuts.clear();
        if self.sel.is_some_and(|s| s.scenes.is_some()) {
            self.sel = None;
        }
        self.load_scenes(&path, true);
    }

    // ---- playback ----------------------------------------------------------

    pub fn toggle_play(&mut self) {
        if self.playing { self.pause() } else { self.play() }
    }

    pub fn pause(&mut self) {
        if self.playing {
            self.pos = self.position();
            self.pos_at = Instant::now();
            self.playing = false;
        }
        self.mpv.set_flag("pause", true);
    }

    pub fn play(&mut self) {
        self.set_rate(1.0);
        self.pos_at = Instant::now();
        self.playing = true;
        self.mpv.set_flag("pause", false);
    }

    pub fn set_volume(&mut self, v: f64) {
        self.volume = v.clamp(0.0, 100.0);
        self.muted = self.volume == 0.0;
        self.mpv.set_double("volume", self.volume);
        self.mpv.set_flag("mute", self.muted);
    }

    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        if !self.muted && self.volume == 0.0 {
            self.volume = 50.0;
            self.mpv.set_double("volume", self.volume);
        }
        self.mpv.set_flag("mute", self.muted);
        self.persist_volume();
    }

    /// Remembered for the next start. Called when a change is finished, not on every drag step.
    pub fn persist_volume(&self) {
        let mut prefs = load_prefs();
        prefs.volume = self.volume;
        prefs.muted = self.muted;
        save_prefs(&prefs);
    }

    /// Switches the UI language now, remembers it, and adds the system font it may need.
    pub fn set_language(&mut self, lang: Lang) {
        i18n::set(lang);
        let mut prefs = load_prefs();
        prefs.lang = Some(lang.code().to_owned());
        save_prefs(&prefs);
        theme::install_fonts(&self.ctx, lang);
    }

    pub fn set_rate(&mut self, r: f64) {
        self.pos = self.position();
        self.pos_at = Instant::now();
        self.rate = r;
        self.mpv.set_double("speed", r);
    }

    pub fn faster(&mut self) {
        if self.playing { self.set_rate((self.rate * 2.0).min(8.0)) } else { self.play() }
    }

    pub fn seek(&mut self, t: f64) {
        let target = if self.duration > 0.0 { t.clamp(0.0, self.duration) } else { t.max(0.0) };
        self.pos = target;
        self.pos_at = Instant::now();
        self.mpv.command(&["seek", &format!("{target:.6}"), "absolute+exact"]);
    }

    pub fn step_frames(&mut self, n: i32) {
        self.pause();
        match n {
            1 => self.mpv.command(&["frame-step"]),
            -1 => self.mpv.command(&["frame-back-step"]),
            _ => self.seek(self.position() + n as f64 / self.fps),
        }
    }

    // ---- scenes ------------------------------------------------------------

    pub fn bounds(&self) -> Vec<f64> {
        bounds(&self.cuts, self.duration)
    }

    pub fn scene_count(&self) -> u32 {
        if self.scenes_status == ScenesStatus::Ready && self.duration > 0.0 { self.bounds().len() as u32 - 1 } else { 0 }
    }

    /// Start and end of scene `n` (1-based); None before detection or out of range.
    pub fn scene_range(&self, n: u32) -> Option<(f64, f64)> {
        if n > self.scene_count() { None } else { scene_range(&self.cuts, self.duration, n) }
    }

    pub fn scene_at(&self, t: f64) -> u32 {
        scene_at(&self.cuts, self.duration, self.fps, t)
    }

    pub fn select_scenes(&mut self, a: u32, b: u32) {
        let (first, last) = (a.min(b), a.max(b));
        let (Some((t_in, _)), Some((_, t_out))) = (self.scene_range(first), self.scene_range(last)) else { return };
        self.pending_in = None;
        self.sel = Some(Selection { t_in, t_out: Some(t_out), scenes: Some((first, last)) });
    }

    pub fn extend_scenes(&mut self, n: u32) {
        // Scenes from a note detected elsewhere may not exist here; start over then.
        match self.sel.and_then(|s| s.scenes).filter(|&(a, b)| self.scene_range(a).is_some() && self.scene_range(b).is_some()) {
            None => self.select_scenes(n, n),
            Some((a, b)) => self.select_scenes(a.min(n), b.max(n)),
        }
    }

    pub fn jump_cut(&mut self, dir: i32) {
        if self.scene_count() == 0 {
            return;
        }
        if let Some(target) = cut_target(&self.cuts, self.duration, self.fps, self.position(), dir) {
            self.pause();
            self.seek(target);
        }
    }

    // ---- selection ---------------------------------------------------------

    pub fn set_range(&mut self, a: f64, b: f64) {
        self.pending_in = None;
        self.sel = range_selection(a, b, self.fps);
    }

    pub fn mark_in(&mut self) {
        self.pending_in = Some(self.position());
        self.sel = None;
    }

    pub fn mark_out(&mut self) {
        if let Some(a) = self.pending_in {
            self.set_range(a, self.position());
        }
    }

    pub fn clear_time(&mut self) {
        self.sel = None;
        self.pending_in = None;
    }

    pub fn clear_all(&mut self) {
        self.focus_id = None;
        self.sel = None;
        self.pending_in = None;
        self.area = None;
    }

    pub fn mode(&self) -> SpanMode {
        match self.sel {
            Some(s) if s.scenes.is_some() => SpanMode::Scene,
            Some(_) => SpanMode::Range,
            None if self.pending_in.is_some() => SpanMode::Range,
            None => SpanMode::Moment,
        }
    }

    pub fn set_mode(&mut self, m: SpanMode) {
        self.pause();
        match m {
            SpanMode::Moment => self.clear_time(),
            SpanMode::Scene => {
                if self.sel.and_then(|s| s.scenes).is_none() && self.scene_count() > 0 {
                    let n = self.scene_at(self.position());
                    self.select_scenes(n, n);
                    if let Some(s) = self.sel {
                        self.seek(s.t_in);
                    }
                }
            }
            SpanMode::Range => match self.sel {
                Some(s) if s.scenes.is_some() => self.sel = Some(Selection { scenes: None, ..s }),
                None => self.mark_in(),
                _ => {}
            },
        }
    }

    /// What a new comment would be attached to right now.
    pub fn draft_target(&self) -> Selection {
        self.sel.unwrap_or(Selection { t_in: self.position(), t_out: None, scenes: None })
    }

    // ---- comments ----------------------------------------------------------

    pub fn is_active(&self, c: &Comment, t: f64) -> bool {
        is_active(c, t, self.fps)
    }

    pub fn add_comment(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() || self.store.is_none() {
            return;
        }
        let t = self.draft_target();
        let c = Comment {
            id: Comment::new_id(),
            text: text.to_owned(),
            t_in: t.t_in,
            t_out: t.t_out,
            area: self.area,
            scenes: t.scenes,
            done: false,
            created: Some(chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()),
        };
        self.focus_id = Some(c.id.clone());
        self.sel = None;
        self.pending_in = None;
        self.area = None;
        self.change(Change::Put(c));
    }

    pub fn update_text(&mut self, id: &str, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.change(Change::Text { id: id.to_owned(), text: text.to_owned() });
    }

    pub fn set_done(&mut self, id: &str, done: bool) {
        let label = if done { "undo.marked_done" } else { "undo.reopened" };
        self.remember_undo(label, Change::Done { id: id.to_owned(), done: !done });
        self.change(Change::Done { id: id.to_owned(), done });
    }

    pub fn remove_comment(&mut self, id: &str) {
        if let Some(c) = self.comments.iter().find(|c| c.id == id).cloned() {
            self.remember_undo("undo.deleted", Change::Put(c));
        }
        if self.focus_id.as_deref() == Some(id) {
            self.focus_id = None;
        }
        self.change(Change::Remove(id.to_owned()));
    }

    fn remember_undo(&mut self, label: &'static str, change: Change) {
        self.undo = Some(Undo { label, change, at: Instant::now() });
    }

    /// Takes back the last delete or done/undone. One step only.
    pub fn undo(&mut self) {
        if let Some(u) = self.undo.take() {
            self.change(u.change);
        }
    }

    /// Applies a change here at once, then writes it to disk.
    fn change(&mut self, ch: Change) {
        ch.apply(&mut self.comments);
        self.pending.push(ch);
        self.flush();
    }

    fn flush(&mut self) {
        let fps = self.fps;
        let Some(store) = &mut self.store else { return };
        // Kept in `pending` (and its file) while the notes file is unreadable; the poll retries.
        if let Err(e) = store.flush(&mut self.comments, &mut self.pending, fps) {
            self.error = Some(e);
        }
    }

    /// Writes changes still waiting, before leaving the video or quitting.
    pub fn save_pending(&mut self) {
        if !self.pending.is_empty() {
            self.flush();
        }
    }

    pub fn focus_comment(&mut self, id: &str) {
        let Some(c) = self.comments.iter().find(|c| c.id == id).cloned() else { return };
        self.pause();
        self.focus_id = Some(c.id.clone());
        self.area = None;
        self.pending_in = None;
        // Scene numbers from another detector may not exist here: keep the time, drop the scenes.
        let scenes = c.scenes.filter(|&(a, b)| self.scene_count() == 0 || (a >= 1 && b <= self.scene_count()));
        self.sel = c.t_out.map(|out| Selection { t_in: c.t_in, t_out: Some(out), scenes });
        self.seek(c.t_in);
    }

    pub fn jump_comment(&mut self, dir: i32) {
        let (t, eps) = (self.position(), 1.0 / self.fps);
        let target = if dir > 0 {
            self.comments.iter().find(|c| c.t_in > t + eps)
        } else {
            self.comments.iter().rev().find(|c| c.t_in < t - eps)
        };
        if let Some(t_in) = target.map(|c| c.t_in) {
            self.pause();
            self.seek(t_in);
        }
    }

    pub fn scene_label(s: (u32, u32)) -> String {
        if s.0 == s.1 { trf("scene.one", &[("n", &s.0)]) } else { trf("scene.many", &[("a", &s.0), ("b", &s.1)]) }
    }
}

/// The displayed size of a `w`×`h` picture with pixel aspect `par` and a
/// clockwise `rotate` in degrees. Like mpv, a wide pixel widens the picture
/// and a tall one makes it taller.
pub fn display_size(w: i64, h: i64, par: f64, rotate: i64) -> (i64, i64) {
    let (w, h) = if par > 1.0 { ((w as f64 * par).round() as i64, h) } else { (w, (h as f64 / par).round() as i64) };
    if rotate.rem_euclid(180) == 90 { (h, w) } else { (w, h) }
}

// ---- scene and span arithmetic (pure, so it can be tested without mpv) -----

/// Scene boundaries: 0, every cut inside the video, the end.
pub fn bounds(cuts: &[f64], duration: f64) -> Vec<f64> {
    let mut b = vec![0.0];
    b.extend(cuts.iter().copied().filter(|c| *c > 0.0 && *c < duration));
    b.push(duration);
    b
}

/// Start and end of scene `n` (1-based), or None when there is no such scene.
pub fn scene_range(cuts: &[f64], duration: f64, n: u32) -> Option<(f64, f64)> {
    let b = bounds(cuts, duration);
    let n = n as usize;
    (n >= 1 && n < b.len()).then(|| (b[n - 1], b[n]))
}

/// Scene (1-based) containing time `t`.
pub fn scene_at(cuts: &[f64], duration: f64, fps: f64, t: f64) -> u32 {
    // By frame: mpv lands on frame starts, which can sit a hair before a cut's rounded time.
    let b = bounds(cuts, duration);
    let f = model::frame_of(t, fps);
    (1..b.len()).find(|&i| f < model::frame_of(b[i], fps)).unwrap_or(b.len() - 1) as u32
}

/// The next (`dir > 0`) or previous boundary more than a frame away from `t`.
pub fn cut_target(cuts: &[f64], duration: f64, fps: f64, t: f64, dir: i32) -> Option<f64> {
    let (b, eps) = (bounds(cuts, duration), 1.0 / fps);
    if dir > 0 { b.into_iter().find(|&x| x > t + eps) } else { b.into_iter().rev().find(|&x| x < t - eps) }
}

/// A range between `a` and `b` in either order; None when shorter than a frame.
pub fn range_selection(a: f64, b: f64, fps: f64) -> Option<Selection> {
    ((a - b).abs() >= 1.0 / fps).then(|| Selection { t_in: a.min(b), t_out: Some(a.max(b)), scenes: None })
}

/// True while `t` is on the note's frame, or inside its range (end exclusive).
pub fn is_active(c: &Comment, t: f64, fps: f64) -> bool {
    let f = |t: f64| model::frame_of(t, fps);
    match c.t_out {
        Some(out) => (f(c.t_in)..f(out)).contains(&f(t)),
        None => f(t) == f(c.t_in),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUTS: [f64; 3] = [2.0, 5.0, 9.0];

    #[test]
    fn display_size_applies_pixel_aspect_and_rotation() {
        assert_eq!(display_size(720, 576, 64.0 / 45.0, 0), (1024, 576), "16:9 PAL");
        assert_eq!(display_size(720, 576, 16.0 / 15.0 * 0.75, 0), (720, 720), "tall pixels grow the height");
        assert_eq!(display_size(1920, 1080, 1.0, 90), (1080, 1920));
        assert_eq!(display_size(1920, 1080, 1.0, 270), (1080, 1920));
        assert_eq!(display_size(1920, 1080, 1.0, 180), (1920, 1080));
        assert_eq!(display_size(720, 576, 64.0 / 45.0, 90), (576, 1024));
    }

    #[test]
    fn bounds_drop_cuts_outside_the_video() {
        assert_eq!(bounds(&[0.0, 2.0, 5.0, 12.0], 10.0), vec![0.0, 2.0, 5.0, 10.0]);
        assert_eq!(bounds(&[], 10.0), vec![0.0, 10.0]);
    }

    #[test]
    fn scene_range_out_of_range_is_none() {
        assert_eq!(scene_range(&CUTS, 10.0, 1), Some((0.0, 2.0)));
        assert_eq!(scene_range(&CUTS, 10.0, 4), Some((9.0, 10.0)));
        assert_eq!(scene_range(&CUTS, 10.0, 0), None, "scene 0 must not underflow");
        assert_eq!(scene_range(&CUTS, 10.0, 5), None);
        assert_eq!(scene_range(&CUTS, 10.0, u32::MAX), None);
        assert_eq!(scene_range(&[], 0.0, 7), None);
    }

    #[test]
    fn scene_at_counts_by_frame() {
        let fps = 25.0;
        assert_eq!(scene_at(&CUTS, 10.0, fps, 0.0), 1);
        assert_eq!(scene_at(&CUTS, 10.0, fps, 1.99), 1);
        assert_eq!(scene_at(&CUTS, 10.0, fps, 2.0), 2);
        // A frame start a hair before the rounded cut time still belongs to the new scene.
        assert_eq!(scene_at(&CUTS, 10.0, fps, 1.99999), 2);
        assert_eq!(scene_at(&CUTS, 10.0, fps, 9.5), 4);
        assert_eq!(scene_at(&CUTS, 10.0, fps, 10.0), 4, "the very end is the last scene");
        assert_eq!(scene_at(&[], 0.0, fps, 3.0), 1);
    }

    #[test]
    fn cut_target_skips_the_cut_under_the_playhead() {
        let fps = 25.0;
        assert_eq!(cut_target(&CUTS, 10.0, fps, 0.0, 1), Some(2.0));
        assert_eq!(cut_target(&CUTS, 10.0, fps, 2.0, 1), Some(5.0));
        assert_eq!(cut_target(&CUTS, 10.0, fps, 2.0, -1), Some(0.0));
        assert_eq!(cut_target(&CUTS, 10.0, fps, 5.01, -1), Some(2.0));
        assert_eq!(cut_target(&CUTS, 10.0, fps, 10.0, 1), None);
        assert_eq!(cut_target(&CUTS, 10.0, fps, 0.0, -1), None);
    }

    #[test]
    fn range_selection_orders_and_ignores_tiny_drags() {
        let s = range_selection(4.0, 1.0, 25.0).unwrap();
        assert_eq!((s.t_in, s.t_out, s.scenes), (1.0, Some(4.0), None));
        assert!(range_selection(1.0, 1.02, 25.0).is_none());
        assert!(range_selection(1.0, 1.05, 25.0).is_some());
    }

    #[test]
    fn is_active_by_frame() {
        let fps = 25.0;
        let moment = Comment { id: "m".into(), text: String::new(), t_in: 2.0, t_out: None, area: None, scenes: None, done: false, created: None };
        assert!(is_active(&moment, 2.0, fps));
        assert!(is_active(&moment, 2.039, fps));
        assert!(!is_active(&moment, 2.04, fps));
        let range = Comment { t_out: Some(4.0), ..moment };
        assert!(is_active(&range, 3.0, fps));
        assert!(!is_active(&range, 4.0, fps), "end is exclusive");
        assert!(!is_active(&range, 1.9, fps));
    }
}
