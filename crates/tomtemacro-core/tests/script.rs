//! Contract tests for the macro script language: parsing, formatting,
//! event conversion, and the tidy transform.

use tomtemacro_core::model::{EventKind, Key, MacroEvent, MouseButton};
use tomtemacro_core::script::{self, Instr, Script, Stmt};

fn body_of(text: &str) -> Vec<Stmt> {
    script::parse(text).expect("parse").body
}

fn instrs_of(text: &str) -> Vec<Instr> {
    body_of(text).into_iter().map(|s| s.instr).collect()
}

#[test]
fn every_command_parses() {
    let instrs = instrs_of(
        "move 812 344\n\
         move -1920 300\n\
         move +10 -20\n\
         move -10 +20\n\
         moverel -10 -20\n\
         click left\n\
         click right at 100 -50\n\
         doubleclick middle\n\
         mousedown left\n\
         mouseup left\n\
         scroll up\n\
         scroll down 3\n\
         scroll left 2\n\
         scroll right 4\n\
         press enter\n\
         keydown ctrl\n\
         keyup ctrl\n\
         type \"Hi #1: \\\"quoted\\\" \\\\ done\"\n\
         wait 500\n\
         wait 250ms\n\
         wait 2s\n\
         wait 1.5s\n\
         wait 100ms..300ms\n",
    );
    assert_eq!(instrs[0], Instr::Move { x: 812, y: 344 });
    assert_eq!(instrs[1], Instr::Move { x: -1920, y: 300 });
    assert_eq!(instrs[2], Instr::MoveRel { dx: 10, dy: -20 });
    assert_eq!(instrs[3], Instr::MoveRel { dx: -10, dy: 20 });
    assert_eq!(instrs[4], Instr::MoveRel { dx: -10, dy: -20 });
    assert_eq!(
        instrs[5],
        Instr::Click {
            button: MouseButton::Left,
            at: None,
            double: false
        }
    );
    assert_eq!(
        instrs[6],
        Instr::Click {
            button: MouseButton::Right,
            at: Some((100, -50)),
            double: false
        }
    );
    assert_eq!(
        instrs[7],
        Instr::Click {
            button: MouseButton::Middle,
            at: None,
            double: true
        }
    );
    assert_eq!(instrs[8], Instr::MouseDown(MouseButton::Left));
    assert_eq!(instrs[9], Instr::MouseUp(MouseButton::Left));
    assert_eq!(instrs[10], Instr::Scroll { dx: 0, dy: 1 });
    assert_eq!(instrs[11], Instr::Scroll { dx: 0, dy: -3 });
    assert_eq!(instrs[12], Instr::Scroll { dx: -2, dy: 0 });
    assert_eq!(instrs[13], Instr::Scroll { dx: 4, dy: 0 });
    assert_eq!(instrs[14], Instr::KeyTap(Key::Return));
    assert_eq!(instrs[15], Instr::KeyDown(Key::ControlLeft));
    assert_eq!(instrs[16], Instr::KeyUp(Key::ControlLeft));
    assert_eq!(instrs[17], Instr::Type("Hi #1: \"quoted\" \\ done".into()));
    assert_eq!(
        instrs[18],
        Instr::Wait {
            min_us: 500_000,
            max_us: 500_000
        }
    );
    assert_eq!(
        instrs[19],
        Instr::Wait {
            min_us: 250_000,
            max_us: 250_000
        }
    );
    assert_eq!(
        instrs[20],
        Instr::Wait {
            min_us: 2_000_000,
            max_us: 2_000_000
        }
    );
    assert_eq!(
        instrs[21],
        Instr::Wait {
            min_us: 1_500_000,
            max_us: 1_500_000
        }
    );
    assert_eq!(
        instrs[22],
        Instr::Wait {
            min_us: 100_000,
            max_us: 300_000
        }
    );
}

#[test]
fn keywords_are_case_insensitive() {
    assert_eq!(
        instrs_of("CLICK Left\nWait 5MS\n")[0],
        Instr::Click {
            button: MouseButton::Left,
            at: None,
            double: false
        }
    );
}

#[test]
fn repeat_blocks_nest() {
    let instrs = instrs_of("repeat 3\n  move 1 2\n  repeat 2\n    click left\n  end\nend\n");
    let Instr::Repeat { count: 3, body, .. } = &instrs[0] else {
        panic!("want outer repeat, got {instrs:?}");
    };
    assert_eq!(body[0].instr, Instr::Move { x: 1, y: 2 });
    let Instr::Repeat { count: 2, body, .. } = &body[1].instr else {
        panic!("want inner repeat");
    };
    assert!(matches!(body[0].instr, Instr::Click { .. }));
}

#[test]
fn comments_and_blanks_are_kept() {
    let body = body_of("# setup\n\nclick left  # the button\n");
    assert_eq!(
        body[0],
        Stmt {
            instr: Instr::Nop,
            comment: Some("setup".into())
        }
    );
    assert_eq!(body[1], Stmt::bare(Instr::Nop));
    assert_eq!(body[2].comment.as_deref(), Some("the button"));
}

