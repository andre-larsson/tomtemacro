//! Text → [`Script`]. Line-oriented; every error carries a 1-based line.

use crate::model::{MacroMeta, ScreenInfo};

use super::names::{char_to_key, parse_button, parse_key};
use super::{Instr, ParseError, Script, Stmt, MAX_REPEAT_DEPTH, TEXT_VERSION};

/// Waits longer than this are almost certainly typos.
const MAX_WAIT_US: u64 = 24 * 3600 * 1_000_000;

pub fn parse(text: &str) -> Result<Script, ParseError> {
    let lines: Vec<&str> = text.lines().collect();
    let (meta, body_start) = parse_header(&lines)?;

    struct Block {
        line: u32,
        count: u32,
        comment: Option<String>,
        body: Vec<Stmt>,
    }
    let mut root: Vec<Stmt> = Vec::new();
    let mut stack: Vec<Block> = Vec::new();

    for (idx, raw) in lines.iter().enumerate().skip(body_start) {
        let line = (idx + 1) as u32;
        let err = |message: String| ParseError { line, message };
        let (code, comment) = split_comment(raw);
        let code = code.trim();

        let stmt = if code.is_empty() {
            Stmt {
                instr: Instr::Nop,
                comment,
            }
        } else {
            let (verb_raw, rest) = split_first_token(code);
            match verb_raw.to_ascii_lowercase().as_str() {
                "repeat" => {
                    if stack.len() >= MAX_REPEAT_DEPTH {
                        return Err(err(format!(
                            "repeat blocks nested deeper than {MAX_REPEAT_DEPTH}"
                        )));
                    }
                    let count: u32 = rest
                        .parse()
                        .ok()
                        .filter(|n| *n >= 1)
                        .ok_or_else(|| err(format!("usage: repeat N (got '{rest}')")))?;
                    stack.push(Block {
                        line,
                        count,
                        comment,
                        body: Vec::new(),
                    });
                    continue;
                }
                "end" => {
                    if !rest.is_empty() {
                        return Err(err(format!("unexpected text after 'end': '{rest}'")));
                    }
                    let block = stack
                        .pop()
                        .ok_or_else(|| err("'end' without a matching 'repeat'".into()))?;
                    Stmt {
                        instr: Instr::Repeat {
                            count: block.count,
                            body: block.body,
                            end_comment: comment,
                        },
                        comment: block.comment,
                    }
                }
                verb => Stmt {
                    instr: parse_instr(verb, rest, line)?,
                    comment,
                },
            }
        };
        match stack.last_mut() {
            Some(block) => block.body.push(stmt),
            None => root.push(stmt),
        }
    }

    if let Some(block) = stack.last() {
        return Err(ParseError {
            line: block.line,
            message: "'repeat' without a matching 'end'".into(),
        });
    }
    Ok(Script { meta, body: root })
}

/// Leading `# key: value` comment lines become metadata; the first line
/// that isn't a recognized directive starts the body.
fn parse_header(lines: &[&str]) -> Result<(MacroMeta, usize), ParseError> {
    let mut meta = MacroMeta::default();
    for (idx, raw) in lines.iter().enumerate() {
        let line = (idx + 1) as u32;
        let err = |message: String| ParseError { line, message };
        let Some(rest) = raw.trim().strip_prefix('#') else {
            return Ok((meta, idx));
        };
        let rest = rest.trim();
        if let Some(version) = rest.strip_prefix("tomte-macro") {
            let version = version.trim();
            let number: u32 = version
                .strip_prefix('v')
                .and_then(|n| n.parse().ok())
                .ok_or_else(|| err(format!("malformed version marker '{rest}'")))?;
            if number > TEXT_VERSION {
                return Err(err(format!(
                    "this file uses macro format v{number}, but this build only \
                     understands up to v{TEXT_VERSION} — update TomteMacro"
                )));
            }
            continue;
        }
        let Some((key, value)) = rest.split_once(':') else {
            return Ok((meta, idx));
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "name" => meta.name = value.into(),
            "created" => meta.created_utc = value.into(),
            "os" => meta.os = value.into(),
            "notes" => meta.notes = value.into(),
            "screen" => {
                let parsed = value.split_once(['x', 'X']).and_then(|(w, h)| {
                    Some(ScreenInfo {
                        width: w.trim().parse().ok()?,
                        height: h.trim().parse().ok()?,
                        scale: 1.0,
                    })
                });
                meta.screen =
                    Some(parsed.ok_or_else(|| {
                        err(format!("malformed screen size '{value}' (want WxH)"))
                    })?);
            }
            _ => return Ok((meta, idx)),
        }
    }
    Ok((meta, lines.len()))
}

