// Remembers the window's size, position and maximized state between runs.

use std::path::PathBuf;
use std::time::Duration;

use iptelly_core::log;
use serde::{Deserialize, Serialize};
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, PhysicalPosition, PhysicalSize};

use crate::AppWindow;

#[derive(Default, Serialize, Deserialize)]
struct WindowState {
    width: u32,
    height: u32,
    // Not known on Wayland, where windows can't place themselves.
    x: Option<i32>,
    y: Option<i32>,
    maximized: bool,
}

fn path() -> Option<PathBuf> {
    Some(
        directories::ProjectDirs::from("dev", "iptelly", "iptelly")?
            .config_dir()
            .join("window-state.json"),
    )
}

fn read() -> Option<WindowState> {
    let text = std::fs::read_to_string(path()?).ok()?;
    serde_json::from_str(&text).ok()
}

/// Restores the saved state, and saves it again when the window closes.
/// Closing quits the app, unless the tray icon is on. Call before the window
/// is shown.
pub fn setup(window: &AppWindow) {
    if let Some(state) = read() {
        if state.width > 0 && state.height > 0 {
            window
                .window()
                .set_size(PhysicalSize::new(state.width, state.height));
        }
        if state.maximized {
            window.window().set_maximized(true);
        }
        if let (Some(x), Some(y)) = (state.x, state.y) {
            // Only once the event loop runs is the winit window (and the
            // list of monitors) available.
            let weak = window.as_weak();
            slint::Timer::single_shot(Duration::ZERO, move || {
                if let Some(window) = weak.upgrade() {
                    restore_position(&window, x, y);
                }
            });
        }
    }

    let weak = window.as_weak();
    window.window().on_close_requested(move || {
        if let Some(window) = weak.upgrade() {
            save(&window);
        }
        if !crate::tray::enabled() {
            let _ = slint::quit_event_loop();
        }
        slint::CloseRequestResponse::HideWindow
    });
}

// Skips a position that's no longer on any monitor (one was unplugged).
fn restore_position(window: &AppWindow, x: i32, y: i32) {
    let on_screen = window.window().with_winit_window(|w| {
        w.available_monitors().any(|m| {
            let (position, size) = (m.position(), m.size());
            x >= position.x
                && y >= position.y
                && x < position.x + size.width as i32
                && y < position.y + size.height as i32
        })
    });
    if on_screen == Some(true) {
        window.window().set_position(PhysicalPosition::new(x, y));
    }
}

fn save(window: &AppWindow) {
    let slint_window = window.window();
    // A maximized window keeps the size and position it had before, to
    // return to when unmaximized.
    let mut state = read().unwrap_or_default();
    state.maximized = slint_window.is_maximized();
    if !state.maximized && !slint_window.is_minimized() {
        let size = slint_window.size();
        state.width = size.width;
        state.height = size.height;
        let position = slint_window.with_winit_window(|w| w.outer_position().ok());
        match position.flatten() {
            Some(position) => {
                state.x = Some(position.x);
                state.y = Some(position.y);
            }
            None => {
                state.x = None;
                state.y = None;
            }
        }
    }
    let result = path()
        .ok_or_else(|| anyhow::anyhow!("No config folder"))
        .and_then(|path| {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, serde_json::to_string(&state)?)?;
            Ok(())
        });
    if let Err(e) = result {
        log::log(format!("Failed to save the window state: {e:?}"));
    }
}