#[test]
fn header_directives_become_meta() {
    let script = script::parse(
        "# tomte-macro v1\n\
         # name: Farm loop\n\
         # created: 2026-07-04T10:00:00Z\n\
         # os: linux-x11\n\
         # screen: 2560x1440\n\
         # notes: be careful\n\
         \n\
         click left\n",
    )
    .unwrap();
    assert_eq!(script.meta.name, "Farm loop");
    assert_eq!(script.meta.created_utc, "2026-07-04T10:00:00Z");
    assert_eq!(script.meta.os, "linux-x11");
    assert_eq!(script.meta.notes, "be careful");
    let screen = script.meta.screen.unwrap();
    assert_eq!((screen.width, screen.height), (2560, 1440));
    // Body = the blank separator plus the click.
    assert_eq!(script.body.len(), 2);
}

#[test]
fn ordinary_leading_comments_stay_in_the_body() {
    let script = script::parse("# just a note to self\nclick left\n").unwrap();
    assert_eq!(script.meta.name, "");
    assert_eq!(
        script.body[0].comment.as_deref(),
        Some("just a note to self")
    );
}

#[test]
fn newer_format_versions_are_rejected() {
    let err = script::parse("# tomte-macro v2\nclick left\n").unwrap_err();
    assert_eq!(err.line, 1);
    assert!(err.message.contains("update TomteMacro"), "{}", err.message);
}

#[test]
fn errors_carry_line_numbers() {
    for (text, line, needle) in [
        ("clik left\n", 1, "unknown command"),
        ("click left\npress bogus\n", 2, "unknown key"),
        ("click side\n", 1, "unknown mouse button"),
        ("move 1\n", 1, "usage: move"),
        ("\n\nrepeat 2\nclick left\n", 3, "without a matching 'end'"),
        ("end\n", 1, "without a matching 'repeat'"),
        ("repeat 0\nend\n", 1, "usage: repeat N"),
        ("wait 300ms..100ms\n", 1, "min..max"),
        ("wait abc\n", 1, "bad duration"),
        ("wait 999999s\n", 1, "24 h"),
        ("type \"héllo\"\n", 1, "cannot type"),
        ("type \"open\n", 1, "unterminated"),
        ("type \"a\" b\n", 1, "unexpected text"),
        ("scroll sideways\n", 1, "unknown scroll direction"),
    ] {
        let err = script::parse(text).unwrap_err();
        assert_eq!(err.line, line, "text {text:?} → {}", err.message);
        assert!(
            err.message.contains(needle),
            "text {text:?} → {}",
            err.message
        );
    }
}

#[test]
fn deep_nesting_is_rejected() {
    let mut text = String::new();
    for _ in 0..33 {
        text.push_str("repeat 2\n");
    }
    text.push_str("click left\n");
    for _ in 0..33 {
        text.push_str("end\n");
    }
    let err = script::parse(&text).unwrap_err();
    assert!(err.message.contains("nested deeper"), "{}", err.message);
}

#[test]
fn format_then_parse_is_identity() {
    let text = "# tomte-macro v1\n\
                # name: Round trip\n\
                # screen: 1920x1080\n\
                \n\
                # warm-up\n\
                move 10 20\n\
                wait 8.5ms\n\
                repeat 4  # outer\n\
                  click left at 5 6  # inner comment\n\
                  wait 100ms..2s\n\
                  repeat 2\n\
                    type \"a \\\"b\\\" c\"\n\
                    scroll down 2\n\
                  end\n\
                end  # done\n\
                move +3 -4\n\
                moverel -3 -4\n\
                press unknown-238\n\
                wait 3s\n";
    let script = script::parse(text).unwrap();
    let formatted = script::format(&script);
    assert_eq!(script::parse(&formatted).unwrap(), script);
    // Canonical text is a fixed point.
    assert_eq!(
        script::format(&script::parse(&formatted).unwrap()),
        formatted
    );
}

fn ev(delay_us: u64, kind: EventKind) -> MacroEvent {
    MacroEvent { delay_us, kind }
}

#[test]
fn from_events_collapses_taps_and_folds_gaps() {
    let stmts = script::from_events(&[
        ev(0, EventKind::MouseMove { x: 100.4, y: 200.6 }),
        ev(50_000, EventKind::ButtonPress(MouseButton::Left)),
        ev(80_000, EventKind::ButtonRelease(MouseButton::Left)),
        ev(100_000, EventKind::KeyPress(Key::KeyA)),
        ev(30_000, EventKind::KeyRelease(Key::KeyA)),
        ev(20_000, EventKind::Wheel { dx: 0, dy: -2 }),
    ]);
    let instrs: Vec<Instr> = stmts.into_iter().map(|s| s.instr).collect();
    assert_eq!(
        instrs,
        vec![
            Instr::Move { x: 100, y: 201 },
            Instr::Wait {
                min_us: 50_000,
                max_us: 50_000
            },
            Instr::Click {
                button: MouseButton::Left,
                at: None,
                double: false
            },
            // 80 ms intra-click gap folded into the 100 ms wait.
            Instr::Wait {
                min_us: 180_000,
                max_us: 180_000
            },
            Instr::KeyTap(Key::KeyA),
            // 30 ms intra-tap gap folded into the 20 ms wait.
            Instr::Wait {
                min_us: 50_000,
                max_us: 50_000
            },
            Instr::Scroll { dx: 0, dy: -2 },
        ]
    );
}

