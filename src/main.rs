//! Video Review: timecoded, scene and on-frame notes on finished cuts.
//! Rust port of the Flutter app; same `<video>.comments.json` files.

mod export;
mod i18n;
mod model;
mod mpv;
mod review;
mod theme;
mod thumbs;
mod ui;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let paths: Vec<PathBuf> = std::env::args().skip(1).filter(|a| !a.starts_with('-')).map(absolutize).collect();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("Video Review")
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([1100.0, 700.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "Video Review",
        options,
        Box::new(move |cc| Ok(Box::new(ui::app::App::new(cc, paths).map_err(|e| e.to_string())?))),
    )
}

/// Paths are saved as the last session, so make them independent of the
/// working directory (a Finder launch runs in `/`).
fn absolutize(arg: String) -> PathBuf {
    let path = PathBuf::from(arg);
    std::path::absolute(&path).unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_paths_become_absolute() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(absolutize("clip.mov".into()), cwd.join("clip.mov"));
        assert!(absolutize(".".into()).is_absolute());
        assert_eq!(absolutize("/v/a.mp4".into()), PathBuf::from("/v/a.mp4"));
    }
}
