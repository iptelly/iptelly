// Downloads: movies, catch-up programmes and whole series or seasons, which
// queue and download one episode at a time. Ported from
// src/app/download.service.ts, download-manager/ and home/download-sidebar/.
//
// Series queues run on the tokio runtime and the UI reads the same list,
// so the state is behind a std Mutex rather than in a thread_local; every
// change schedules a redraw of the downloads list and tile progress bars.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use anyhow::Result;
use iptelly_core::types::{Channel, DownloadHistoryItem, DownloadProgress};
use iptelly_core::{api, downloads, sql, utils};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{AppWindow, DownloadAction, DownloadRow, DownloadsState, STATE};

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Queued,
    Downloading,
    Paused,
}

struct Active {
    id: String,
    channel: Channel,
    status: Status,
    progress: f64,
    downloaded: i64,
    total: i64,
}

#[derive(Default)]
struct Downloads {
    // In the order they were added.
    active: Vec<Active>,
    // One flag per running series/season queue; set by Cancel all.
    batches: Vec<Arc<AtomicBool>>,
    history: Vec<DownloadHistoryItem>,
}

static DOWNLOADS: LazyLock<Mutex<Downloads>> = LazyLock::new(|| Mutex::new(Downloads::default()));
// Set while a redraw is already scheduled, so a burst of progress events
// causes one redraw rather than hundreds.
static REDRAW_PENDING: AtomicBool = AtomicBool::new(false);

