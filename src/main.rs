//! Video Review: timecoded, scene and on-frame notes on finished cuts.
//! Rust port of the Flutter app; same `<video>.comments.json` files.

mod export;
mod i18n;
mod model;
mod mpv;
mod open_with;
mod review;
mod theme;
mod thumbs;
mod ui;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let paths: Vec<PathBuf> = path_args(std::env::args().skip(1)).map(absolutize).collect();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("Video Review")
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([1100.0, 700.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    open_with::install();
    eframe::run_native(
        "Video Review",
        options,
        Box::new(move |cc| {
            // A Finder launch has no argv paths; its files arrived before this runs.
            let paths = if paths.is_empty() { open_with::take() } else { paths };
            open_with::set_repaint(&cc.egui_ctx);
            Ok(Box::new(ui::app::App::new(cc, paths).map_err(|e| e.to_string())?))
        }),
    )
}

/// The video paths among the CLI args. The app has no options, so only the
/// `-psn_*` process serial number macOS may pass is dropped; everything after
/// `--` is a path even if it is `-psn_…`.
fn path_args(args: impl IntoIterator<Item = String>) -> impl Iterator<Item = String> {
    let mut after_dashes = false;
    args.into_iter().filter(move |a| {
        if after_dashes {
            true
        } else if a == "--" {
            after_dashes = true;
            false
        } else {
            !a.starts_with("-psn_")
        }
    })
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
    fn cli_args_keep_dash_paths() {
        let args = |v: &[&str]| path_args(v.iter().map(|s| s.to_string())).collect::<Vec<_>>();
        assert_eq!(args(&["-psn_0_12345", "-take2.mov", "a.mp4"]), ["-take2.mov", "a.mp4"]);
        assert_eq!(args(&["a.mp4", "--", "--", "-psn_x"]), ["a.mp4", "--", "-psn_x"]);
        assert!(args(&[]).is_empty());
    }

    #[test]
    fn cli_paths_become_absolute() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(absolutize("clip.mov".into()), cwd.join("clip.mov"));
        assert!(absolutize(".".into()).is_absolute());
        assert_eq!(absolutize("/v/a.mp4".into()), PathBuf::from("/v/a.mp4"));
    }
}
