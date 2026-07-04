# The TomteMacro macro language

TomteMacro macros are plain-text scripts saved as `.tomte` files in the macro
library folder (open it via **Macros → 📂**). Recording produces the same
text you edit, so you can record a rough pass, then tidy and tweak it by hand
— in the built-in editor or any text editor.

```
# tomte-macro v1
# name: farm loop

move 812 344
click left
wait 250ms..400ms      # a little natural variation
repeat 10
  press e
  wait 1.5s
end
type "done!"
```

## Rules of the road

- One command per line. Keywords are case-insensitive; blank lines are fine.
- `#` starts a comment — whole-line or after a command. Comments survive
  recording, the tidy tool, and renames.
- Nothing waits implicitly: commands run back-to-back until a `wait`.
  Playback speed (the × slider) divides every wait; a script's rhythm never
  drifts, because delays are scheduled against absolute deadlines.
- A parse error shows up under the toolbar as `line N: message`, and Play is
  disabled until the script parses.

## Commands

### Mouse

| Command | Meaning |
|---|---|
| `move X Y` | Move to absolute screen pixels. Negative values are legal on multi-monitor setups. |
| `move +DX -DY` | **Relative** move: applied to wherever the cursor is at that moment during playback. A move is relative when at least one argument has an explicit leading `+`. |
| `moverel DX DY` | Relative move for the one case `move` can't express: both deltas negative. |
| `click B` | Press and release button `B` — `left`, `right`, or `middle`. |
| `click B at X Y` | Move there, then click. |
| `doubleclick B [at X Y]` | Two clicks, 30 ms apart. |
| `mousedown B` / `mouseup B` | Hold and release a button — build drags with a `move` in between. |
| `scroll up\|down\|left\|right [N]` | Scroll `N` notches (default 1). |

### Keyboard

| Command | Meaning |
|---|---|
| `press KEY` | Tap a key (press + release). |
| `keydown KEY` / `keyup KEY` | Hold and release a key — chords like ctrl-c are `keydown ctrl` · `press c` · `keyup ctrl`. |
| `type "text"` | Type a string. Printable ASCII only, checked when the script is parsed. `\"` and `\\` are the only escapes. |

Key names are lowercase, no spaces:

- letters `a`–`z`, digits `0`–`9`, function keys `f1`–`f12`
- `enter` `space` `tab` `esc` `backspace` `delete` `insert`
- `home` `end` `pageup` `pagedown`, arrows `up` `down` `left` `right`
  (no clash with mouse buttons — key commands never take a button and vice
  versa)
- modifiers `ctrl` `rctrl` `shift` `rshift` `alt` `altgr` `meta` `rmeta`
- punctuation `minus` `equal` `lbracket` `rbracket` `semicolon` `quote`
  `backquote` `backslash` `intlbackslash` `comma` `dot` `slash`
- keypad `kp0`–`kp9` `kpenter` `kpplus` `kpminus` `kpmultiply` `kpdivide`
  `kpdelete`, plus `printscreen` `scrolllock` `pause` `numlock` `capslock`
- a few aliases parse too: `escape`, `return`, `del`, `ins`, `win`
- `unknown-N` round-trips keys the recorder couldn't name

Keys are **physical positions** (QWERTY reference), not characters: `press a`
taps the key at the QWERTY "A" position whatever your layout. `type` is the
character-oriented tool.

### Timing

| Command | Meaning |
|---|---|
| `wait 500` | Pause 500 ms (bare numbers are milliseconds). |
| `wait 250ms` / `wait 2s` / `wait 8.5ms` | Units and decimals work; max 24 h. |
| `wait 100ms..300ms` | Uniformly random pause, re-rolled **every** time the line runs — inside a `repeat`, each pass gets a fresh roll. |

### Structure

```
repeat 25          # whole numbers ≥ 1; blocks nest (up to 32 deep)
  click left
  wait 100ms..200ms
end
```

For "repeat forever", use the **loop forever** playback option instead — it
also survives the stop hotkey more predictably than a huge count.

## The metadata header

Recordings start with a few `# key: value` comment lines that TomteMacro
reads back as metadata. All are optional, and since they're comments,
deleting them is harmless:

```
# tomte-macro v1                  ← format version; newer files are refused
# name: farm loop                 ← display name (else the file name is used)
# created: 2026-07-04T09:52:18Z
# os: linux-x11
# screen: 2560x1440               ← warns when replayed on a different screen
# notes: anything you like
```

The header ends at the first line that isn't a recognized directive —
ordinary comments below it are just comments.

## Recording, tidying, converting

- **Record** appends to the open macro under a `# recorded <timestamp>`
  marker (or starts a fresh buffer). Adjacent press/release pairs are
  collapsed into `click`/`press` lines and delays are rounded to whole
  milliseconds for readability.
- **🧹 Tidy** removes every mouse move except the ones directly before a
  `click`/`mousedown` and merges the surrounding waits, so the total
  duration is preserved. Recordings shrink by ~90 % and become actually
  readable. (Tidy reformats the script canonically.)
- **Legacy `.ron` macros** (from older TomteMacro versions) still load and
  play forever. Opening one shows the converted script; saving writes a
  `.tomte` file and retires the `.ron`.

## Worked example: a drag with humanized pacing

```
# tomte-macro v1
# name: drag item to slot

move 640 400
mousedown left
repeat 8
  move +20 +5          # ease toward the slot in small steps
  wait 15ms..35ms
end
mouseup left
wait 300ms
doubleclick left at 980 442
```
