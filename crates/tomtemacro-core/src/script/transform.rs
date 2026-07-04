//! Whole-script transforms used by the editor's tidy tools.

use super::{Instr, Stmt};

/// Remove every mouse move except those directly before a button press
/// (`click`, `doubleclick`, or `mousedown` — separated at most by waits,
/// blanks, or comments). Waits made adjacent by a removal are merged, both
/// bounds summed, so the overall timeline is preserved. Recurses into
/// `repeat` bodies but never looks across a block boundary.
///
/// Returns the number of moves removed.
pub fn strip_moves(body: &mut Vec<Stmt>) -> usize {
    let mut removed = 0;
    strip_block(body, &mut removed);
    removed
}

fn strip_block(body: &mut Vec<Stmt>, removed: &mut usize) {
    for stmt in body.iter_mut() {
        if let Instr::Repeat { body: inner, .. } = &mut stmt.instr {
            strip_block(inner, removed);
        }
    }

    // Scan backward tracking whether the next significant instruction
    // presses a mouse button; waits/blanks/comments are transparent.
    let mut keep = vec![true; body.len()];
    let mut next_presses = false;
    for (i, stmt) in body.iter().enumerate().rev() {
        match &stmt.instr {
            Instr::Nop | Instr::Wait { .. } => {}
            Instr::Move { .. } | Instr::MoveRel { .. } => {
                if !next_presses {
                    keep[i] = false;
                    *removed += 1;
                }
                // Even a kept move blocks earlier moves: only the move
                // directly before the press survives.
                next_presses = false;
            }
            Instr::Click { .. } | Instr::MouseDown(_) => next_presses = true,
            _ => next_presses = false,
        }
    }
    let mut index = 0;
    body.retain(|_| {
        index += 1;
        keep[index - 1]
    });

    // Merge runs of now-adjacent waits.
    let mut merged: Vec<Stmt> = Vec::with_capacity(body.len());
    for stmt in body.drain(..) {
        if let Instr::Wait { min_us, max_us } = stmt.instr {
            if let Some(Stmt {
                instr:
                    Instr::Wait {
                        min_us: prev_min,
                        max_us: prev_max,
                    },
                comment: prev_comment,
            }) = merged.last_mut()
            {
                *prev_min = prev_min.saturating_add(min_us);
                *prev_max = prev_max.saturating_add(max_us);
                if prev_comment.is_none() {
                    *prev_comment = stmt.comment;
                }
                continue;
            }
        }
        merged.push(stmt);
    }
    *body = merged;
}
