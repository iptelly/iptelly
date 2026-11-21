// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod categories;
mod custom;
mod details;
mod dialog;
mod downloads;
mod epg;
mod events;
mod file_name;
mod home;
mod images;
mod instance;
mod restream;
mod settings_page;
mod setup;
#[cfg(test)]
mod testing;
mod tray;
mod window_state;
mod zoom;

use std::cell::RefCell;
use std::sync::{LazyLock, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result};
use iptelly_core::events::Events;
use iptelly_core::types::AppState;
use iptelly_core::{log, settings, sql, utils};
use slint::{ComponentHandle, Timer, Weak};
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

slint::include_modules!();

// Core calls run here, never on the UI thread: most of them do network or
// database work, and play() doesn't return until the player exits.
// A few workers are plenty for that; one per core (the default) is just
// more threads, each with its own malloc arena.
static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("failed to start the tokio runtime")
});

// Running players, restreams, downloads and the adult-content lock.
pub static STATE: LazyLock<Mutex<AppState>> = LazyLock::new(|| Mutex::new(AppState::default()));

static WINDOW: OnceLock<Weak<AppWindow>> = OnceLock::new();
static EVENTS: OnceLock<Events> = OnceLock::new();

thread_local! {
    static TOAST_TIMER: RefCell<Timer> = RefCell::new(Timer::default());
    // The full text of the last error toast, for the error dialog.
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
    // Kept alive because on X11 the clipboard contents vanish with it.
    static CLIPBOARD: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
}

// glibc raises its mmap threshold each time it frees a large mmapped block,
// after which full-size image decodes (10-25 MB) come from the decoding
// thread's malloc arena instead, and glibc never shrinks that arena again.
// Across the blocking threads that left over 800 MB resident after browsing
// a few categories. Setting the threshold fixes it at 128 KiB, so large
// buffers are always mmapped and go back to the OS when freed.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn use_mmap_for_big_allocations() {
    // SAFETY: mallopt only changes malloc's tuning, before any other threads start.
    unsafe { libc::mallopt(libc::M_MMAP_THRESHOLD, 128 * 1024) };
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn use_mmap_for_big_allocations() {}

fn main() -> Result<()> {
    use_mmap_for_big_allocations();
    _ = utils::check_nuke()
        .with_context(|| "Failed to delete db after nuke request")
        .inspect_err(|e| log::log(format!("{:?}", e)));
    if instance::already_running() {
        return Ok(());
    }
    sql::create_or_initialize_db()?;
    // Downloads left 'downloading' or 'queued' by the app closing mid-transfer.
    if let Err(e) = sql::reconcile_interrupted_downloads() {
        log::log(format!("{:?}", e));
    }

    let window = AppWindow::new()?;
    // Wayland desktops take the taskbar icon from the .desktop file matching
    // this id (the packages' iptelly.desktop, which also sets
    // StartupWMClass=iptelly), not from the window's own icon.
    slint::set_xdg_app_id("iptelly")?;
    instance::listen();
    window_state::setup(&window);
    zoom::setup(&window);
    window.set_version(env!("CARGO_PKG_VERSION").into());
    WINDOW.set(window.as_weak()).ok();
    EVENTS.set(events::new(&window)).ok();
    home::setup(&window);
    setup::setup(&window);
    settings_page::setup(&window);
    categories::setup(&window);
    downloads::setup(&window);
    epg::setup(&window);
    restream::setup(&window);
    custom::setup(&window);
    dialog::setup(&window);

    setup_toasts(&window);

    start();

    // Not window.run(), which quits once the window is hidden: with the tray
    // icon on, closing the window only hides it. window_state quits on close
    // otherwise.
    window.show()?;
    slint::run_event_loop_until_quit()?;
    Ok(())
}

/// Loads the settings and sources and opens the home screen, or setup if
/// there are no sources. Runs at startup, after adding the first source and
/// after importing app data.
pub fn start() {
    spawn(
        blocking(|| Ok((settings::get_settings()?, sql::get_sources()?))),
        |window, result| match result {
            Ok((settings, sources)) => {
                window
                    .global::<Theme>()
                    .set_classic(settings.theme.as_deref() == Some("classic"));
                zoom::set(settings.zoom.unwrap_or(100));
                tray::set(settings.enable_tray_icon.unwrap_or(true));
                if sources.is_empty() {
                    setup::show(window, false);
                } else {
                    home::start(window, settings, sources);
                }
            }
            Err(e) => {
                show_error(window, &e);
                setup::show(window, false);
            }
        },
    );
}

