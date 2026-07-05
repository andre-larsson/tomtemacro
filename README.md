```text
#####  ###  #   # ##### ##### #   #  ###   #### ####   ###
  #   #   # ## ##   #   #     ## ## #   # #     #   # #   #
  #   #   # # # #   #   ####  # # # ##### #     ####  #   #
  #   #   # #   #   #   #     #   # #   # #     #  #  #   #
  #    ###  #   #   #   ##### #   # #   #  #### #   #  ###

          /\
         /##\
        /####\        record  ==>  replay  ==>  refine
       /######\
      /########\      hotkeys : paths : timing : script
     #==========#
      | o    o |
      |   /\   |
      |  (__)  |
   ___|        |___
  /   \########/   \
 /  ___\######/___  \
'--'   |######|   '--'
       |######|
      _|######|_
     #==========#
```

*The little gnome that clicks for you.*

TomteMacro is a cross-platform auto-clicker and mouse/keyboard macro recorder,
written in Rust. Like the tomte of Scandinavian folklore — the household gnome
that quietly does your chores while you sleep — it takes over the repetitive
clicking and typing so you don't have to.

## Features

- **Auto-clicker / auto-presser** — configurable interval (ms precision),
  mouse button or keyboard key, single/double click, fixed position or
  follow-cursor, optional humanized jitter (interval variance + position
  offset), optional click-count limit.
- **Macro recorder** — record global mouse & keyboard input with original
  timing, replay at any speed, loop N times or forever.
- **Macro language & editor** — macros are plain-text scripts
  (`click left`, `wait 100ms..300ms`, `repeat 10 … end`, `type "hello"`, …)
  you can record, hand-write, and edit in the built-in editor, with regex
  find & replace, an in-app cheat sheet, and a one-click tidy tool that
  strips recorded mouse noise. See
  [docs/macro-language.md](https://github.com/andre-larsson/tomtemacro/blob/main/docs/macro-language.md)
  for the full reference.
- **Macro library** — macros saved as human-readable, hand-editable `.tomte`
  script files you can rename, tweak, and share (old `.ron` recordings keep
  working and convert on save).
- **Live input readout** — the status bar always shows the current mouse
  position and the last pressed button.
- **Anti-sleep** — optional keep-awake that nudges the mouse one pixel (and
  straight back) after a configurable idle period, so the machine never
  sleeps and presence apps stay "active". It only fires when you're actually
  idle — never while you're moving the mouse, recording, or playing.
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

## Download & install

Prebuilt binaries for the latest release (these links always point at the
newest version):

- [Linux x86_64](https://github.com/andre-larsson/tomtemacro/releases/latest/download/tomte-linux-x86_64.tar.gz)
- [Windows x86_64](https://github.com/andre-larsson/tomtemacro/releases/latest/download/tomte-windows-x86_64.zip)
- [macOS universal](https://github.com/andre-larsson/tomtemacro/releases/latest/download/tomte-macos-universal.tar.gz)

The binaries are unsigned, so expect a one-time warning on first launch (see
[fair use](#a-note-on-games-and-fair-use) below). If you'd rather not trust a
prebuilt binary, build from source instead.

### Linux

```bash
tar -xzf tomte-linux-x86_64.tar.gz
./tomte
```

That's it — but if you want TomteMacro in your app launcher, the tarball also
ships a `.desktop` file and icon (no root needed; `~/.local/bin` is on `PATH`
on most distros):

```bash
install -Dm755 tomte ~/.local/bin/tomte
install -Dm644 tomtemacro.desktop ~/.local/share/applications/tomtemacro.desktop
install -Dm644 tomtemacro.png ~/.local/share/icons/hicolor/256x256/apps/tomtemacro.png
```

Remember to log into an **X11 session** — see the platform table above for
the Wayland situation.

### Windows

Unzip and run `tomte.exe`. SmartScreen will object the first time:
**More info → Run anyway**.

### macOS

The tarball contains `TomteMacro.app`. Drag it into `/Applications`, then
**right-click → Open** the first time, or clear the quarantine flag:

```bash
xattr -dr com.apple.quarantine /Applications/TomteMacro.app
```

On first launch TomteMacro walks you through granting the **Accessibility**
permission it needs for recording and playback.

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
