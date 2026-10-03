// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod dialog;
mod events;
mod home;
mod images;
mod settings_page;
mod setup;

use std::cell::RefCell;
use std::sync::{LazyLock, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result};
use iptelly_core::types::AppState;
use iptelly_core::{log, settings, sql, utils};
use slint::{ComponentHandle, Timer, Weak};
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

slint::include_modules!();

// Core calls run here, never on the UI thread: most of them do network or
// database work, and play() doesn't return until the player exits.
static RUNTIME: LazyLock<Runtime> =
    LazyLock::new(|| Runtime::new().expect("failed to start the tokio runtime"));

// What src-tauri keeps in tauri::State: running players, restreams,
// downloads and the adult-content lock.
pub static STATE: LazyLock<Mutex<AppState>> = LazyLock::new(|| Mutex::new(AppState::default()));

static WINDOW: OnceLock<Weak<AppWindow>> = OnceLock::new();

thread_local! {
    static TOAST_TIMER: RefCell<Timer> = RefCell::new(Timer::default());
}

fn main() -> Result<()> {
    _ = utils::check_nuke()
        .with_context(|| "Failed to delete db after nuke request")
        .inspect_err(|e| log::log(format!("{:?}", e)));
    sql::create_or_initialize_db()?;
    // Downloads left 'downloading' or 'queued' by the app closing mid-transfer.
    if let Err(e) = sql::reconcile_interrupted_downloads() {
        log::log(format!("{:?}", e));
    }

    let window = AppWindow::new()?;
    window.set_version(env!("CARGO_PKG_VERSION").into());
    WINDOW.set(window.as_weak()).ok();
    home::setup(&window);
    setup::setup(&window);
    settings_page::setup(&window);
    dialog::setup(&window);
    start();

    window.run()?;
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
    log::log(format!("{:?}", error));
    toast(window, &format!("{error:#}"), true);
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
