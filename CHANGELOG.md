# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Entries are seeded from the
conventional-commit history; PR numbers are in parentheses. See [README.md](README.md#releases) for
how to cut a release.

## [Unreleased]

### Added

- Notes about the whole video, not tied to a time (#60).
- Open videos from Finder "Open With" and Dock drops (#59).
- macOS disk image installer, `tools/make-dmg.sh` (#2).
- Multilingual UI: 12 languages, switchable live from the header (#1).
- Tag-triggered GitHub release workflow that builds the DMG, and this changelog.

### Changed

- The `.app` version is taken from `Cargo.toml` (#56).
- Licensed under GPL-3.0-or-later (#30).
- Faster: bound the ruler tick count for implausible durations (#88).
- Faster: persist the volume once per scroll gesture (#52).
- Faster: draw note rows by index instead of finding and cloning each note (#45).

### Fixed

- Export: snap the FCPXML `frameDuration` to a standard frame rate (#92).
- Export: cap EDL exports at 999 events and warn (#41).
- Notes: keep an unparsable pending file instead of deleting it (#90).
- Notes: keep unknown JSON keys when saving the notes file (#85).
- Notes: keep the notes file's fps until the real frame rate is known (#54).
- Notes: tell the user when an edit targets a note deleted elsewhere (#53).
- Notes: keep notes in time order when saving fails (#48).
- Notes: keep unsaved note changes across video switches and quit (#42).
- Notes: detect outside edits to the notes file by content, not mtime (#33).
- Notes: write the scene cache and prefs atomically (#47).
- Notes: stamp the scene cache with the video version detection read (#38).
- Playback: free the mpv render context while the GL context is current (#91).
- Playback: size the stage from the displayed picture size (#51).
- Thumbnails: time out hung ffmpeg thumbnail grabs (#89).
- Thumbnails: exclude the open video's folder from the prune total (#87).
- Thumbnails: use a unique temp file per thumbnail job (#84).
- Thumbnails: detach thumbnail ffmpeg from the terminal's stdin (#46).
- Thumbnails: keep the startup prune off the open video's cache (#40).
- Shortcuts: ignore key auto-repeat for toggle shortcuts (#86).
- Shortcuts: keep global shortcuts working when a non-text widget has focus (#39).
- Editor: make Escape cancel note edits without clearing the selection (#49).
- Editor: let Shift+Enter insert a newline in the note editor (#35).
- Stage: turn zero-width or zero-height area drags into pins (#37).
- CLI: accept file paths that start with a dash (#83).
- CLI: store absolute paths so the last session reopens from any directory (#31).
- i18n: fill placeholders in a single pass (#34).
- Installer: save the Finder layout in the dmg (#36).
- Build: rerun the build script when `MPV_LIB_DIR` changes (#32).

[Unreleased]: https://github.com/jablonkai/video-review/commits/main
