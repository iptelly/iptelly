// Delivers the core's background events (download progress, restream
// started, EPG reminders) to the UI thread.

use std::sync::Arc;

use anyhow::Result;
use iptelly_core::events::{EventSink, Events};
use iptelly_core::types::DownloadProgress;
use slint::{ComponentHandle, Weak};

use crate::AppWindow;

struct SlintEvents(Weak<AppWindow>);

impl EventSink for SlintEvents {
    // Nothing shows download progress until the downloads screen is ported
    // (see PORTING.md).
    fn download_progress(&self, _download_id: &str, _progress: DownloadProgress) {}

    fn restream_started(&self) {
        let _ = self.0.upgrade_in_event_loop(|window| {
            window.set_restream_running(true);
        });
    }

    // Shown as a toast until desktop notifications are added (PORTING.md).
    fn notify(&self, title: &str, body: &str) -> Result<()> {
        let message = format!("{title} - {body}");
        let _ = self.0.upgrade_in_event_loop(move |window| {
            crate::show_toast(&window, &message);
        });
        Ok(())
    }
}

pub fn new(window: &AppWindow) -> Events {
    Arc::new(SlintEvents(window.as_weak()))
}
