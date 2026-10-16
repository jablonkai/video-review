//! Small frame grabs (filmstrip, note previews) made with ffmpeg and cached on
//! disk in the app cache, keyed by video identity and time in milliseconds.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, Once, mpsc};
use std::time::SystemTime;

use egui::{ColorImage, TextureHandle, TextureOptions};
use sha1::{Digest, Sha1};

const WIDTH: u32 = 240;
const PARALLEL: usize = 3;
/// The cache is pruned to this size, oldest-used video first.
const CACHE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

fn cache_root() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("video-review").join("thumbs")
}

struct Job {
    generation: u64,
    ms: i64,
    video: PathBuf,
    dir: PathBuf,
}

struct Queue {
    jobs: Mutex<VecDeque<Job>>,
    ready: Condvar,
}

pub struct Thumb {
    pub color: TextureHandle,
    gray: Option<TextureHandle>,
    image: Arc<ColorImage>,
}

pub struct Thumbnails {
    ctx: egui::Context,
    queue: Arc<Queue>,
    results: mpsc::Receiver<(u64, i64, ColorImage)>,
    images: HashMap<i64, Thumb>,
    requested: HashSet<i64>,
    generation: u64,
    video: Option<PathBuf>,
    dir: Option<PathBuf>,
}

impl Thumbnails {
    pub fn new(ctx: egui::Context) -> Self {
        let queue = Arc::new(Queue { jobs: Mutex::new(VecDeque::new()), ready: Condvar::new() });
        let (tx, results) = mpsc::channel();
        static PRUNE: Once = Once::new();
        PRUNE.call_once(|| {
            std::thread::spawn(|| prune(&cache_root(), CACHE_LIMIT));
        });
        if let Some(ffmpeg) = crate::model::find_ffmpeg() {
            for _ in 0..PARALLEL {
                let (q, tx, ffmpeg, ctx) = (queue.clone(), tx.clone(), ffmpeg.clone(), ctx.clone());
                std::thread::spawn(move || {
                    loop {
                        let job = {
                            let mut jobs = q.jobs.lock().unwrap();
                            loop {
                                if let Some(j) = jobs.pop_front() {
                                    break j;
                                }
                                jobs = q.ready.wait(jobs).unwrap();
                            }
                        };
                        if let Some(img) = make(&ffmpeg, &job) {
                            if tx.send((job.generation, job.ms, img)).is_err() {
                                return;
                            }
                            // Dozens can land within a frame; one repaint shortly after covers them.
                            ctx.request_repaint_after(std::time::Duration::from_millis(100));
                        }
                    }
                });
            }
        }
        Self {
            ctx,
            queue,
            results,
            images: HashMap::new(),
            requested: HashSet::new(),
            generation: 0,
            video: None,
            dir: None,
        }
    }

    pub fn set_video(&mut self, path: &Path) {
        self.generation += 1;
        self.images.clear();
        self.requested.clear();
        self.queue.jobs.lock().unwrap().clear();
        let meta = std::fs::metadata(path).ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let key = Sha1::digest(format!("{}|{size}|{modified}", path.display()).as_bytes());
        let hex: String = key.iter().take(8).map(|b| format!("{b:02x}")).collect();
        let dir = cache_root().join(hex);
        let _ = std::fs::create_dir_all(&dir);
        // Mark as recently used for the LRU prune.
        let _ = std::fs::File::open(&dir).and_then(|f| f.set_modified(SystemTime::now()));
        self.video = Some(path.to_owned());
        self.dir = Some(dir);
    }

    /// Moves finished grabs onto the GPU. Call once per frame.
    pub fn pump(&mut self) {
        while let Ok((generation, ms, img)) = self.results.try_recv() {
            if generation != self.generation {
                continue;
            }
            let image = Arc::new(img);
            let color = self.ctx.load_texture(format!("thumb-{ms}"), image.clone(), TextureOptions::LINEAR);
            self.images.insert(ms, Thumb { color, gray: None, image });
        }
    }

    /// The frame at `t` seconds, or None while it is being made.
    pub fn at(&mut self, t: f64) -> Option<&TextureHandle> {
        let ms = (t * 1000.0).round() as i64;
        if self.images.contains_key(&ms) {
            return self.images.get(&ms).map(|t| &t.color);
        }
        self.request(ms);
        None
    }

