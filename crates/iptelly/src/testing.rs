// Headless UI tests: Slint's testing backend runs the real UI with no
// window on screen and a mock clock, which i_slint_backend_testing::
// mock_elapsed_time advances (firing timers due by then).

use std::sync::Once;

use iptelly_core::sql;
use slint::{ComponentHandle, LogicalSize};

use crate::AppWindow;

/// A new main window on the calling thread (each test runs on its own).
/// Anything that reaches the database uses a throwaway one, never the
/// user's.
pub fn window() -> AppWindow {
    static DATABASE: Once = Once::new();
    DATABASE.call_once(|| {
        let path = std::env::temp_dir().join(format!("iptelly-test-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        sql::use_db_path(path.to_string_lossy().into_owned());
        sql::create_or_initialize_db().expect("failed to create the test database");
    });
    i_slint_backend_testing::init_no_event_loop();
    let window = AppWindow::new().expect("failed to create the window");
    window.window().set_size(LogicalSize::new(1280.0, 800.0));
    window.show().expect("failed to show the window");
    window
}

/// Advances the mock clock by `ms` milliseconds.
pub fn wait(ms: u64) {
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(ms));
}