/// Where the core sends download progress, restream and EPG events.
pub fn events() -> Events {
    EVENTS
        .get()
        .expect("events are set before the UI starts")
        .clone()
}

/// The main window. Only valid on the UI thread.
pub fn window() -> Option<AppWindow> {
    WINDOW.get()?.upgrade()
}

/// Runs `future` on the tokio runtime, then hands its result to `then` on
/// the UI thread.
pub fn spawn<T, F>(future: F, then: impl FnOnce(&AppWindow, Result<T>) + Send + 'static)
where
    T: Send + 'static,
    F: Future<Output = Result<T>> + Send + 'static,
{
    RUNTIME.spawn(async move {
        let result = future.await;
        if let Some(window) = WINDOW.get() {
            let _ = window.upgrade_in_event_loop(move |window| then(&window, result));
        }
    });
}

/// For the core's synchronous (database and file) functions.
pub async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(f).await?
}

pub fn show_toast(window: &AppWindow, message: &str) {
    toast(window, message, false);
}

pub fn show_error(window: &AppWindow, error: &anyhow::Error) {
    let details = format!("{error:?}");
    log::log(details.clone());
    error_toast(window, &format!("{error:#}"), details);
}

fn error_toast(window: &AppWindow, message: &str, details: String) {
    LAST_ERROR.set(details);
    toast(window, &format!("{message}\nClick for details"), true);
}

// Clicking an error toast opens the error dialog with the full details.
fn setup_toasts(window: &AppWindow) {
    let weak = window.as_weak();
    window.on_toast_clicked(move || {
        let Some(window) = weak.upgrade() else { return };
        if window.get_toast_error() {
            window.set_toast("".into());
            dialog::show_error(&window, &LAST_ERROR.with_borrow(|e| e.clone()));
        }
    });
}

/// Copies text to the system clipboard.
pub fn copy_to_clipboard(text: String) -> Result<()> {
    CLIPBOARD.with_borrow_mut(|clipboard| {
        if clipboard.is_none() {
            *clipboard = Some(arboard::Clipboard::new()?);
        }
        clipboard.as_mut().unwrap().set_text(text)?;
        Ok(())
    })
}

fn toast(window: &AppWindow, message: &str, error: bool) {
    window.set_toast(message.into());
    window.set_toast_error(error);
    let weak = window.as_weak();
    TOAST_TIMER.with_borrow(|timer| {
        timer.start(
            slint::TimerMode::SingleShot,
            Duration::from_secs(if error { 8 } else { 4 }),
            move || {
                if let Some(window) = weak.upgrade() {
                    window.set_toast("".into());
                }
            },
        )
    });
}

#[cfg(test)]
mod tests {
    use i_slint_backend_testing::ElementHandle;
    use slint::platform::PointerEventButton;

    use super::*;
    use crate::testing::{wait, window};

    fn click_toast(window: &AppWindow) {
        wait(0);
        ElementHandle::find_by_element_id(window, "AppWindow::toast-area")
            .next()
            .expect("no toast on screen")
            .mock_single_click(PointerEventButton::Left);
    }

    #[test]
    fn clicking_an_error_toast_opens_the_error_dialog() {
        let window = window();
        setup_toasts(&window);
        dialog::setup(&window);
        error_toast(&window, "Couldn't load", "the full error".into());
        assert_eq!(window.get_toast(), "Couldn't load\nClick for details");

        click_toast(&window);

        assert_eq!(window.get_toast(), "");
        let dialog = window.global::<DialogState>();
        assert_eq!(dialog.get_kind(), DialogKind::Error);
        assert_eq!(dialog.get_details(), "the full error");
    }

    #[test]
    fn clicking_an_info_toast_does_nothing() {
        let window = window();
        setup_toasts(&window);
        show_toast(&window, "Saved");

        click_toast(&window);

        assert_eq!(window.get_toast(), "Saved");
        assert_eq!(window.global::<DialogState>().get_kind(), DialogKind::None);
    }

    #[test]
    fn toasts_hide_after_4_seconds_or_8_for_errors() {
        let window = window();
        show_toast(&window, "Saved");
        wait(3900);
        assert_eq!(window.get_toast(), "Saved");
        wait(200);
        assert_eq!(window.get_toast(), "");

        error_toast(&window, "Failed", String::new());
        wait(7900);
        assert!(window.get_toast_error());
        assert_ne!(window.get_toast(), "");
        wait(200);
        assert_eq!(window.get_toast(), "");
    }
}
