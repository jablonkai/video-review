# Video Review

Rust/egui desktop app for timecoded, scene-range and on-frame review notes on finished video cuts
(successor of the removed Flutter version). Notes are stored next to each video as
`<video>.comments.json`, and scene cuts as `<video>.scenes.json`. The Python/web version reads and
writes the same files, so both can be used on the same folder; external changes are picked up every
4 seconds.

- UI: eframe/egui 0.37 (glow renderer)
- Playback: libmpv through a small FFI (`src/mpv.rs`); mpv renders into an OpenGL
  texture that egui shows, with exact seeks (`hr-seek`) and frame stepping
- Frames for the filmstrip and notes: ffmpeg, cached in `~/Library/Caches/video-review/thumbs`
- Scene cuts: ffmpeg scene score 0.25 (same as the other versions)
- Export: notes as EDL (DaVinci Resolve markers), FCPXML or Markdown, picked by the file extension

## Requirements

```sh
brew install mpv ffmpeg
```

`build.rs` adds `/opt/homebrew/lib` or `/usr/local/lib` to the link path if `libmpv.dylib` is
there; set `MPV_LIB_DIR` otherwise.

## Run

```sh
cargo run --release -- <folder-or-videos...>   # a folder or video files
./tools/bundle-macos.sh                        # target/release/Video Review.app
```

## macOS installer

```sh
./tools/make-dmg.sh   # -> target/release/Video Review-0.1.0.dmg
```

The script builds the `.app` (via `tools/bundle-macos.sh`) and wraps it in a
drag-to-install disk image containing `Video Review.app`, an `Applications`
alias, and a ReadMe (hu/en).

The app is **not code-signed** (no Apple Developer ID), so on first launch the
user must right-click the app and choose **Open**. It needs Homebrew's
`mpv` and `ffmpeg` at runtime (`brew install mpv ffmpeg`); without them
playback, thumbnails and scene detection are unavailable, which the app
reports in its UI.

With no arguments it reopens the last folder/video (`~/Library/Application Support/video-review/prefs.json`).
Press `?` in the app for the shortcuts.

## Layout

| File | What |
|---|---|
| `src/mpv.rs` | libmpv client + OpenGL render API binding |
| `src/model.rs` | comments/scenes JSON, timecode, ffmpeg scene detection (unit tests) |
| `src/thumbs.rs` | ffmpeg frame grabs, 3 workers, newest request first |
| `src/review.rs` | all review state and playback control |
| `src/export.rs` | EDL / FCPXML / Markdown export with frame-exact timecodes |
| `src/theme.rs` | palette, fonts, icons |
| `src/ui/` | app layout, filmstrip + ruler, picture stage, notes panel, grease-pencil marks |
| `tools/make_fonts.py` | static Archivo/Source Serif instances with tabular figures frozen in |

Fonts: Archivo and Source Serif 4 (OFL), Material Icons (Apache 2.0); licenses in `assets/fonts`.

## Tests

```sh
cargo test
cargo clippy --all-targets -- -D warnings   # CI gate
```

The UI was checked end to end with real mouse/keyboard events
(frame step, scene click, area drag + save, ruler range, help, edit, done, delete,
next note, video switch).

## License

Copyright (C) 2026 Tamás Jablonkai

Licensed under the GNU General Public License v3.0 or later — see [LICENSE](LICENSE).
Bundled fonts keep their own licenses: Archivo and Source Serif 4 under the SIL Open Font
License, Material Icons under Apache 2.0 (texts in `assets/fonts`).
