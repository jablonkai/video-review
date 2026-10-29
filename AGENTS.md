# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

Rust/egui desktop app for timecoded, scene-range and on-frame review notes on finished video cuts. Successor of a removed Flutter app; also coexists with a Python/web version that reads/writes the same files.

## Commands

```sh
brew install mpv ffmpeg                         # libmpv needed to link/run; ffmpeg at runtime
cargo run --release -- <folder-or-videos...>    # no args = reopen last session
cargo test                                      # unit tests live in-file (#[cfg(test)])
cargo test <name>                               # single test, e.g. cargo test exact_frame_durations
cargo fmt --check                               # CI gate (rustfmt.toml; toolchain pinned in rust-toolchain.toml)
cargo clippy --all-targets -- -D warnings       # CI gate (macOS, together with fmt + cargo test)
cargo deny check advisories                     # CI gate (RustSec advisories; `cargo install cargo-deny`)
./tools/bundle-macos.sh                         # -> target/release/Video Review.app
./tools/make-dmg.sh                             # -> target/release/Video Review-<version>.dmg (drag-to-install .dmg)
```

`build.rs` adds `/opt/homebrew/lib` or `/usr/local/lib` to the link path if `libmpv.dylib` is there; set `MPV_LIB_DIR` otherwise. Edition 2024. CI installs mpv via Homebrew, which pulls in ffmpeg, so the ffmpeg scene-detection test runs there too (it silently no-ops only where ffmpeg is missing).

## Architecture

