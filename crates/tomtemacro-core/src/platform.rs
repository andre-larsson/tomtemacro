//! Session/platform detection: TomteMacro's capture and injection need X11
//! on Linux; Wayland blocks both by design, so we detect it and warn loudly
//! instead of misleadingly half-working through XWayland.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    X11,
    Wayland,
    Windows,
    MacOs,
    Unknown,
}

pub fn detect_session() -> SessionKind {
    #[cfg(target_os = "windows")]
    {
        SessionKind::Windows
    }
    #[cfg(target_os = "macos")]
    {
        SessionKind::MacOs
    }
    #[cfg(target_os = "linux")]
    {
        let xdg = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        if xdg.eq_ignore_ascii_case("wayland") {
            return SessionKind::Wayland;
        }
        if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("DISPLAY").is_none() {
            return SessionKind::Wayland;
        }
        if xdg.eq_ignore_ascii_case("x11") || std::env::var_os("DISPLAY").is_some() {
            return SessionKind::X11;
        }
        SessionKind::Unknown
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        SessionKind::Unknown
    }
}

/// Whether global capture + injection can work in this session.
pub fn input_supported(kind: SessionKind) -> bool {
    matches!(
        kind,
        SessionKind::X11 | SessionKind::Windows | SessionKind::MacOs
    )
}

/// Informational label stored in macro metadata.
pub fn os_label(kind: SessionKind) -> &'static str {
    match kind {
        SessionKind::X11 => "linux-x11",
        SessionKind::Wayland => "linux-wayland",
        SessionKind::Windows => "windows",
        SessionKind::MacOs => "macos",
        SessionKind::Unknown => "unknown",
    }
}
