//! Review data and the files it lives in. JSON-compatible with the Python/web
//! and Flutter versions: `<video>.comments.json` and `<video>.scenes.json`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::i18n::{tr, trf};

pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "webm", "mkv", "mxf", "avi"];

/// Region on the frame, normalized to 0..1. w == h == 0 is a pin.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub w: f64,
    #[serde(default)]
    pub h: f64,
}

impl Area {
    pub fn pin(x: f64, y: f64) -> Self {
        Self { x, y, w: 0.0, h: 0.0 }
    }

    pub fn is_pin(&self) -> bool {
        self.w == 0.0 && self.h == 0.0
    }

    fn rounded(&self) -> Self {
        let r = |v: f64| (v.clamp(0.0, 1.0) * 10000.0).round() / 10000.0;
        Self { x: r(self.x), y: r(self.y), w: r(self.w.min(1.0 - self.x)), h: r(self.h.min(1.0 - self.y)) }
    }
}

/// Time span a comment applies to. `t_out == None` means a single moment.
/// `scenes` holds the first and last (1-based) scene numbers when the span
/// was chosen as whole scenes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Selection {
    pub t_in: f64,
    pub t_out: Option<f64>,
    pub scenes: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub text: String,
    pub t_in: f64,
    pub t_out: Option<f64>,
    pub area: Option<Area>,
    pub scenes: Option<(u32, u32)>,
    pub done: bool,
    pub created: Option<String>,
}

impl Comment {
    pub fn new_id() -> String {
        use std::hash::{BuildHasher, Hasher};
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_nanos());
        format!("{:08x}", h.finish() as u32)
    }
}

#[derive(Serialize, Deserialize)]
struct CommentJson {
    #[serde(default)]
    text: String,
    t_in: f64,
    #[serde(default)]
    t_out: Option<f64>,
    #[serde(default)]
    area: Option<Area>,
    #[serde(default)]
    scenes: Option<Vec<u32>>,
    #[serde(default)]
    frame_in: Option<i64>,
    #[serde(default)]
    frame_out: Option<i64>,
    #[serde(default)]
    done: bool,
    id: String,
    #[serde(default)]
    created: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct FileJson {
    #[serde(default)]
    video: String,
    #[serde(default)]
    fps: f64,
    #[serde(default)]
    comments: Vec<CommentJson>,
}

// ---- time ------------------------------------------------------------------

/// Frame index containing time `t`. The small epsilon absorbs float error in
/// presentation timestamps (frame n starts at n / fps).
pub fn frame_of(t: f64, fps: f64) -> i64 {
    (t * fps + 1e-3).floor() as i64
}

/// Non-drop-frame timecode HH:MM:SS:FF, counted at the nominal (rounded) rate
/// the way NLEs label 29.97 fps material.
pub fn timecode(t: f64, fps: f64) -> String {
    let nominal = (fps.round() as i64).clamp(1, 1000);
    let f = frame_of(t.max(0.0), fps);
    let s = f / nominal;
    format!("{:02}:{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60, f % nominal)
}

/// Duration label, e.g. "4.0 s".
pub fn secs(a: f64, b: f64) -> String {
    trf("time.seconds", &[("s", &format!("{:.1}", b - a))])
}

// ---- files -----------------------------------------------------------------

fn is_video(p: &Path) -> bool {
    let hidden = p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'));
    let ext = p.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
    !hidden && VIDEO_EXTENSIONS.contains(&ext.as_str())
}

/// Expands files and folders into a sorted list of video files.
pub fn collect_videos(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut files: Vec<PathBuf> =
                std::fs::read_dir(path).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_file() && is_video(p)).collect();
            files.sort();
            out.extend(files);
        } else if path.is_file() && is_video(path) {
            out.push(path.clone());
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|p| seen.insert(p.clone()));
    out
}

fn sidecar(video: &Path, suffix: &str) -> PathBuf {
    let mut s = video.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Writes `bytes` to a `.tmp` sidecar and renames it over `path`, so a crash mid-write
/// never leaves a truncated file for this or the other app versions to read.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = sidecar(path, ".tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// One edit to the notes, keyed by id, so it can be applied again on top of
/// a list freshly read from disk (when someone else wrote the file meanwhile).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Change {
    /// Adds the note, or replaces the one with the same id (also undoes a delete).
    Put(Comment),
    Text {
        id: String,
        text: String,
    },
    Done {
        id: String,
        done: bool,
    },
    Remove(String),
}

/// The order notes are listed, navigated and saved in. Stable, so notes at the same time keep their order.
fn sort_by_time(list: &mut [Comment]) {
    list.sort_by(|a, b| a.t_in.total_cmp(&b.t_in));
}

