// UI zoom: the window's scale factor is the system's times the zoom. Slint resets it whenever the system's changes
// (moving to another monitor, for one), so it's applied again after that.

use std::cell::Cell;
use std::time::Duration;

use slint::ComponentHandle;
use slint::winit_030::{EventResult, WinitWindowAccessor, winit};

use crate::AppWindow;

pub const MIN: u16 = 50;
pub const MAX: u16 = 300;

thread_local! {
    static ZOOM: Cell<f32> = const { Cell::new(1.0) };
}

pub fn setup(window: &AppWindow) {
    window.window().on_winit_window_event(|_, event| {
        if matches!(event, winit::event::WindowEvent::ScaleFactorChanged { .. }) {
            // After Slint has applied the system's new scale factor.
            slint::Timer::single_shot(Duration::ZERO, apply);
        }
        EventResult::Propagate
    });
}

/// Sets the zoom, in percent.
pub fn set(percent: u16) {
    let zoom = f32::from(percent.clamp(MIN, MAX)) / 100.0;
    if ZOOM.replace(zoom) != zoom {
        apply();
    }
}

fn apply() {
    let Some(window) = crate::window() else {
        return;
    };
    let Some(system) = window
        .window()
        .with_winit_window(|w| w.scale_factor() as f32)
    else {
        // The native window isn't created yet (at startup); try again soon.
        slint::Timer::single_shot(Duration::from_millis(100), apply);
        return;
    };
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
            scale_factor: system * ZOOM.get(),
        });
}
