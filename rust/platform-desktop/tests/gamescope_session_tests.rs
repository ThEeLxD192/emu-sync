//! Detección de gamescope: decide si la ventana ocupa toda la salida para que el puntero y el
//! táctil coincidan con lo dibujado en el modo juego de Steam Deck.

use emusync_platform_desktop::gamescope_session_from;
use std::collections::HashMap;

fn detect(vars: &[(&str, &str)]) -> bool {
    let env: HashMap<&str, &str> = vars.iter().copied().collect();
    gamescope_session_from(|key| env.get(key).map(|value| value.to_string()))
}

#[test]
fn steam_deck_gaming_mode_is_detected() {
    assert!(detect(&[
        ("XDG_CURRENT_DESKTOP", "gamescope"),
        ("GAMESCOPE_WAYLAND_DISPLAY", "gamescope-0"),
    ]));
}

#[test]
fn gamescope_is_detected_inside_a_desktop_list_or_with_another_case() {
    assert!(detect(&[("XDG_CURRENT_DESKTOP", "KDE:Gamescope")]));
}

#[test]
fn nested_gamescope_is_detected_by_its_wayland_display() {
    assert!(detect(&[
        ("XDG_CURRENT_DESKTOP", "KDE"),
        ("GAMESCOPE_WAYLAND_DISPLAY", "gamescope-0"),
    ]));
}

#[test]
fn desktop_sessions_keep_a_regular_window() {
    assert!(!detect(&[]));
    assert!(!detect(&[("XDG_CURRENT_DESKTOP", "KDE")]));
    assert!(!detect(&[
        ("XDG_CURRENT_DESKTOP", "GNOME:gamescope-like"),
        ("GAMESCOPE_WAYLAND_DISPLAY", " "),
        ("SteamGamepadUI", "1"),
    ]));
}