impl Change {
    /// Keeps `list` in [`sort_by_time`] order, also when the result can't be saved.
    pub fn apply(&self, list: &mut Vec<Comment>) {
        match self {
            Change::Put(c) => {
                match list.iter_mut().find(|x| x.id == c.id) {
                    Some(x) => *x = c.clone(),
                    None => list.push(c.clone()),
                }
                sort_by_time(list);
            }
            Change::Text { id, text } => {
                if let Some(c) = list.iter_mut().find(|c| &c.id == id) {
                    c.text = text.clone();
                }
            }
            Change::Done { id, done } => {
                if let Some(c) = list.iter_mut().find(|c| &c.id == id) {
                    c.done = *done;
                }
            }
            Change::Remove(id) => list.retain(|c| &c.id != id),
        }
    }
}

/// Whether a text or done edit in `changes` (already applied to `list`) hit a
/// note that is gone, i.e. was deleted elsewhere, so the edit was dropped. A
/// later `Remove` of the same note in `changes` is a local delete, not a loss.
pub fn edits_lost(changes: &[Change], list: &[Comment]) -> bool {
    changes.iter().enumerate().any(|(i, ch)| match ch {
        Change::Text { id, .. } | Change::Done { id, .. } => {
            !list.iter().any(|c| &c.id == id) && !changes[i + 1..].iter().any(|l| matches!(l, Change::Remove(r) if r == id))
        }
        _ => false,
    })
}

/// Reads and writes `<video>.comments.json` next to the video.
pub struct CommentStore {
    video: PathBuf,
    /// The file's bytes as last read or written (`None`: missing or unreadable).
    /// Compared by content, not mtime: FAT/exFAT and some network shares have
    /// 1-2 s timestamps, so another app's write could keep the same mtime.
    known: Option<Vec<u8>>,
    /// The file exists but could not be read: never write over it until it parses again.
    read_only: bool,
    /// Copy the file to `.bak` before the next write, because it was broken once.
    needs_backup: bool,
    /// The `fps` in the file as last read or written; kept while ours is only an estimate.
    file_fps: Option<f64>,
}

/// The frame rate a save writes, with how sure the player is about it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fps {
    /// From the container: written as is.
    Known(f64),
    /// A guess (decoder estimate or the 24 fps default): the file's own value wins.
    Estimate(f64),
}

impl CommentStore {
    pub fn new(video: &Path) -> Self {
        Self { video: video.to_owned(), known: None, read_only: false, needs_backup: false, file_fps: None }
    }

    pub fn file(&self) -> PathBuf {
        sidecar(&self.video, ".comments.json")
    }

    pub fn backup_file(&self) -> PathBuf {
        sidecar(&self.video, ".comments.json.bak")
    }