#[test]
fn from_events_keeps_slow_or_interleaved_pairs_explicit() {
    // Gap over 500 ms: a deliberate hold.
    let hold = script::from_events(&[
        ev(0, EventKind::ButtonPress(MouseButton::Left)),
        ev(600_000, EventKind::ButtonRelease(MouseButton::Left)),
    ]);
    assert_eq!(hold[0].instr, Instr::MouseDown(MouseButton::Left));
    assert_eq!(hold[2].instr, Instr::MouseUp(MouseButton::Left));

    // Intervening event: a drag.
    let drag = script::from_events(&[
        ev(0, EventKind::ButtonPress(MouseButton::Left)),
        ev(10_000, EventKind::MouseMove { x: 5.0, y: 5.0 }),
        ev(10_000, EventKind::ButtonRelease(MouseButton::Left)),
    ]);
    assert_eq!(drag[0].instr, Instr::MouseDown(MouseButton::Left));
    assert!(matches!(drag[2].instr, Instr::Move { .. }));
    assert_eq!(drag[4].instr, Instr::MouseUp(MouseButton::Left));
}

#[test]
fn from_events_rounds_to_ms_and_drops_zero_waits() {
    let stmts = script::from_events(&[
        ev(0, EventKind::MouseMove { x: 1.0, y: 1.0 }),
        ev(400, EventKind::MouseMove { x: 2.0, y: 2.0 }), // rounds to 0 ms
        ev(8_600, EventKind::MouseMove { x: 3.0, y: 3.0 }), // rounds to 9 ms
    ]);
    let instrs: Vec<Instr> = stmts.into_iter().map(|s| s.instr).collect();
    assert_eq!(
        instrs,
        vec![
            Instr::Move { x: 1, y: 1 },
            Instr::Move { x: 2, y: 2 },
            Instr::Wait {
                min_us: 9_000,
                max_us: 9_000
            },
            Instr::Move { x: 3, y: 3 },
        ]
    );
}

#[test]
fn strip_moves_keeps_only_moves_before_presses() {
    let text = "move 1 1\n\
                wait 10ms\n\
                move 2 2\n\
                wait 10ms\n\
                click left\n\
                move 3 3\n\
                wait 10ms\n\
                move 4 4\n\
                wait 5ms\n\
                press a\n\
                move 5 5\n";
    let mut body = body_of(text);
    let removed = script::strip_moves(&mut body);
    // Moves 1, 3, 4 (a key press is not a button press), and 5 go;
    // only move 2 — directly before the click — survives.
    assert_eq!(removed, 4);
    let instrs: Vec<Instr> = body.into_iter().map(|s| s.instr).collect();
    assert_eq!(
        instrs,
        vec![
            Instr::Wait {
                min_us: 10_000,
                max_us: 10_000
            },
            Instr::Move { x: 2, y: 2 },
            Instr::Wait {
                min_us: 10_000,
                max_us: 10_000
            },
            Instr::Click {
                button: MouseButton::Left,
                at: None,
                double: false
            },
            // The waits around the removed moves 3 and 4 merged.
            Instr::Wait {
                min_us: 15_000,
                max_us: 15_000
            },
            Instr::KeyTap(Key::KeyA),
        ]
    );
}

#[test]
fn strip_moves_recurses_into_repeats_but_not_across_them() {
    let mut body = body_of(
        "move 1 1\n\
         repeat 2\n\
           move 2 2\n\
           wait 5ms\n\
           move 3 3\n\
           mousedown left\n\
           mouseup left\n\
         end\n",
    );
    let removed = script::strip_moves(&mut body);
    // `move 1 1` (before the repeat) and `move 2 2` go; `move 3 3` stays.
    assert_eq!(removed, 2);
    let Instr::Repeat { body: inner, .. } = &body[0].instr else {
        panic!("repeat expected first after strip");
    };
    assert_eq!(inner[1].instr, Instr::Move { x: 3, y: 3 });
}

#[test]
fn strip_moves_sums_ranged_wait_bounds() {
    let mut body = body_of("wait 10ms..20ms\nmove 1 1\nwait 5ms..7ms\npress a\n");
    assert_eq!(script::strip_moves(&mut body), 1);
    assert_eq!(
        body[0].instr,
        Instr::Wait {
            min_us: 15_000,
            max_us: 27_000
        }
    );
}

#[test]
fn stats_multiply_through_repeats() {
    let script = Script {
        meta: Default::default(),
        body: body_of("repeat 10\n  click left\n  wait 100ms..300ms\nend\n"),
    };
    let stats = script.stats();
    assert_eq!(stats.instructions, 20);
    assert_eq!(stats.nominal_us, 10 * 200_000);
}