    /// Desaturated version, for notes marked done.
    pub fn gray_at(&mut self, t: f64) -> Option<&TextureHandle> {
        let ms = (t * 1000.0).round() as i64;
        if !self.images.contains_key(&ms) {
            self.request(ms);
            return None;
        }
        let th = self.images.get_mut(&ms)?;
        if th.gray.is_none() {
            let mut g = (*th.image).clone();
            for px in g.pixels.iter_mut() {
                let l = ((px.r() as u32 + px.g() as u32 + px.b() as u32) / 3) as u8;
                *px = egui::Color32::from_rgb(l, l, l);
            }
            th.gray = Some(self.ctx.load_texture(format!("thumb-gray-{ms}"), g, TextureOptions::LINEAR));
        }
        th.gray.as_ref()
    }

    fn request(&mut self, ms: i64) {
        let (Some(video), Some(dir)) = (&self.video, &self.dir) else { return };
        if !self.requested.insert(ms) {
            return;
        }
        // Most recent requests first: what is on screen now.
        let job = Job { generation: self.generation, ms, video: video.clone(), dir: dir.clone() };
        self.queue.jobs.lock().unwrap().push_front(job);
        self.queue.ready.notify_one();
    }
}

fn make(ffmpeg: &Path, job: &Job) -> Option<ColorImage> {
    let file = job.dir.join(format!("{}.jpg", job.ms));
    if !file.exists() {
        // Write beside it and rename, so an interrupted grab never leaves a truncated jpg.
        let tmp = job.dir.join(format!("{}.jpg.tmp", job.ms));
        let status = Command::new(ffmpeg)
            .args(["-v", "error", "-ss", &format!("{:.3}", job.ms as f64 / 1000.0), "-i"])
            .arg(&job.video)
            .args(["-frames:v", "1", "-vf", &format!("scale={WIDTH}:-2"), "-q:v", "4", "-f", "mjpeg", "-y"])
            .arg(&tmp)
            .status()
            .ok()?;
        if !status.success() || std::fs::rename(&tmp, &file).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return None;
        }
    }
    let Ok(img) = image::open(&file) else {
        // Broken cache entry: drop it so the next request makes it again.
        let _ = std::fs::remove_file(&file);
        return None;
    };
    let img = img.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

/// Deletes least recently used video directories under `root` until the rest fit in `limit` bytes.
fn prune(root: &Path, limit: u64) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    let dirs: Vec<(PathBuf, u64, SystemTime)> = entries
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok().filter(|m| m.is_dir())?;
            let size = std::fs::read_dir(e.path())
                .ok()?
                .flatten()
                .filter_map(|f| f.metadata().ok())
                .map(|m| m.len())
                .sum();
            Some((e.path(), size, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)))
        })
        .collect();
    for dir in to_prune(dirs, limit) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// From `(dir, size, last used)` entries, the oldest ones to drop so the total is at most `limit`.
fn to_prune(mut dirs: Vec<(PathBuf, u64, SystemTime)>, limit: u64) -> Vec<PathBuf> {
    let mut total: u64 = dirs.iter().map(|d| d.1).sum();
    dirs.sort_by_key(|d| d.2);
    let mut out = Vec::new();
    for (path, size, _) in dirs {
        if total <= limit {
            break;
        }
        total -= size;
        out.push(path);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn entry(name: &str, size: u64, secs: u64) -> (PathBuf, u64, SystemTime) {
        (PathBuf::from(name), size, SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
    }

    #[test]
    fn under_limit_keeps_all() {
        assert!(to_prune(vec![entry("a", 10, 1), entry("b", 20, 2)], 30).is_empty());
        assert!(to_prune(Vec::new(), 0).is_empty());
    }

    #[test]
    fn drops_oldest_first_until_under_limit() {
        let dirs = vec![entry("new", 50, 30), entry("old", 40, 10), entry("mid", 30, 20)];
        assert_eq!(to_prune(dirs.clone(), 100), vec![PathBuf::from("old")]);
        assert_eq!(to_prune(dirs.clone(), 50), vec![PathBuf::from("old"), PathBuf::from("mid")]);
        assert_eq!(to_prune(dirs, 0).len(), 3);
    }
}