    /// Changes that could not be written yet, kept so they survive a video switch or quit.
    /// Only this app reads it; the notes file format stays untouched.
    pub fn pending_file(&self) -> PathBuf {
        sidecar(&self.video, ".comments.pending.json")
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    fn current(&self) -> Option<Vec<u8>> {
        std::fs::read(self.file()).ok()
    }

    /// A missing file is an empty list; a file that can't be read or parsed is
    /// an error and makes the store read-only until a later load succeeds.
    pub fn load(&mut self) -> Result<Vec<Comment>, String> {
        let read = std::fs::read(self.file());
        let parsed = match &read {
            Ok(raw) => serde_json::from_slice::<FileJson>(raw).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(FileJson { video: String::new(), fps: 0.0, comments: Vec::new() }),
            Err(e) => Err(e.to_string()),
        };
        self.known = read.ok();
        let data = match parsed {
            Ok(d) => d,
            Err(e) => {
                self.read_only = true;
                self.needs_backup = true;
                let name = self.file().file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                return Err(trf("error.notes_unreadable", &[("name", &name), ("err", &e)]));
            }
        };
        self.read_only = false;
        self.file_fps = Some(data.fps).filter(|f| *f > 1.0 && *f < 1000.0);
        let mut list: Vec<Comment> = data
            .comments
            .into_iter()
            .map(|j| {
                let scenes = j.scenes.filter(|s| !s.is_empty()).map(|mut s| {
                    s.sort_unstable();
                    (s[0], s[s.len() - 1])
                });
                Comment { id: j.id, text: j.text, t_in: j.t_in, t_out: j.t_out, area: j.area, scenes, done: j.done, created: j.created }
            })
            .collect();
        sort_by_time(&mut list);
        Ok(list)
    }

    /// Writes `changes` (already applied to `comments`) to disk. If the file
    /// changed on disk or was unreadable, it is read again first and the
    /// changes are applied on top, so an outside edit is not lost.
    pub fn commit(&mut self, comments: &mut Vec<Comment>, changes: &[Change], fps: Fps) -> Result<(), String> {
        if self.read_only || self.changed_externally() {
            let mut fresh = self.load()?;
            for ch in changes {
                ch.apply(&mut fresh);
            }
            *comments = fresh;
        }
        self.save(comments, fps).map_err(|e| trf("error.save", &[("err", &e)]))
    }

    /// Commits `pending`, clearing it on success. Whatever is left is written to
    /// [`Self::pending_file`] (removed once empty), so it is not lost when the app moves on.
    pub fn flush(&mut self, comments: &mut Vec<Comment>, pending: &mut Vec<Change>, fps: Fps) -> Result<(), String> {
        let result = self.commit(comments, pending, fps);
        if result.is_ok() {
            pending.clear();
        }
        let file = self.pending_file();
        let kept = if pending.is_empty() {
            match std::fs::remove_file(&file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        } else {
            serde_json::to_vec_pretty(pending).map_err(std::io::Error::from).and_then(|b| write_atomic(&file, &b))
        };
        result.and(kept.map_err(|e| trf("error.save", &[("err", &e)])))
    }

    /// Changes a previous session could not write (see [`Self::flush`]).
    pub fn load_pending(&self) -> Vec<Change> {
        std::fs::read(self.pending_file()).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default()
    }

    /// Until the real frame rate is known, the file's `fps` (and the frame numbers
    /// from it) is kept, so values written by the other versions aren't replaced by a guess.
    pub fn save(&mut self, comments: &mut [Comment], fps: Fps) -> std::io::Result<()> {
        if self.read_only {
            return Err(std::io::Error::other(tr("error.notes_broken")));
        }
        let fps = match fps {
            Fps::Known(f) => f,
            Fps::Estimate(f) => self.file_fps.unwrap_or(f),
        };
        sort_by_time(comments);
        let payload = FileJson {
            video: self.video.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            fps,
            comments: comments
                .iter()
                .map(|c| CommentJson {
                    text: c.text.clone(),
                    t_in: c.t_in,
                    t_out: c.t_out,
                    area: c.area.map(|a| a.rounded()),
                    scenes: c.scenes.map(|(a, b)| vec![a, b]),
                    frame_in: Some(frame_of(c.t_in, fps)),
                    frame_out: c.t_out.map(|t| frame_of(t, fps)),
                    done: c.done,
                    id: c.id.clone(),
                    created: c.created.clone(),
                })
                .collect(),
        };
        let file = self.file();
        if self.needs_backup && file.exists() {
            std::fs::copy(&file, self.backup_file())?;
        }
        self.needs_backup = false;
        let bytes = serde_json::to_string_pretty(&payload)?.into_bytes();
        write_atomic(&file, &bytes)?;
        // What we wrote, not a re-read: a write landing right after the rename still counts as outside.
        self.known = Some(bytes);
        self.file_fps = Some(fps);
        Ok(())
    }

    /// True when someone else (another app, Claude, the web version) changed the file.
    pub fn changed_externally(&self) -> bool {
        self.current() != self.known
    }
}

/// GUI apps don't inherit the shell PATH, so look in the usual places too.
pub fn find_ffmpeg() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from))
        .map(|d| d.join(exe))
        .find(|p| p.is_file())
}

