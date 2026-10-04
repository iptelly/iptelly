// Delivers the core's background events (download progress, restream
// started, EPG reminders) to the UI thread.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use iptelly_core::events::{EventSink, Events};
use iptelly_core::log;
use iptelly_core::types::DownloadProgress;
use slint::{ComponentHandle, Weak};

use crate::AppWindow;

struct SlintEvents(Weak<AppWindow>);

impl EventSink for SlintEvents {
    fn download_progress(&self, download_id: &str, progress: DownloadProgress) {
        crate::downloads::on_progress(download_id, progress);
    }

    fn restream_started(&self) {
        let _ = self.0.upgrade_in_event_loop(|window| {
            let state = window.global::<crate::RestreamState>();
            state.set_started(true);
            state.set_loading(false);
        });
    }

    // A desktop notification, or a toast if there's no notification service.
    // Shown from its own thread, as showing one can block (D-Bus on Linux).
    fn notify(&self, title: &str, body: &str) -> Result<()> {
        let (title, body) = (title.to_string(), body.to_string());
        let weak = self.0.clone();
        std::thread::spawn(move || {
            let mut notification = notify_rust::Notification::new();
            notification.appname("IPTelly").summary(&title).body(&body);
            if let Some(icon) = icon_path() {
                notification.icon(&icon.to_string_lossy());
            }
            let shown = notification.show();
            if let Err(e) = shown {
                log::log(format!("Failed to show a notification: {e:?}"));
                let message = format!("{title} - {body}");
                let _ = weak.upgrade_in_event_loop(move |window| {
                    crate::show_toast(&window, &message);
                });
            }
        });
        Ok(())
    }
}

const ICON: &[u8] = include_bytes!("../../../src-tauri/icons/128x128@2x.png");

/// The app icon as a file, for notifications (which take a path or an
/// installed icon's name, and the app may not be installed). Written to the
/// cache folder the first time.
fn icon_path() -> Option<PathBuf> {
    let path = directories::ProjectDirs::from("dev", "iptelly", "iptelly")?
        .cache_dir()
        .join("notification-icon.png");
    let current = std::fs::metadata(&path).is_ok_and(|m| m.len() == ICON.len() as u64);
    if !current {
        let written =
            std::fs::create_dir_all(path.parent()?).and_then(|()| std::fs::write(&path, ICON));
        if let Err(e) = written {
            log::log(format!("Failed to write the notification icon: {e:?}"));
            return None;
        }
    }
    Some(path)
}

pub fn new(window: &AppWindow) -> Events {
    Arc::new(SlintEvents(window.as_weak()))
}
