# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

TomteMacro: a cross-platform auto-clicker and mouse/keyboard macro recorder in Rust. Linux/X11 is the primary target; Windows and macOS are supported; Wayland is detected and refused with a warning. Input is synthesized through ordinary OS APIs (XTest, SendInput, CGEvent) — anti-cheat/detection evasion is explicitly out of scope for this project.

## Commands

```bash
# Linux build deps (Debian/Ubuntu) — needed before anything compiles
sudo apt-get install libx11-dev libxtst-dev libxi-dev

cargo build --workspace
cargo run -p tomtemacro-gui          # the GUI binary is named `tomte`
cargo test --workspace               # headless-safe; X-server tests are #[ignore]d
cargo fmt --all --check              # CI enforces
cargo clippy --workspace --all-targets -- -D warnings   # CI enforces; workspace lints set clippy::all=warn

# Single test
cargo test -p tomtemacro-core --test script every_command_parses

# X11 runtime smoke tests (inject → capture → replay) under a virtual display, as CI runs them:
TOMTE_FULL_SMOKE=1 xvfb-run -a cargo test --workspace -- --ignored
# Without TOMTE_FULL_SMOKE the full smoke self-skips; the cursor round-trip test is safe
# on a live desktop (moves the pointer and puts it back, no clicks).

# Headless engine examples (no GUI): clicker, record, play, hotkeys
cargo run -p tomtemacro-core --example record
```

## Releasing

Releases are built by `.github/workflows/release.yml`, triggered **only by pushing a `v*` tag** — merging to main does nothing release-wise. To ship:

```bash
# 1. Bump [workspace.package] version in the root Cargo.toml, refresh the lockfile
cargo check --workspace
# 2. Commit, push, tag the release commit, push the tag
git push origin main
git tag v0.X.Y && git push origin v0.X.Y
```

The workflow builds Linux (ubuntu-22.04 for a glibc 2.35 baseline), Windows, and a macOS universal binary, then publishes the GitHub release atomically once all three assets exist. Asset names are deliberately **versionless** (`tomte-linux-x86_64.tar.gz`, …) because external sites link to `releases/latest/download/<asset>` — never rename them.

## Architecture

Two-crate workspace:

- **`crates/tomtemacro-core`** — the engine: event model, capture, injection, recording, playback, clicker, script language, storage. Deliberately GUI-free and headless-testable.
- **`crates/tomtemacro-gui`** — egui/eframe app (binary `tomte`): tabs, settings persistence (RON in the platform config dir), OS-registered global hotkeys, platform warning banners.

### Core design decisions (the things that explain "why is it built this way")

- **Own event model** (`model.rs`): `MacroFile`/`EventKind`/`Key` are project types, never re-exports from rdev/enigo — the on-disk format must survive swapping either dependency. `SCHEMA_VERSION` gates loading; newer files are rejected.
- **Traits as isolation seams**: `InputCapture` (capture.rs, rdev-backed) and `Injector` (inject.rs, enigo-backed) exist so backends can be swapped — rdev is stale and a future evdev/uinput Wayland backend is planned — and so tests can use mock/null injectors.
- **Engine threading contract** (`engine.rs`): one long-lived controller thread owns the injector and runs one activity at a time (`Mode`: Idle/Recording/Playing/Clicking). Commands arrive on a channel; live telemetry lives in `SharedState` atomics so the GUI reads it every frame without message churn; **stop is an atomic flag polled every few ms** so a hotkey stop lands mid-click-loop. Start commands arriving while busy are dropped, not queued.
- **Self-trigger defense**: injected events must never enter a recording or fire our own hotkeys. Three layers: capture gates on `mode() == Recording` (injection only happens in Playing/Clicking), OS-level hotkey registration consumes the chords, and the recorder strips configured hotkey chords plus trims the stop chord's tail as belt-and-braces.
- **Timing** (`timing.rs`): everything schedules against *absolute* deadlines (`start + n*interval`), never per-delta sleeps, so drift self-corrects. Waits are coarse native sleep (polling stop every ~5 ms) with a ~3 ms spin-sleep tail for ms precision on Windows' 15.6 ms timer.
- **Script language is the source of truth** (`script/`): macros are `.tomte` plain-text files (`docs/macro-language.md` is the reference). Playback walks the parsed AST — `repeat` blocks, ranged waits (`wait 100ms..300ms`), relative moves are resolved at *play time*, not pre-expanded into events. `script::from_events` converts flat recordings into scripts; `storage.rs` loads legacy `.ron` recordings forever and converts them to `.tomte` on save. Comments and blank lines round-trip through parse/format/transform.
- **Key semantics per platform** (`convert.rs`): on X11 keys are injected as raw keycodes (physical-key semantics — recording `å` on a Swedish layout replays as `å`); on Windows/macOS printables currently use character semantics via enigo `Key::Unicode`.

### GUI structure

`app.rs` spawns the engine and capture threads at startup, then every frame drains the hotkey/status channels and reads the shared atomics (100 ms repaint baseline keeps counters live). Tabs live in `tabs/` (clicker, macros with the script editor + cheat sheet, settings). Hotkeys (`hotkeys.rs`) are F1–F12 only, rebindable live from Settings.

## Testing notes

- `cargo test` must stay headless-safe: anything needing a display is `#[ignore]`d (`x11_smoke.rs`, `hotkey_smoke.rs`).
- X11 smoke tests serialize on a shared `Mutex` because they all drive the display's one cursor/keyboard; a poisoned lock is deliberately ignored via `into_inner`.
- Contract tests: `tests/script.rs` (parse/format/convert/tidy), `tests/roundtrip.rs` (on-disk format losslessness).