/// What a line means to the metadata header.
enum HeaderLine {
    Marker,
    Directive(&'static str),
    Body,
}

fn classify_header_line(line: &str) -> HeaderLine {
    let Some(rest) = line.trim().strip_prefix('#') else {
        return HeaderLine::Body;
    };
    let rest = rest.trim();
    if rest.starts_with("tomte-macro") {
        return HeaderLine::Marker;
    }
    let Some((key, _)) = rest.split_once(':') else {
        return HeaderLine::Body;
    };
    match key.trim().to_ascii_lowercase().as_str() {
        "name" => HeaderLine::Directive("name"),
        "created" => HeaderLine::Directive("created"),
        "os" => HeaderLine::Directive("os"),
        "screen" => HeaderLine::Directive("screen"),
        "notes" => HeaderLine::Directive("notes"),
        _ => HeaderLine::Body,
    }
}

/// Rewrite (or insert) the `# name:` header directive, leaving every other
/// line untouched — this is how rename works without reformatting the file.
pub fn with_header_name(text: &str, name: &str) -> String {
    let directive = format!("# name: {name}");
    let mut out: Vec<String> = Vec::new();
    let mut done = false;
    let mut in_header = true;
    for line in text.lines() {
        if in_header {
            match classify_header_line(line) {
                HeaderLine::Directive("name") if !done => {
                    out.push(directive.clone());
                    done = true;
                    continue;
                }
                HeaderLine::Marker | HeaderLine::Directive(_) => {}
                HeaderLine::Body => {
                    in_header = false;
                    if !done {
                        out.push(directive.clone());
                        done = true;
                    }
                }
            }
        }
        out.push(line.to_string());
    }
    if !done {
        out.push(directive);
    }
    out.join("\n") + "\n"
}

fn parse_instr(verb: &str, rest: &str, line: u32) -> Result<Instr, ParseError> {
    let err = |message: String| ParseError { line, message };
    let args: Vec<&str> = rest.split_whitespace().collect();
    let key_arg = |usage: &str| -> Result<crate::model::Key, ParseError> {
        let [name] = args[..] else {
            return Err(err(format!("usage: {usage}")));
        };
        parse_key(name).ok_or_else(|| err(format!("unknown key '{name}'")))
    };

    match verb {
        "move" => {
            let [x, y] = args[..] else {
                return Err(err("usage: move X Y (or move +DX -DY)".into()));
            };
            let relative = x.starts_with('+') || y.starts_with('+');
            let x = int_arg(x, line)?;
            let y = int_arg(y, line)?;
            Ok(if relative {
                Instr::MoveRel { dx: x, dy: y }
            } else {
                Instr::Move { x, y }
            })
        }
        "moverel" => {
            let [dx, dy] = args[..] else {
                return Err(err("usage: moverel DX DY".into()));
            };
            Ok(Instr::MoveRel {
                dx: int_arg(dx, line)?,
                dy: int_arg(dy, line)?,
            })
        }
        "click" | "doubleclick" => {
            let usage = format!("usage: {verb} left|right|middle [at X Y]");
            let (name, at) = match args[..] {
                [name] => (name, None),
                [name, at, x, y] if at.eq_ignore_ascii_case("at") => {
                    (name, Some((int_arg(x, line)?, int_arg(y, line)?)))
                }
                _ => return Err(err(usage)),
            };
            let button =
                parse_button(name).ok_or_else(|| err(format!("unknown mouse button '{name}'")))?;
            Ok(Instr::Click {
                button,
                at,
                double: verb == "doubleclick",
            })
        }
        "mousedown" | "mouseup" => {
            let [name] = args[..] else {
                return Err(err(format!("usage: {verb} left|right|middle")));
            };
            let button =
                parse_button(name).ok_or_else(|| err(format!("unknown mouse button '{name}'")))?;
            Ok(match verb {
                "mousedown" => Instr::MouseDown(button),
                _ => Instr::MouseUp(button),
            })
        }
        "scroll" => {
            let (dir, n) = match args[..] {
                [dir] => (dir, 1),
                [dir, n] => (
                    dir,
                    n.parse::<i32>()
                        .ok()
                        .filter(|n| *n >= 1)
                        .ok_or_else(|| err(format!("bad scroll count '{n}'")))?,
                ),
                _ => return Err(err("usage: scroll up|down|left|right [N]".into())),
            };
            match dir.to_ascii_lowercase().as_str() {
                "up" => Ok(Instr::Scroll { dx: 0, dy: n }),
                "down" => Ok(Instr::Scroll { dx: 0, dy: -n }),
                "right" => Ok(Instr::Scroll { dx: n, dy: 0 }),
                "left" => Ok(Instr::Scroll { dx: -n, dy: 0 }),
                other => Err(err(format!("unknown scroll direction '{other}'"))),
            }
        }
        "press" => Ok(Instr::KeyTap(key_arg("press KEY")?)),
        "keydown" => Ok(Instr::KeyDown(key_arg("keydown KEY")?)),
        "keyup" => Ok(Instr::KeyUp(key_arg("keyup KEY")?)),
        "type" => {
            let text = parse_quoted(rest, line)?;
            for c in text.chars() {
                if char_to_key(c).is_none() {
                    return Err(err(format!(
                        "cannot type character {c:?} (printable ASCII only)"
                    )));
                }
            }
            Ok(Instr::Type(text))
        }
        "wait" => {
            let [spec] = args[..] else {
                return Err(err("usage: wait 500ms | 2s | 100ms..300ms".into()));
            };
            let (min_us, max_us) = match spec.split_once("..") {
                Some((min, max)) => (parse_duration_us(min, line)?, parse_duration_us(max, line)?),
                None => {
                    let us = parse_duration_us(spec, line)?;
                    (us, us)
                }
            };
            if min_us > max_us {
                return Err(err(format!("wait range '{spec}' must be min..max")));
            }
            Ok(Instr::Wait { min_us, max_us })
        }
        other => Err(err(format!(
            "unknown command '{other}' — see the cheat sheet"
        ))),
    }
}

fn int_arg(token: &str, line: u32) -> Result<i32, ParseError> {
    token.parse().map_err(|_| ParseError {
        line,
        message: format!("expected a whole number, got '{token}'"),
    })
}

/// `500` (ms), `500ms`, `1.5s`, `0.5ms` → microseconds.
fn parse_duration_us(token: &str, line: u32) -> Result<u64, ParseError> {
    let err = || ParseError {
        line,
        message: format!("bad duration '{token}' (use e.g. 500, 500ms, or 1.5s)"),
    };
    let token = token.to_ascii_lowercase();
    let (number, per_unit) = if let Some(n) = token.strip_suffix("ms") {
        (n, 1_000.0)
    } else if let Some(n) = token.strip_suffix('s') {
        (n, 1_000_000.0)
    } else {
        (token.as_str(), 1_000.0)
    };
    let value: f64 = number.parse().map_err(|_| err())?;
    if !value.is_finite() || value < 0.0 {
        return Err(err());
    }
    let us = (value * per_unit).round() as u64;
    if us > MAX_WAIT_US {
        return Err(ParseError {
            line,
            message: format!("'{token}' is longer than the 24 h wait limit"),
        });
    }
    Ok(us)
}

/// Split a line at the first `#` that is not inside a quoted string.
/// The comment comes back trimmed, without the `#`.
fn split_comment(line: &str) -> (&str, Option<String>) {
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '#' if !in_string => {
                return (&line[..i], Some(line[i + 1..].trim().to_string()));
            }
            _ => {}
        }
    }
    (line, None)
}

fn split_first_token(code: &str) -> (&str, &str) {
    match code.find(char::is_whitespace) {
        Some(i) => (&code[..i], code[i..].trim()),
        None => (code, ""),
    }
}

/// A double-quoted string with `\"` and `\\` escapes; nothing may follow.
fn parse_quoted(rest: &str, line: u32) -> Result<String, ParseError> {
    let err = |message: String| ParseError { line, message };
    let rest = rest.trim();
    let Some(inner) = rest.strip_prefix('"') else {
        return Err(err("usage: type \"some text\"".into()));
    };
    let mut out = String::new();
    let mut chars = inner.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some((_, escapee @ ('"' | '\\'))) => out.push(escapee),
                Some((_, other)) => {
                    return Err(err(format!("unsupported escape '\\{other}'")));
                }
                None => return Err(err("unterminated string".into())),
            },
            '"' => {
                let tail = inner[i + 1..].trim();
                if !tail.is_empty() {
                    return Err(err(format!("unexpected text after string: '{tail}'")));
                }
                return Ok(out);
            }
            _ => out.push(c),
        }
    }
    Err(err("unterminated string".into()))
}
