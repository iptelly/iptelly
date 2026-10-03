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
