//! The on-disk format must round-trip losslessly and stay human-readable.

use tomtemacro_core::model::{
    EventKind, Key, MacroEvent, MacroFile, MacroMeta, MouseButton, ScreenInfo,
};

fn sample() -> MacroFile {
    MacroFile::new(
        MacroMeta {
            name: "farm-loop".into(),
            created_utc: "2026-07-02T14:31:07Z".into(),
            os: "linux-x11".into(),
            screen: Some(ScreenInfo {
                width: 2560,
                height: 1440,
                scale: 1.0,
            }),
            notes: "covers every event kind".into(),
        },
        vec![
            MacroEvent {
                delay_us: 0,
                kind: EventKind::MouseMove { x: 812.0, y: 440.5 },
            },
            MacroEvent {
                delay_us: 8_300,
                kind: EventKind::ButtonPress(MouseButton::Left),
            },
            MacroEvent {
                delay_us: 71_200,
                kind: EventKind::ButtonRelease(MouseButton::Left),
            },
            MacroEvent {
                delay_us: 5_000,
                kind: EventKind::ButtonPress(MouseButton::Other(8)),
            },
            MacroEvent {
                delay_us: 5_000,
                kind: EventKind::ButtonRelease(MouseButton::Other(8)),
            },
            MacroEvent {
                delay_us: 230_000,
                kind: EventKind::Wheel { dx: 0, dy: -1 },
            },
            MacroEvent {
                delay_us: 412_000,
                kind: EventKind::KeyPress(Key::KeyE),
            },
            MacroEvent {
                delay_us: 90_400,
                kind: EventKind::KeyRelease(Key::KeyE),
            },
            MacroEvent {
                delay_us: 10_000,
                kind: EventKind::KeyPress(Key::Unknown(238)),
            },
            MacroEvent {
                delay_us: 10_000,
                kind: EventKind::KeyRelease(Key::Unknown(238)),
            },
        ],
    )
}

#[test]
fn ron_roundtrip_is_lossless() {
    let original = sample();
    let text = ron::ser::to_string_pretty(&original, ron::ser::PrettyConfig::default()).unwrap();
    let parsed: MacroFile = ron::from_str(&text).unwrap();
    assert_eq!(original, parsed);
}

#[test]
fn hand_written_ron_parses() {
    // The format a user could plausibly type by hand (and the README shows).
    let text = r#"
(
    version: 1,
    meta: (
        name: "hello",
        created_utc: "2026-07-02T14:31:07Z",
        os: "linux-x11",
        screen: Some((width: 2560, height: 1440, scale: 1.0)),
        notes: "",
    ),
    events: [
        (delay_us: 0,      kind: MouseMove(x: 812.0, y: 440.0)),
        (delay_us: 8300,   kind: ButtonPress(Left)),
        (delay_us: 71200,  kind: ButtonRelease(Left)),
        (delay_us: 412000, kind: KeyPress(KeyE)),
        (delay_us: 90400,  kind: KeyRelease(KeyE)),
        (delay_us: 230000, kind: Wheel(dx: 0, dy: -1)),
    ],
)
"#;
    let parsed: MacroFile = ron::from_str(text).unwrap();
    assert_eq!(parsed.events.len(), 6);
    assert_eq!(parsed.duration_us(), 811_900);
}

#[test]
fn duration_sums_delays() {
    assert_eq!(sample().duration_us(), 841_900);
}