- **`main.rs`** → eframe with the **Glow renderer** (required: mpv renders via OpenGL into a texture egui displays). CLI args not starting with `-` are paths.
- **`review.rs` — `Review`** owns the review state: mpv handle, loaded video list, comments, scene cuts, selection/area/pending in-point, 20-deep undo of note changes (add, text edit, done/undone, delete; cleared on video switch), prefs (volume/mute/last session in `dirs::config_dir()/video-review/prefs.json`). Issues the mpv commands (`seek … absolute+exact`, `frame-step`); `mpv.rs` only sets `hr-seek=yes` and exposes a generic `command()`. `Review::update()` runs once per frame: drains mpv property events (time-pos, fps, size…), pumps thumbnails, polls scene detection, and every 4 s reloads the notes file if it changed externally or was unreadable. `position()` extrapolates between `time-pos` events while playing (`pos + elapsed * rate`); pausing resets the rate to 1.0. Transient widget state (draft text, inline edit, armed two-step delete, drags, help flag) lives in `ui::App`, not in `Review`.
- **`open_with.rs`** — Finder "Open With" / Dock drops arrive as an `odoc` Apple Event, not argv. An `NSAppleEventManager` handler (objc2) installed at `applicationWillFinishLaunching` queues the paths; `main` uses them when argv has none, `App::ui` drains later ones into `open_paths`. `tools/bundle-macos.sh` declares the `CFBundleDocumentTypes` (`public.movie` + `model::VIDEO_EXTENSIONS` + folders).
- **`mpv.rs`** — hand-written libmpv FFI (client + OpenGL render API). Wakeup callbacks call `ctx.request_repaint()`.
- **`model.rs`** — data + persistence, JSON-compatible with the other versions (don't change the on-disk format):
  - `<video>.comments.json` via `CommentStore`. Edits are `Change` values (Put/Text/Done/Remove, keyed by id). `commit()` re-reads the file and re-applies the changes if it changed on disk, so concurrent edits from other app versions aren't lost. The write (`write()`) normalizes (sorted by `t_in`, `Area` rounded to 4 decimals, `scenes` as `[first, last]`, `frame_in`/`frame_out` recomputed) and writes atomically via a `.tmp` sidecar; the file is compared once more right before the rename, and a write that landed meanwhile is merged the same way (`SAVE_TRIES` attempts, then an error and the changes stay pending). An unparsable file makes the store read-only (never overwritten) and a `.bak` is made before the next write; un-persisted changes wait in `Review::pending`, mirrored by `CommentStore::flush()` to `<video>.comments.pending.json` (this app only) so a video switch or quit doesn't lose them; `load_video` reads it back and the poll retries. If even that file can't be written (`CommentStore::pending_on_disk`), a switch keeps them in `Review::left_behind` until the video is reopened, and the first window close is cancelled with a warning (`Review::save_all`).
  - Whole-video notes (`SpanMode::Video`): `"general": true` (written only when true), `t_in` 0, no span/area/scenes. Listed and exported first, never active, not on the ruler, skipped by note navigation; older versions see them as notes on frame 0.
  - `<video>.scenes.json`: ffmpeg scene-score (threshold 0.25, same as other versions) cut detection, cached and invalidated by threshold + video size/mtime. Runs on a thread; only one `Detection` exists at a time and dropping it kills ffmpeg. Notes whose scene numbers exceed the current cut count keep their `t_in`/`t_out` and drop the `scenes` field.
  - Time: `frame_of(t, fps)` floors with a 1e-3 epsilon to absorb PTS float drift; active-note and scene lookups compare frame indices, range ends are frame-exclusive. Timecodes are non-drop-frame at the rounded nominal rate (29.97 → 30).
  - `find_ffmpeg()` — GUI apps may lack PATH. Missing ffmpeg: thumbnails spawn no workers, detection reports `ScenesStatus::NoFfmpeg`, UI shows the warning (`ffmpeg_missing` only drives that warning and hides the re-detect button).
- **`thumbs.rs`** — ffmpeg frame grabs (240 px wide), 3 workers, newest request first, disk cache in `dirs::cache_dir()/video-review/thumbs` keyed by `SHA1(path|size|mtime)`, pruned oldest-first to 2 GB **once per process** at startup. UI widgets call `thumbs.at()`/`gray_at()` directly to enqueue grabs; a `generation` counter drops stale results after switching videos.
- **`export.rs`** — notes → EDL (DaVinci Resolve markers), FCPXML, Markdown; format chosen by the save-path extension. Frame-exact timecode handling for fractional rates.
- **`i18n.rs`** — UI strings. One flat JSON catalog per language in `assets/i18n/<code>.json` (embedded with `include_str!`), `key -> text` or `key -> {one/few/many/other}` for counts; placeholders are `{name}`. `tr()`, `trf(key, args)`, `trn(key, n, args)` read the current language (`i18n::set/current`, CLDR plural rules in `Lang::plural`); English is the fallback for missing keys. 12 languages (en, hu, de, fr, es, it, pt, nl, pl, ru, ja, zh); the language comes from `prefs.json` (`lang`) or the system locale, and the globe button in the header / empty screen switches it live (`Review::set_language`). Undo notices store the catalog key. A test checks every catalog has exactly the English key set, placeholders and plural forms — add a key to all 12 files.
- **`theme.rs`** — palette, fonts (from `assets/fonts`, embedded), Material Icons glyphs. `install_fonts(ctx, lang)` appends a system CJK font (Hiragino / Noto Sans CJK / Yu Gothic…) for ja/zh, since Archivo and Source Serif lack those glyphs.
- **`ui/`** — `app.rs` (eframe `App`: layout, header/transport, global keyboard shortcuts + `?` help table `GROUPS`, file dialogs, drag-and-drop), `timeline.rs` (filmstrip + ruler, scene/range selection), `stage.rs` (video picture), `panel.rs` (notes list/editor), `pencil.rs` (grease-pencil area/pin marks; `Area` is normalized 0..1, `w == h == 0` is a pin; stroke wobble is seeded from the comment id so it never jitters).
  - Cross-widget flow: a finished ruler range drag or a stage pin/area placement returns `drafted = true`, which pauses playback and focuses the note composer. `Review::hover_id` is shared by panel, ruler and stage for highlighting; `panel()` runs before the central panel each frame, so it resets `hover_id` and the ruler may override it.

## Conventions

- Code and comments are English. User-facing strings go through `i18n` (never literal in the UI); the English catalog is the source text, the other catalogs are translations of it. Durations are shown as `"4.0 s"`, decimals with a dot.
- `tools/make_fonts.py` generates the static Archivo/Source Serif instances (tabular figures frozen in) in `assets/fonts`; regenerate rather than hand-editing fonts.