fn with_downloads<R>(f: impl FnOnce(&mut Downloads) -> R) -> R {
    f(&mut DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn setup(window: &AppWindow) {
    let state = window.global::<DownloadsState>();

    let weak = window.as_weak();
    state.on_category_selected(move |_| {
        if let Some(window) = weak.upgrade() {
            redraw(&window);
        }
    });

    state.on_action(|id, action| row_action(id.to_string(), action));

    state.on_pause_all(|| {
        crate::spawn(downloads::pause_all(&STATE), report_error);
    });

    state.on_cancel_all(|| {
        with_downloads(|d| {
            for batch in &d.batches {
                batch.store(true, Ordering::Relaxed);
            }
        });
        crate::spawn(downloads::cancel_all(&STATE), report_error);
    });

    state.on_restart_all(|| {
        let items: Vec<DownloadHistoryItem> = with_downloads(|d| {
            d.history
                .iter()
                .filter(|h| is_cancelled(&h.status))
                .cloned()
                .collect()
        });
        crate::spawn(
            async move {
                for item in items {
                    restart(item).await;
                }
                Ok(())
            },
            report_error,
        );
    });

    state.on_clear_all(|| {
        crate::spawn(
            crate::blocking(api::clear_cancelled_downloads),
            |window, result| {
                report_error(window, result);
                refresh_history();
            },
        );
    });

    refresh_history();
}

fn report_error(window: &AppWindow, result: Result<()>) {
    if let Err(e) = result {
        crate::show_error(window, &e);
    }
}

fn is_cancelled(status: &str) -> bool {
    matches!(status, "cancelled" | "failed" | "paused")
}

/// Progress reported by the core while a download runs (any thread).
pub fn on_progress(download_id: &str, progress: DownloadProgress) {
    with_downloads(|d| {
        if let Some(a) = d.active.iter_mut().find(|a| a.id == download_id) {
            a.progress = progress.progress;
            a.downloaded = progress.downloaded_bytes;
            a.total = progress.total_bytes;
        }
    });
    changed();
}

/// Schedules a redraw on the UI thread (callable from any thread).
fn changed() {
    if REDRAW_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = slint::invoke_from_event_loop(|| {
        REDRAW_PENDING.store(false, Ordering::Release);
        if let Some(window) = crate::window() {
            redraw(&window);
        }
    });
}

/// Progress (0-100) of every active download, by download id, for the
/// channel tiles.
pub fn progress_by_id() -> Vec<(String, f64)> {
    with_downloads(|d| {
        d.active
            .iter()
            .map(|a| (a.id.clone(), a.progress))
            .collect()
    })
}

pub fn is_active(download_id: &str) -> bool {
    with_downloads(|d| d.active.iter().any(|a| a.id == download_id))
}

/// Whether a series or season queue is running (the Angular app allows one
/// at a time).
pub fn batch_running() -> bool {
    with_downloads(|d| !d.batches.is_empty())
}

fn redraw(window: &AppWindow) {
    let state = window.global::<DownloadsState>();
    let category = state.get_category();
    let (rows, counts) = with_downloads(|d| {
        let rows: Vec<DownloadRow> = match category {
            0 => d.active.iter().map(active_row).collect(),
            1 => d
                .history
                .iter()
                .filter(|h| h.status == "completed")
                .map(history_row)
                .collect(),
            _ => d
                .history
                .iter()
                .filter(|h| is_cancelled(&h.status))
                .map(history_row)
                .collect(),
        };
        let completed = d.history.iter().filter(|h| h.status == "completed").count();
        let cancelled = d.history.iter().filter(|h| is_cancelled(&h.status)).count();
        (rows, [d.active.len(), completed, cancelled])
    });
    state.set_rows(ModelRc::new(VecModel::from(rows)));
    state.set_queued_count(counts[0] as i32);
    state.set_completed_count(counts[1] as i32);
    state.set_cancelled_count(counts[2] as i32);
    crate::home::refresh_download_progress(window);
    crate::epg::refresh_download(window);
}

fn active_row(a: &Active) -> DownloadRow {
    let detail = match a.status {
        Status::Queued => "Queued".to_string(),
        status => {
            let paused = if status == Status::Paused {
                "Paused - "
            } else {
                ""
            };
            let sizes = if a.total > 0 {
                format!(
                    " ({} / {})",
                    format_bytes(a.downloaded),
                    format_bytes(a.total)
                )
            } else {
                String::new()
            };
            format!("{paused}{:.0}%{sizes}", a.progress)
        }
    };
    DownloadRow {
        id: a.id.as_str().into(),
        name: a.channel.name.as_str().into(),
        detail: detail.into(),
        progress: (a.progress / 100.0) as f32,
        status: match a.status {
            Status::Queued => 0,
            Status::Downloading => 1,
            Status::Paused => 2,
        },
    }
}

fn history_row(h: &DownloadHistoryItem) -> DownloadRow {
    let size = h.total_bytes.unwrap_or(h.downloaded_bytes);
    let status = match h.status.as_str() {
        "completed" => 3,
        "paused" => 2,
        "failed" => 5,
        _ => 4,
    };
    let label = match status {
        3 => "Completed",
        2 => "Paused",
        5 => "Failed",
        _ => "Cancelled",
    };
    let detail = if size > 0 {
        format!("{label} - {}", format_bytes(size))
    } else {
        label.to_string()
    };
    DownloadRow {
        id: h.id.as_str().into(),
        name: h.name.as_str().into(),
        detail: detail.into(),
        progress: 0.0,
        status,
    }
}

fn format_bytes(bytes: i64) -> String {
    let mut size = bytes as f64;
    for unit in ["B", "KB", "MB", "GB"] {
        if size < 1024.0 {
            return format!("{size:.1} {unit}");
        }
        size /= 1024.0;
    }
    format!("{size:.1} TB")
}

pub fn refresh_history() {
    crate::spawn(
        crate::blocking(downloads::get_history),
        |window, result| match result {
            Ok(history) => {
                with_downloads(|d| d.history = history);
                redraw(window);
            }
            Err(e) => crate::show_error(window, &e),
        },
    );
}

fn set_status(download_id: &str, status: Status) {
    with_downloads(|d| {
        if let Some(a) = d.active.iter_mut().find(|a| a.id == download_id) {
            a.status = status;
        }
    });
    changed();
}

fn add(download_id: &str, channel: &Channel, status: Status) {
    with_downloads(|d| {
        if let Some(a) = d.active.iter_mut().find(|a| a.id == download_id) {
            a.status = status;
            return;
        }
        d.active.push(Active {
            id: download_id.to_string(),
            channel: channel.clone(),
            status,
            progress: 0.0,
            downloaded: 0,
            total: 0,
        });
    });
    changed();
}

fn remove(download_id: &str) {
    with_downloads(|d| d.active.retain(|a| a.id != download_id));
    changed();
}

/// Downloads one item and waits for it to finish, pause or be cancelled.
pub async fn download(download_id: String, channel: Channel, path: Option<String>) {
    add(&download_id, &channel, Status::Downloading);
    let result = downloads::download(&STATE, crate::events(), channel, &download_id, path).await;
    finished(&download_id, result);
}

async fn resume(download_id: String, channel: Channel) {
    add(&download_id, &channel, Status::Downloading);
    with_downloads(|d| d.history.retain(|h| h.id != download_id));
    let result = downloads::resume(&STATE, crate::events(), download_id.clone(), channel).await;
    finished(&download_id, result);
}

// The core reports a pause or cancel as an error with a known message.
fn finished(download_id: &str, result: Result<()>) {
    let paused = matches!(&result, Err(e) if e.to_string() == "download paused");
    if paused {
        set_status(download_id, Status::Paused);
    } else {
        remove(download_id);
    }
    let _ = slint::invoke_from_event_loop(move || {
        let Some(window) = crate::window() else {
            return;
        };
        match result {
            Ok(()) => crate::show_toast(&window, "Download completed successfully"),
            Err(e) if e.to_string() == "download paused" => {
                crate::show_toast(&window, "Download paused")
            }
            Err(e) if e.to_string() == "download aborted" => {
                crate::show_toast(&window, "Download cancelled")
            }
            Err(e) => crate::show_error(&window, &e),
        }
        refresh_history();
    });
}

pub fn cancel(download_id: String) {
    crate::spawn(
        async move {
            let result = downloads::cancel(&STATE, &download_id).await;
            remove(&download_id);
            result
        },
        |window, result| {
            report_error(window, result);
            refresh_history();
        },
    );
}

async fn restart(item: DownloadHistoryItem) {
    let Some(channel_id) = item.channel_id else {
        let _ = slint::invoke_from_event_loop(|| {
            if let Some(window) = crate::window() {
                crate::show_error(
                    &window,
                    &anyhow::anyhow!("Can't resume - this download has no channel reference"),
                );
            }
        });
        return;
    };
    match crate::blocking(move || sql::get_channel_by_id(channel_id)).await {
        Ok(channel) => resume(item.id, channel).await,
        Err(e) => {
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = crate::window() {
                    crate::show_error(&window, &e);
                }
            });
        }
    }
}

