//! [`Script`] → text. Canonical output: two-space indents inside `repeat`,
//! durations in the shortest unit, comments preserved.

use super::names::{button_name, key_name};
use super::{Instr, Script, Stmt, TEXT_VERSION};

pub fn format(script: &Script) -> String {
    let mut out = String::new();
    let meta = &script.meta;
    out.push_str(&format!("# tomte-macro v{TEXT_VERSION}\n"));
    for (key, value) in [
        ("name", &meta.name),
        ("created", &meta.created_utc),
        ("os", &meta.os),
        ("notes", &meta.notes),
    ] {
        if !value.is_empty() {
            out.push_str(&format!("# {key}: {value}\n"));
        }
    }
    if let Some(screen) = meta.screen {
        out.push_str(&format!("# screen: {}x{}\n", screen.width, screen.height));
    }
    write_block(&mut out, &script.body, 0);
    out
}

pub fn format_body(body: &[Stmt]) -> String {
    let mut out = String::new();
    write_block(&mut out, body, 0);
    out
}

fn write_block(out: &mut String, body: &[Stmt], depth: usize) {
    for stmt in body {
        match &stmt.instr {
            Instr::Nop => match &stmt.comment {
                Some(comment) => push_line(out, depth, &comment_text(comment), &None),
                None => out.push('\n'),
            },
            Instr::Repeat {
                count,
                body: inner,
                end_comment,
            } => {
                push_line(out, depth, &format!("repeat {count}"), &stmt.comment);
                write_block(out, inner, depth + 1);
                push_line(out, depth, "end", end_comment);
            }
            instr => push_line(out, depth, &instr_text(instr), &stmt.comment),
        }
    }
}

fn push_line(out: &mut String, depth: usize, text: &str, comment: &Option<String>) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.push_str(text);
    if let Some(comment) = comment {
        out.push_str("  ");
        out.push_str(&comment_text(comment));
    }
    out.push('\n');
}

fn comment_text(comment: &str) -> String {
    if comment.is_empty() {
        "#".into()
    } else {
        format!("# {comment}")
    }
}

fn instr_text(instr: &Instr) -> String {
    match instr {
        Instr::Move { x, y } => format!("move {x} {y}"),
        Instr::MoveRel { dx, dy } => {
            // Relative needs at least one leading '+'; when both deltas are
            // negative only the explicit form can say so.
            if *dx < 0 && *dy < 0 {
                format!("moverel {dx} {dy}")
            } else {
                format!("move {} {}", signed(*dx), signed(*dy))
            }
        }
        Instr::Click { button, at, double } => {
            let verb = if *double { "doubleclick" } else { "click" };
            match at {
                Some((x, y)) => format!("{verb} {} at {x} {y}", button_name(*button)),
                None => format!("{verb} {}", button_name(*button)),
            }
        }
        Instr::MouseDown(button) => format!("mousedown {}", button_name(*button)),
        Instr::MouseUp(button) => format!("mouseup {}", button_name(*button)),
        Instr::Scroll { dx, dy } => {
            debug_assert!((*dx == 0) != (*dy == 0), "scroll must be single-axis");
            let (direction, n) = match (dx, dy) {
                (_, dy) if *dy > 0 => ("up", *dy),
                (_, dy) if *dy < 0 => ("down", -dy),
                (dx, _) if *dx > 0 => ("right", *dx),
                (dx, _) => ("left", -dx),
            };
            if n == 1 {
                format!("scroll {direction}")
            } else {
                format!("scroll {direction} {n}")
            }
        }
        Instr::KeyTap(key) => format!("press {}", key_name(*key)),
        Instr::KeyDown(key) => format!("keydown {}", key_name(*key)),
        Instr::KeyUp(key) => format!("keyup {}", key_name(*key)),
        Instr::Type(text) => format!(
            "type \"{}\"",
            text.replace('\\', "\\\\").replace('"', "\\\"")
        ),
        Instr::Wait { min_us, max_us } => {
            if min_us == max_us {
                format!("wait {}", duration_text(*min_us))
            } else {
                format!(
                    "wait {}..{}",
                    duration_text(*min_us),
                    duration_text(*max_us)
                )
            }
        }
        Instr::Nop | Instr::Repeat { .. } => unreachable!("handled in write_block"),
    }
}

fn signed(v: i32) -> String {
    if v >= 0 {
        format!("+{v}")
    } else {
        v.to_string()
    }
}

fn duration_text(us: u64) -> String {
    if us >= 1_000_000 && us.is_multiple_of(1_000_000) {
        format!("{}s", us / 1_000_000)
    } else if us.is_multiple_of(1_000) {
        format!("{}ms", us / 1_000)
    } else {
        format!("{}ms", us as f64 / 1000.0)
    }
}