/// Hard-cut detection with ffmpeg's scene score, cached in `<video>.scenes.json`.
pub mod scenes {
    use super::*;
    use std::io::Read;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex, mpsc};

    pub const THRESHOLD: f64 = 0.25; // found 65/66 cuts on the Hangulat reel

    /// The message for a missing ffmpeg, so the UI can tell it from other failures.
    pub const NO_FFMPEG: &str = "ffmpeg not found";

    #[derive(Serialize, Deserialize)]
    struct Cache {
        threshold: f64,
        cuts: Vec<f64>,
        /// The video's size and mtime (seconds) when detected; a cache without them is stale.
        #[serde(default)]
        size: Option<u64>,
        #[serde(default)]
        mtime: Option<f64>,
    }

    fn file(video: &Path) -> PathBuf {
        sidecar(video, ".scenes.json")
    }

    /// Size and modification time of the video, the key the cache is valid for.
    pub(super) fn stamp(video: &Path) -> Option<(u64, f64)> {
        let m = std::fs::metadata(video).ok()?;
        let t = m.modified().ok()?.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs_f64();
        Some((m.len(), t))
    }

    /// Cuts from the cache, if it was made for this very file with the current threshold.
    pub fn cached(video: &Path) -> Option<Vec<f64>> {
        let raw = std::fs::read_to_string(file(video)).ok()?;
        let c = serde_json::from_str::<Cache>(&raw).ok()?;
        let (size, mtime) = stamp(video)?;
        (c.threshold == THRESHOLD && c.size == Some(size) && c.mtime == Some(mtime)).then_some(c.cuts)
    }

    /// Drops the cache so the next detection starts from scratch.
    pub fn forget(video: &Path) {
        let _ = std::fs::remove_file(file(video));
    }

    /// Caches `cuts` found in the version of the file that had the `analyzed` stamp, unless
    /// the video has changed since (re-exported or still being written during detection).
    pub(super) fn store(video: &Path, cuts: &[f64], analyzed: Option<(u64, f64)>) {
        let Some((size, mtime)) = analyzed.filter(|&a| stamp(video) == Some(a)) else { return };
        let cache = Cache { threshold: THRESHOLD, cuts: cuts.to_vec(), size: Some(size), mtime: Some(mtime) };
        if let Ok(bytes) = serde_json::to_vec_pretty(&cache) {
            let _ = write_atomic(&file(video), &bytes);
        }
    }

    pub(super) fn parse(text: &str) -> Vec<f64> {
        let mut cuts: Vec<f64> = text
            .split("pts_time:")
            .skip(1)
            .filter_map(|s| {
                let num: String = s.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                num.parse::<f64>().ok()
            })
            .map(|t| (t * 1000.0).round() / 1000.0)
            .collect();
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        cuts
    }

    fn spawn(video: &Path) -> Result<Child, String> {
        let ffmpeg = find_ffmpeg().ok_or(NO_FFMPEG)?;
        let mut cmd = Command::new(ffmpeg);
        cmd.args(["-hide_banner", "-nostats", "-loglevel", "error", "-progress", "pipe:2"]);
        if cfg!(target_os = "macos") {
            cmd.args(["-hwaccel", "videotoolbox"]);
        }
        // The cuts go to stdout from a sink; the other branch outputs 2 fps so `-progress`
        // has an advancing `out_time_us` (select alone outputs almost nothing).
        cmd.arg("-i").arg(video).args([
            "-filter_complex",
            &format!("[0:v]scale=320:-2,split[a][b];[a]select='gt(scene,{THRESHOLD})',metadata=print:file=-,nullsink;[b]fps=2[p]"),
            "-map",
            "[p]",
            "-f",
            "null",
            "-",
        ]);
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.spawn().map_err(|e| e.to_string())
    }

    /// `(stopped, process)`: shared so the UI thread can kill ffmpeg mid-decode.
    type Slot = Arc<Mutex<(bool, Option<Child>)>>;

    /// One detection running in the background. Dropping it kills ffmpeg.
    pub struct Detection {
        pub video: PathBuf,
        slot: Slot,
        rx: mpsc::Receiver<Result<Vec<f64>, String>>,
        /// How far ffmpeg has decoded, in microseconds of video time.
        done_us: Arc<AtomicU64>,
    }

    impl Detection {
        /// Starts ffmpeg on a thread; `done` runs there once the result is ready.
        pub fn start(video: &Path, done: impl Fn() + Send + 'static) -> Self {
            let slot: Slot = Arc::new(Mutex::new((false, None)));
            let (tx, rx) = mpsc::channel();
            let done_us = Arc::new(AtomicU64::new(0));
            let (v, s, d) = (video.to_owned(), slot.clone(), done_us.clone());
            std::thread::spawn(move || {
                let _ = tx.send(run(&v, &s, &d));
                done();
            });
            Self { video: video.to_owned(), slot, rx, done_us }
        }

        /// Seconds of video ffmpeg has got through so far.
        pub fn progress(&self) -> f64 {
            self.done_us.load(Ordering::Relaxed) as f64 / 1e6
        }

        /// The result, once; None while ffmpeg is still running.
        pub fn poll(&self) -> Option<Result<Vec<f64>, String>> {
            self.rx.try_recv().ok()
        }

        pub fn stop(&self) {
            let mut s = self.slot.lock().unwrap_or_else(|e| e.into_inner());
            s.0 = true;
            if let Some(child) = &mut s.1 {
                let _ = child.kill();
            }
        }
    }

    impl Drop for Detection {
        fn drop(&mut self) {
            self.stop();
        }
    }

    fn run(video: &Path, slot: &Slot, done_us: &Arc<AtomicU64>) -> Result<Vec<f64>, String> {
        // Taken before ffmpeg opens the file, so the cache names the version it read.
        let analyzed = stamp(video);
        let (stdout, stderr) = {
            let mut s = slot.lock().unwrap_or_else(|e| e.into_inner());
            if s.0 {
                return Err("stopped".into());
            }
            let mut child = spawn(video)?;
            let out = (child.stdout.take(), child.stderr.take());
            s.1 = Some(child);
            out
        };
        // `-progress` writes `out_time_us=…` lines to stderr; read them on their own thread
        // so neither pipe can fill up and stall ffmpeg.
        if let Some(err) = stderr {
            let d = done_us.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<u64>().ok()) {
                        d.store(us, Ordering::Relaxed);
                    }
                }
            });
        }
        // Read without the lock so `stop` can kill ffmpeg meanwhile; that ends the stream.
        let mut text = String::new();
        if let Some(mut out) = stdout {
            let _ = out.read_to_string(&mut text);
        }
        let mut s = slot.lock().unwrap_or_else(|e| e.into_inner());
        let status = s.1.as_mut().map(|c| c.wait()).transpose().map_err(|e| e.to_string())?;
        if s.0 {
            return Err("stopped".into());
        }
        match status {
            Some(st) if !st.success() => return Err(format!("ffmpeg failed: {st}")),
            _ => {}
        }
        let cuts = parse(&text);
        store(video, &cuts, analyzed);
        Ok(cuts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_and_timecode() {
        let fps = 30000.0 / 1001.0;
        assert_eq!(frame_of(5.005, fps), 150);
        assert_eq!(frame_of(0.0, fps), 0);
        assert_eq!(timecode(5.005, fps), "00:00:05:00");
        assert_eq!(timecode(62.5, 24.0), "00:01:02:12");
        assert_eq!(secs(1.0, 5.0), "4.0 s");
    }

    #[test]
    fn collect_videos_filters_and_sorts() {
        let dir = std::env::temp_dir().join(format!("vr-test-{}", Comment::new_id()));
        let (cut, loose) = (dir.join("cut"), dir.join("loose"));
        std::fs::create_dir_all(cut.join("sub")).unwrap();
        std::fs::create_dir_all(cut.join("folder.mkv")).unwrap();
        std::fs::create_dir_all(&loose).unwrap();
        for f in ["b.MOV", "a.mp4", "c.webm", ".hidden.mp4", "notes.txt", "a.mp4.comments.json", "noext", "sub/deep.mp4"] {
            std::fs::write(cut.join(f), b"").unwrap();
        }
        for f in ["z.mxf", "y.avi", ".x.mov", "w.png"] {
            std::fs::write(loose.join(f), b"").unwrap();
        }
        let p = |rel: &str| dir.join(rel);
        let cut_videos = || vec![p("cut/a.mp4"), p("cut/b.MOV"), p("cut/c.webm")];
        let cases: Vec<(Vec<PathBuf>, Vec<PathBuf>)> = vec![
            // A folder: only its visible video files (any extension case), sorted,
            // no subfolders (not even one named like a video), not recursive.
            (vec![cut.clone()], cut_videos()),
            // Files are kept in argument order; hidden and non-video files are dropped.
            (
                vec![p("loose/z.mxf"), p("loose/y.avi"), p("loose/.x.mov"), p("loose/w.png"), p("cut/notes.txt")],
                vec![p("loose/z.mxf"), p("loose/y.avi")],
            ),
            // Folders and files mixed: each argument expands in place, duplicates
            // keep their first position, missing paths are skipped.
            (
                vec![p("loose/z.mxf"), cut.clone(), p("cut/b.MOV"), p("missing.mp4"), p("loose/z.mxf")],
                [vec![p("loose/z.mxf")], cut_videos()].concat(),
            ),
            (vec![p("cut/sub")], vec![p("cut/sub/deep.mp4")]),
            (vec![p("cut/folder.mkv"), p("missing")], vec![]),
            (vec![], vec![]),
        ];
        for (args, want) in cases {
            assert_eq!(collect_videos(&args), want, "{args:?}");
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn json_round_trip_keeps_format() {
        let dir = std::env::temp_dir().join(format!("vr-test-{}", Comment::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("a.mp4");
        let mut store = CommentStore::new(&video);
        let mut list = vec![
            Comment {
                id: "b".into(),
                text: "later".into(),
                t_in: 62.5,
                t_out: Some(75.97),
                area: Some(Area { x: 0.123456, y: 0.5, w: 0.95, h: 0.2 }),
                scenes: Some((3, 4)),
                done: false,
                created: None,
            },
            Comment {
                id: "a".into(),
                text: "first".into(),
                t_in: 25.36,
                t_out: None,
                area: None,
                scenes: None,
                done: true,
                created: Some("2026-10-16T01:00:00".into()),
            },
        ];
        store.save(&mut list, Fps::Known(24.0)).unwrap();
        let raw = std::fs::read_to_string(store.file()).unwrap();
        assert!(raw.contains("\"frame_in\": 608"), "{raw}");
        assert!(raw.contains("\"x\": 0.1235"));
        assert!(raw.contains("\"w\": 0.8765"), "area clamped to the frame: {raw}");
        assert!(raw.find("\"text\"").unwrap() < raw.find("\"t_in\"").unwrap());
        let back = store.load().unwrap();
        assert_eq!(back[0].id, "a");
        assert_eq!(back[1].scenes, Some((3, 4)));
        assert!(!store.changed_externally());
        std::fs::remove_dir_all(dir).ok();
    }

    fn note(id: &str, t_in: f64) -> Comment {
        Comment { id: id.into(), text: id.into(), t_in, t_out: None, area: None, scenes: None, done: false, created: None }
    }

    fn temp_video() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("vr-test-{}", Comment::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("a.mp4");
        (dir, video)
    }

    #[test]
    fn missing_file_is_empty_not_an_error() {
        let (dir, video) = temp_video();
        let mut store = CommentStore::new(&video);
        assert!(store.load().unwrap().is_empty());
        assert!(!store.read_only());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn bad_json_is_never_overwritten_and_backed_up() {
        let (dir, video) = temp_video();
        let mut store = CommentStore::new(&video);
        let bad = r#"{"comments": [{"t_in": 1.0, "text": "no id"},]}"#;
        std::fs::write(store.file(), bad).unwrap();
        assert!(store.load().is_err());
        assert!(store.read_only());

        let mut list = vec![note("x", 2.0)];
        assert!(store.save(&mut list, Fps::Known(24.0)).is_err());
        assert!(store.commit(&mut list, &[Change::Put(note("x", 2.0))], Fps::Known(24.0)).is_err());
        assert_eq!(std::fs::read_to_string(store.file()).unwrap(), bad, "the broken file stays as it was");
        assert_eq!(list.len(), 1, "the in-memory list is kept");

        // Someone fixes the file: the next write goes through, after a backup.
        let fixed = r#"{"comments": [{"t_in": 1.0, "text": "fixed", "id": "f"}]}"#;
        std::fs::write(store.file(), fixed).unwrap();
        store.commit(&mut list, &[Change::Put(note("x", 2.0))], Fps::Known(24.0)).unwrap();
        assert!(!store.read_only());
        assert_eq!(list.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["f", "x"]);
        assert_eq!(std::fs::read_to_string(store.backup_file()).unwrap(), fixed);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn unsaved_changes_survive_until_the_file_is_fixed() {
        let (dir, video) = temp_video();
        let bad = r#"{"comments": [}"#;
        let mut store = CommentStore::new(&video);
        std::fs::write(store.file(), bad).unwrap();
        let mut list = store.load().unwrap_or_default();
        let ch = Change::Put(note("x", 2.0));
        ch.apply(&mut list);
        let mut pending = vec![ch];
        assert!(store.flush(&mut list, &mut pending, Fps::Known(24.0)).is_err());
        assert_eq!(pending.len(), 1, "kept in memory");
        assert_eq!(std::fs::read_to_string(store.file()).unwrap(), bad);

        // Switch away and back (or quit and reopen): a new store finds them.
        let mut store = CommentStore::new(&video);
        let mut pending = store.load_pending();
        assert_eq!(pending.len(), 1, "kept on disk");
        std::fs::write(store.file(), r#"{"comments": [{"t_in": 1.0, "text": "fixed", "id": "f"}]}"#).unwrap();
        let mut list = store.load().unwrap();
        for ch in &pending {
            ch.apply(&mut list);
        }
        store.flush(&mut list, &mut pending, Fps::Known(24.0)).unwrap();
        assert!(pending.is_empty());
        assert!(!store.pending_file().exists(), "removed once written");
        assert_eq!(store.load().unwrap().iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["f", "x"]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn outside_edit_survives_a_save() {
        let (dir, video) = temp_video();
        let mut store = CommentStore::new(&video);
        let mut list = vec![note("a", 1.0), note("b", 2.0)];
        store.save(&mut list, Fps::Known(24.0)).unwrap();

        // Another tool adds "c" and edits "a"; make sure the mtime differs.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut theirs = vec![note("a", 1.0), note("b", 2.0), note("c", 3.0)];
        theirs[0].text = "edited elsewhere".into();
        CommentStore::new(&video).save(&mut theirs, Fps::Known(24.0)).unwrap();
        assert!(store.changed_externally());

        // Here, "b" is marked done before the poll noticed anything.
        let ch = Change::Done { id: "b".into(), done: true };
        ch.apply(&mut list);
        store.commit(&mut list, &[ch], Fps::Known(24.0)).unwrap();

        let back = store.load().unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back[0].text, "edited elsewhere");
        assert!(back[1].done);
        assert_eq!(back[2].id, "c");
        assert_eq!(list.len(), 3, "the in-memory list picks up the outside edit too");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn outside_edit_with_the_same_mtime_is_noticed() {
        let (dir, video) = temp_video();
        let mut store = CommentStore::new(&video);
        let mut list = vec![note("a", 1.0)];
        store.save(&mut list, Fps::Known(24.0)).unwrap();
        let mtime = std::fs::metadata(store.file()).unwrap().modified().unwrap();

        // Another app writes within the same timestamp tick (coarse FAT/SMB mtimes).
        let mut theirs = vec![note("a", 1.0), note("c", 3.0)];
        CommentStore::new(&video).save(&mut theirs, Fps::Known(24.0)).unwrap();
        std::fs::File::options().write(true).open(store.file()).unwrap().set_modified(mtime).unwrap();
        assert!(store.changed_externally());

        let ch = Change::Put(note("b", 2.0));
        ch.apply(&mut list);
        store.commit(&mut list, &[ch], Fps::Known(24.0)).unwrap();
        let ids: Vec<_> = store.load().unwrap().into_iter().map(|c| c.id).collect();
        assert_eq!(ids, ["a", "b", "c"]);
        assert!(!store.changed_externally());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn estimated_fps_keeps_the_files_rate() {
        let (dir, video) = temp_video();
        let raw = |store: &CommentStore| std::fs::read_to_string(store.file()).unwrap();
        // Written by another version that knew the rate.
        let mut store = CommentStore::new(&video);
        store.save(&mut [note("a", 2.0)], Fps::Known(25.0)).unwrap();

        // Reopened before the container rate is known: the file's 25 fps stays.
        let mut store = CommentStore::new(&video);
        let mut list = store.load().unwrap();
        let ch = Change::Put(note("b", 4.0));
        ch.apply(&mut list);
        store.commit(&mut list, &[ch], Fps::Estimate(29.5)).unwrap();
        assert!(raw(&store).contains("\"fps\": 25.0"), "{}", raw(&store));
        assert!(raw(&store).contains("\"frame_in\": 100"), "frames from the file's rate");

        // Once known, the real rate is written.
        store.save(&mut list, Fps::Known(30.0)).unwrap();
        assert!(raw(&store).contains("\"fps\": 30.0"));
        assert!(raw(&store).contains("\"frame_in\": 120"));

        // A file without a rate takes the estimate.
        let (dir2, video2) = temp_video();
        let mut store = CommentStore::new(&video2);
        store.load().unwrap();
        store.save(&mut [note("a", 2.0)], Fps::Estimate(29.5)).unwrap();
        assert!(raw(&store).contains("\"fps\": 29.5"));
        assert!(raw(&store).contains("\"frame_in\": 59"));
        std::fs::remove_dir_all(dir).ok();
        std::fs::remove_dir_all(dir2).ok();
    }

    #[test]
    fn changes_apply_by_id() {
        let mut list = vec![note("a", 1.0), note("b", 2.0)];
        Change::Text { id: "a".into(), text: "new".into() }.apply(&mut list);
        Change::Done { id: "b".into(), done: true }.apply(&mut list);
        Change::Remove("missing".into()).apply(&mut list);
        assert_eq!(list[0].text, "new");
        assert!(list[1].done);
        let removed = list[0].clone();
        Change::Remove("a".into()).apply(&mut list);
        assert_eq!(list.len(), 1);
        Change::Put(removed.clone()).apply(&mut list);
        Change::Put(removed).apply(&mut list);
        assert_eq!(list.len(), 2, "putting back twice keeps one copy");
    }

    #[test]
    fn edit_of_a_note_deleted_elsewhere_is_reported() {
        let (dir, video) = temp_video();
        let mut store = CommentStore::new(&video);
        let mut list = vec![note("a", 1.0), note("b", 2.0)];
        store.save(&mut list, Fps::Known(24.0)).unwrap();
        // Another app deletes "a" while it is being edited here.
        let mut other = CommentStore::new(&video);
        let mut theirs = other.load().unwrap();
        theirs.retain(|c| c.id != "a");
        other.save(&mut theirs, Fps::Known(24.0)).unwrap();
        let edits = [Change::Text { id: "a".into(), text: "new".into() }, Change::Done { id: "b".into(), done: true }];
        for ch in &edits {
            ch.apply(&mut list);
        }
        assert!(!edits_lost(&edits, &list), "still there before the commit");
        store.commit(&mut list, &edits, Fps::Known(24.0)).unwrap();
        assert_eq!(list.len(), 1);
        assert!(edits_lost(&edits, &list));
        assert!(!edits_lost(&edits[1..], &list), "the edit of a note that is still there is not lost");
        // An edit followed by a local delete of the same note is not a loss.
        let deleted = [Change::Text { id: "b".into(), text: "x".into() }, Change::Remove("b".into())];
        assert!(!edits_lost(&deleted, &[]));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn put_keeps_time_order_while_the_file_is_broken() {
        let (dir, video) = temp_video();
        let mut store = CommentStore::new(&video);
        std::fs::write(store.file(), "{").unwrap();
        assert!(store.load().is_err());
        let mut list = vec![note("a", 5.0), note("c", 60.0)];
        let mut pending = Vec::new();
        for ch in [Change::Put(note("b", 50.0)), Change::Put(note("a", 70.0))] {
            ch.apply(&mut list);
            pending.push(ch);
            assert!(store.flush(&mut list, &mut pending, Fps::Known(24.0)).is_err());
        }
        let ids: Vec<&str> = list.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["b", "c", "a"], "a new or moved note goes in by time, like save() orders it");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn write_atomic_replaces_and_leaves_no_sidecar() {
        let (dir, video) = temp_video();
        let file = sidecar(&video, ".scenes.json");
        write_atomic(&file, b"old").unwrap();
        write_atomic(&file, b"new").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"new");
        assert!(!sidecar(&file, ".tmp").exists());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn scene_cache_is_keyed_on_the_file_and_threshold() {
        let (dir, video) = temp_video();
        std::fs::write(&video, b"first cut").unwrap();
        let cache = sidecar(&video, ".scenes.json");

        // Written by an older version: no size or mtime, so it can't be trusted.
        std::fs::write(&cache, format!(r#"{{"threshold": {}, "cuts": [1.0]}}"#, scenes::THRESHOLD)).unwrap();
        assert_eq!(scenes::cached(&video), None);

        scenes::store(&video, &[1.0, 2.5], scenes::stamp(&video));
        assert_eq!(scenes::cached(&video), Some(vec![1.0, 2.5]));

        // Another threshold makes it a miss.
        let raw = std::fs::read_to_string(&cache).unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        v["threshold"] = serde_json::json!(0.4);
        std::fs::write(&cache, v.to_string()).unwrap();
        assert_eq!(scenes::cached(&video), None);

        // A new export over the same name (other size) makes it a miss.
        scenes::store(&video, &[1.0, 2.5], scenes::stamp(&video));
        std::fs::write(&video, b"the re-exported, longer cut").unwrap();
        assert_eq!(scenes::cached(&video), None);

        // Same size, new mtime too.
        scenes::store(&video, &[1.0, 2.5], scenes::stamp(&video));
        let f = std::fs::File::options().write(true).open(&video).unwrap();
        f.set_modified(SystemTime::now() + std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(scenes::cached(&video), None);

        // The file changed while ffmpeg read it: the cuts belong to the old version.
        scenes::forget(&video);
        let before = scenes::stamp(&video);
        std::fs::write(&video, b"still being rendered").unwrap();
        scenes::store(&video, &[4.0], before);
        assert_eq!(scenes::cached(&video), None);
        assert!(!cache.exists());

        scenes::store(&video, &[3.0], scenes::stamp(&video));
        scenes::forget(&video);
        assert_eq!(scenes::cached(&video), None);
        assert!(!cache.exists());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn scene_cuts_parse_from_ffmpeg_metadata() {
        let out = "frame:0 pts:48 pts_time:2.0021\nlavfi.scene_score=0.9\nframe:1 pts:120 pts_time:5\nframe:2 pts:120 pts_time:5.0\n";
        assert_eq!(scenes::parse(out), vec![2.002, 5.0]);
    }

    #[test]
    fn detection_finds_a_cut_and_can_be_stopped() {
        let Some(ffmpeg) = find_ffmpeg() else { return };
        let (dir, video) = temp_video();
        let ok = Command::new(ffmpeg)
            .args(["-v", "error", "-f", "lavfi", "-i", "color=red:s=64x64:r=25:d=1", "-f", "lavfi", "-i", "color=blue:s=64x64:r=25:d=1"])
            .args(["-filter_complex", "[0][1]concat=n=2:v=1:a=0", "-pix_fmt", "yuv420p"])
            .arg(&video)
            .status()
            .is_ok_and(|s| s.success());
        assert!(ok);

        let d = scenes::Detection::start(&video, || {});
        let cuts = loop {
            if let Some(r) = d.poll() {
                break r.unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert_eq!(cuts, vec![1.0]);
        assert_eq!(scenes::cached(&video), Some(vec![1.0]));

        // Stopped before ffmpeg starts: no result is stored.
        scenes::forget(&video);
        let d = scenes::Detection::start(&video, || {});
        d.stop();
        let r = loop {
            if let Some(r) = d.poll() {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        if r.is_err() {
            assert_eq!(scenes::cached(&video), None);
        }
        std::fs::remove_dir_all(dir).ok();
    }
}
