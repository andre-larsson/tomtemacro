# TomteMacro 🍄

*The little gnome that clicks for you.*

TomteMacro is a cross-platform auto-clicker and mouse/keyboard macro recorder,
written in Rust. Like the tomte of Scandinavian folklore — the household gnome
that quietly does your chores while you sleep — it takes over the repetitive
clicking and typing so you don't have to.

## Features

- **Auto-clicker** — configurable interval (ms precision), mouse button,
  single/double click, fixed position or follow-cursor, optional humanized
  jitter (interval variance + position offset), optional click-count limit.
- **Macro recorder** — record global mouse & keyboard input with original
  timing, replay at any speed, loop N times or forever.
- **Macro library** — macros saved as human-readable, hand-editable
  [RON](https://github.com/ron-rs/ron) files you can rename, tweak, and share.
- **Global hotkeys** — start/stop everything from anywhere
  (defaults: `F6` clicker, `F7` record, `F8` play, `F9` stop all).
- **GUI** — a small [egui](https://github.com/emilk/egui) app; the engine runs
  on background threads so the UI never blocks.

## Platform support

| Platform | Status | Notes |
|---|---|---|
| Linux (X11) | ✅ Primary target | Needs `libx11`, `libxtst`, `libxi` |
| Windows | ✅ Supported | Can't control elevated (admin) windows unless TomteMacro itself runs elevated. Millisecond timing works around the default 15.6 ms Windows timer granularity. |
| macOS | ✅ Supported | Requires the **Accessibility** permission (System Settings → Privacy & Security → Accessibility) for both recording and playback. TomteMacro guides you through this on first launch. |
| Linux (Wayland) | ❌ Not yet | Wayland by design blocks global input capture/injection. TomteMacro detects Wayland sessions and warns. An `evdev`/`uinput` backend is planned. |

## Building from source

```bash
# Linux build dependencies (Debian/Ubuntu)
sudo apt-get install libx11-dev libxtst-dev libxi-dev

cargo build --release
./target/release/tomte
```

## A note on games and fair use

TomteMacro synthesizes input through the ordinary OS APIs (`SendInput`,
CGEvent, XTest). It makes **no attempt to hide itself** — anti-cheat systems
detect synthetic input by design, and using macros may violate a game's terms
of service. Evasion features are explicitly out of scope for this project.
Unsigned auto-clickers are also common antivirus false-positive targets;
building from source is the transparent way around that.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
