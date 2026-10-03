use std::sync::Arc;

use anyhow::Result;

use crate::types::DownloadProgress;

// How the backend reaches the UI when the UI didn't ask - download
// progress, a restream coming up, an EPG reminder going off. Each frontend
// implements this over its own event system.
pub trait EventSink: Send + Sync {
    fn download_progress(&self, download_id: &str, progress: DownloadProgress);
    fn restream_started(&self);
    // A desktop notification.
    fn notify(&self, title: &str, body: &str) -> Result<()>;
}

pub type Events = Arc<dyn EventSink>;
