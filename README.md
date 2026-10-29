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
./tools/make-dmg.sh   # -> target/release/Video Review-<version>.dmg
```

The script builds the `.app` (via `tools/bundle-macos.sh`) and wraps it in a
drag-to-install disk image containing `Video Review.app`, an `Applications`
alias, and a ReadMe (hu/en). `<version>` is the `version` in `Cargo.toml`.

The app is **not code-signed** (no Apple Developer ID), so on first launch the
user must right-click the app and choose **Open**. libmpv is not bundled: the
`.app` is arm64-only (Apple Silicon) and links Homebrew's libmpv at
`/opt/homebrew/opt/mpv/lib/libmpv.2.dylib`, so it needs `brew install mpv` in
the default `/opt/homebrew` prefix. Without it (or on an Intel Mac) the app does
not start at all. ffmpeg (`brew install ffmpeg`) is needed for thumbnails and
scene detection; if it is missing, the app still runs and shows a warning.

With no arguments it reopens the last folder/video (`~/Library/Application Support/video-review/prefs.json`).
Press `?` in the app for the shortcuts.

## Languages

The UI comes in 12 languages: English, Magyar, Deutsch, Français, Español, Italiano, Português,
Nederlands, Polski, Русский, 日本語 and 中文. The language is the `lang` code saved in `prefs.json`,
or the system locale if none is saved (English if that is not supported). The globe button in the
header (and on the empty screen) switches it at once and saves the choice. Japanese and Chinese use
a system CJK font, since the bundled fonts lack those glyphs.

Every string key must be in all 12 `assets/i18n/<code>.json` catalogs (`en.json` is the source
text); a test checks that each catalog has exactly the English keys, placeholders and plural forms.

## Layout

| File | What |
|---|---|
| `src/main.rs` | entry point: eframe with the Glow renderer, CLI paths |
| `src/open_with.rs` | Finder "Open With" and Dock drops (`odoc` Apple Event) |
| `src/mpv.rs` | libmpv client + OpenGL render API binding |
| `src/model.rs` | comments/scenes JSON, timecode, ffmpeg scene detection (unit tests) |
| `src/thumbs.rs` | ffmpeg frame grabs, 3 workers, newest request first |
| `src/review.rs` | all review state and playback control |
| `src/export.rs` | EDL / FCPXML / Markdown export with frame-exact timecodes |
| `src/i18n.rs`, `assets/i18n/*.json` | UI strings: one JSON catalog per language, plural rules, English fallback |
| `src/theme.rs` | palette, fonts, icons |
| `src/ui/` | app layout, filmstrip + ruler, picture stage, notes panel, grease-pencil marks |
| `tools/make_fonts.py` | static Archivo/Source Serif instances with tabular figures frozen in |

Fonts: Archivo and Source Serif 4 (OFL), Material Icons (Apache 2.0); licenses in `assets/fonts`.

## Tests

```sh
cargo test                                  # CI gate; unit tests live in-file (#[cfg(test)])
cargo fmt --check                           # CI gate
cargo clippy --all-targets -- -D warnings   # CI gate
cargo deny check advisories                 # CI gate (RustSec advisories; `cargo install cargo-deny`)
```

The UI was checked end to end with real mouse/keyboard events
(frame step, scene click, area drag + save, ruler range, help, edit, done, delete,
next note, video switch).

## License

Copyright (C) 2026 Tamás Jablonkai

Licensed under the GNU General Public License v3.0 or later — see [LICENSE](LICENSE).
Bundled fonts keep their own licenses: Archivo and Source Serif 4 under the SIL Open Font
License, Material Icons under Apache 2.0 (texts in `assets/fonts`).
