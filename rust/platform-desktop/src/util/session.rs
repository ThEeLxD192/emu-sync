//! Detection of the graphical session the application runs in.

/// Whether the application runs inside gamescope (Steam Deck Gaming Mode or a nested session).
///
/// Gamescope scales windows that do not fill its output; pointer and touch input then stop
/// matching what is drawn, so the window must cover the whole output there.
pub fn is_gamescope_session() -> bool {
    gamescope_session_from(|key| std::env::var(key).ok())
}

/// [`is_gamescope_session`] over an arbitrary environment lookup.
pub fn gamescope_session_from(var: impl Fn(&str) -> Option<String>) -> bool {
    let desktop_is_gamescope = var("XDG_CURRENT_DESKTOP").is_some_and(|desktops| {
        desktops
            .split(':')
            .any(|desktop| desktop.trim().eq_ignore_ascii_case("gamescope"))
    });
    desktop_is_gamescope
        || var("GAMESCOPE_WAYLAND_DISPLAY").is_some_and(|display| !display.trim().is_empty())
}