fn row_action(id: String, action: DownloadAction) {
    match action {
        DownloadAction::Pause => {
            crate::spawn(
                async move { downloads::pause(&STATE, &id).await },
                report_error,
            );
        }
        DownloadAction::Resume => {
            let channel = with_downloads(|d| {
                d.active
                    .iter()
                    .find(|a| a.id == id)
                    .map(|a| a.channel.clone())
            });
            match channel {
                Some(channel) => {
                    crate::spawn(
                        async move {
                            resume(id, channel).await;
                            Ok(())
                        },
                        report_error,
                    );
                }
                // A paused item in the history list (from an earlier run).
                None => restart_from_history(id),
            }
        }
        DownloadAction::Restart => restart_from_history(id),
        DownloadAction::Cancel => cancel(id),
        DownloadAction::Play => {
            let Some(path) = with_downloads(|d| {
                d.history
                    .iter()
                    .find(|h| h.id == id)
                    .map(|h| h.path.clone())
            }) else {
                return;
            };
            crate::spawn(downloads::play_file(path), report_error);
        }
        DownloadAction::DeleteFile => {
            crate::spawn(
                async move { downloads::delete_completed_download(&id).await },
                |window, result| {
                    report_error(window, result);
                    refresh_history();
                },
            );
        }
        DownloadAction::DeleteEntry => {
            crate::spawn(
                crate::blocking(move || sql::delete_download_row(&id)),
                |window, result| {
                    report_error(window, result);
                    refresh_history();
                },
            );
        }
    }
}

fn restart_from_history(id: String) {
    let Some(item) = with_downloads(|d| d.history.iter().find(|h| h.id == id).cloned()) else {
        return;
    };
    crate::spawn(
        async move {
            restart(item).await;
            Ok(())
        },
        report_error,
    );
}

/// Whether to ask where to save, rather than using the recording folder.
pub fn ask_where_to_save() -> bool {
    utils::is_container() || crate::home::always_ask_save()
}

/// Queues every episode under `<base>/<show>/<season>/`, skipping files
/// already downloaded, then downloads them one at a time.
pub async fn enqueue_series(
    base: String,
    show: String,
    episodes: Vec<(Channel, String)>,
) -> Result<()> {
    let mut queue = Vec::new();
    let mut skipped = 0;
    for (channel, season) in episodes {
        let Some(id) = channel.id else { continue };
        let file_name = utils::get_filename(
            channel.name.clone(),
            channel.url.clone().unwrap_or_default(),
        )?;
        let path = std::path::Path::new(&base)
            .join(utils::sanitize(show.clone()))
            .join(utils::sanitize(season))
            .join(file_name)
            .to_string_lossy()
            .into_owned();
        if downloads::is_already_downloaded(&channel, &path)
            .await
            .unwrap_or(false)
        {
            skipped += 1;
            continue;
        }
        queue.push((id.to_string(), channel, path));
    }
    if skipped > 0 {
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = crate::window() {
                let plural = if skipped == 1 { "" } else { "s" };
                crate::show_toast(
                    &window,
                    &format!("Skipped {skipped} episode{plural} already downloaded"),
                );
            }
        });
    }
    for (id, channel, path) in &queue {
        let (id_, channel_, path_) = (id.clone(), channel.clone(), path.clone());
        crate::blocking(move || downloads::enqueue(&id_, &channel_, &path_)).await?;
        add(id, channel, Status::Queued);
    }

    let cancelled = Arc::new(AtomicBool::new(false));
    with_downloads(|d| d.batches.push(cancelled.clone()));
    for (id, channel, path) in queue {
        if cancelled.load(Ordering::Relaxed) {
            let _ = downloads::cancel(&STATE, &id).await;
            remove(&id);
            continue;
        }
        // Cancelled on its own while it waited.
        if !is_active(&id) {
            continue;
        }
        download(id.clone(), channel, Some(path)).await;
        // Paused: hold the queue until it's resumed and finishes, or is
        // cancelled.
        while is_active(&id) {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }
    with_downloads(|d| d.batches.retain(|b| !Arc::ptr_eq(b, &cancelled)));
    changed();
    Ok(())
}
