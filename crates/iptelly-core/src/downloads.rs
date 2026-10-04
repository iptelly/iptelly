use crate::events::Events;
use crate::log::log;
use crate::sql;
use crate::types::{AppState, Channel, DownloadControl, DownloadHistoryItem, DownloadProgress};
use crate::utils::{
    get_download_path, get_filename, get_series_download_path, handle_max_streams,
    insert_play_token, new_http_client_builder, remove_from_play_stop,
};
use anyhow::{Context, Result, anyhow, bail};
use reqwest::header::{HeaderMap, HeaderValue};
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, watch};

// One shared "now" used for both created_at (only on first insert, since
// ON CONFLICT leaves it untouched) and updated_at.
fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

// Shared by start() (the real download) and probe_remote_size() (the
// already-downloaded verification check below) - same headers/UA/SSL
// handling either way, since it's the same channel being requested.
fn build_channel_client(
    channel: &Channel,
    source: &crate::types::Source,
) -> Result<reqwest::Client> {
    let headers = sql::get_channel_headers_by_id(channel.id.context("no channel id?")?)?;
    let mut client = new_http_client_builder()?;
    let mut headers_map = HeaderMap::new();
    if let Some(headers) = headers.as_ref() {
        if let Some(origin) = headers.http_origin.as_ref() {
            headers_map.insert("Origin", HeaderValue::from_str(origin)?);
        }
        if let Some(referrer) = headers.referrer.as_ref() {
            headers_map.insert("Referer", HeaderValue::from_str(referrer)?);
        }
        if let Some(ignore_ssl) = headers.ignore_ssl {
            if ignore_ssl {
                client = client.danger_accept_invalid_certs(true);
            }
        }
    }
    // Unlike get_user_agent_from_source() (used for Xtream API calls), this
    // deliberately does NOT fall back to DEFAULT_USER_AGENT ("IPTelly") -
    // mpv playback (see mpv.rs's set_headers) only sends --user-agent when
    // one is explicitly configured, otherwise sending nothing and letting
    // mpv/ffmpeg use its own default, so downloads match that "send
    // nothing unless configured" behavior instead of always sending
    // something a provider might not recognize.
    let user_agent = headers
        .and_then(|f| f.user_agent)
        .or(source.stream_user_agent.clone());
    if let Some(user_agent) = user_agent {
        client = client.user_agent(user_agent);
    }
    Ok(client.default_headers(headers_map).build()?)
}

// Xtream Codes panels don't expose any content hash/checksum for a VOD file
// anywhere in their API (confirmed against a real get_series_info response -
// no md5/sha field of any kind) - the closest verifiable signal available
// is the exact remote file size, read via a 1-byte Range probe's
// Content-Range header (mirrors how a real resume already reads it,
// without downloading the file again).
//
// Checked against the actual file at `path` (the exact destination the
// caller is about to download into), not just this app's own download-
// history row - a file downloaded before that history table existed (or
// through some other means) still deserves to be recognized, rather than
// only ever trusting our own bookkeeping. A completed history row's
// recorded size is used as a bonus cross-check when there is one, but its
// absence doesn't disqualify a file that's genuinely sitting on disk.
pub async fn is_already_downloaded(channel: &Channel, path: &str) -> Result<bool> {
    let on_disk_size = match tokio::fs::metadata(path).await {
        Ok(meta) if meta.len() > 0 => meta.len() as i64,
        _ => return Ok(false),
    };
    if let Some(download_id) = channel.id.map(|id| id.to_string()) {
        if let Ok(Some(row)) = sql::get_download_row(&download_id) {
            if row.status == "completed" {
                if let Some(recorded) = row.total_bytes {
                    if recorded != on_disk_size {
                        return Ok(false);
                    }
                }
            }
        }
    }
    // Best-effort remote verification - if the provider can't be reached
    // right now, fall back to trusting the file's presence/size rather than
    // forcing a redundant redownload over a transient network blip.
    match probe_remote_size(channel).await {
        Ok(Some(remote_total)) => Ok(remote_total == on_disk_size),
        _ => Ok(true),
    }
}

async fn probe_remote_size(channel: &Channel) -> Result<Option<i64>> {
    let source_id = channel.source_id.context("no source id")?;
    let source = sql::get_source_from_id(source_id)?;
    let client = build_channel_client(channel, &source)?;
    let url = channel.url.clone().context("no url")?;
    let response = client.get(url).header("Range", "bytes=0-0").send().await?;
    Ok(response
        .headers()
        .get("Content-Range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit('/').next())
        .and_then(|v| v.parse::<i64>().ok()))
}

pub async fn start(
    state: &Mutex<AppState>,
    events: Events,
    channel: Channel,
    download_id: &str,
    path: Option<String>,
    resume_from: Option<u64>,
) -> Result<()> {
    let source_id = channel.source_id.context("no source id provided")?;
    let source = sql::get_source_from_id(source_id)
        .with_context(|| format!("failed to fetch source with id {}", source_id))?;

    _ = handle_max_streams(&source, state)
        .await
        .map_err(|e| log(format!("{:?}", e)));

    let token = tokio_util::sync::CancellationToken::new();
    _ = insert_play_token(source_id, download_id.to_string(), token.clone(), state)
        .await
        .map_err(|e| log(format!("{:?}", e)));

    let (control_tx, mut control_rx) = watch::channel(DownloadControl::Running);
    state
        .lock()
        .await
        .download_controls
        .insert(download_id.to_string(), control_tx);

    let client = build_channel_client(&channel, &source)?;
    let url = channel.url.clone().context("no url provided")?;
    let name = channel.name.clone();
    let path = match path {
        Some(p) => p,
        None => {
            let filename = get_filename(name.clone(), url.clone())?;
            // Only when nothing explicit was passed (an "always ask" save
            // dialog pick is respected as-is, same as before) - if this
            // channel is an episode (its own row links to a season/series),
            // it goes in the same <show>/<season>/ structure "Download
            // Series"/"Download Season" use, instead of dumping episodes
            // flat into the base folder alongside movies.
            match sql::get_episode_folder_names(channel.id.context("no channel id?")?)? {
                Some((series_name, season_name)) => {
                    get_series_download_path(&series_name, &season_name, &filename)?
                }
                None => get_download_path(filename)?,
            }
        }
    };
    // "Download Series"/"Download Season" (and now single-episode downloads
    // above) pass nested show/season folder paths that may not exist yet -
    // File::create doesn't create parent directories itself.
    if let Some(parent) = Path::new(&path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut request = client.get(&url);
    if let Some(offset) = resume_from {
        request = request.header("Range", format!("bytes={offset}-"));
    }
    let mut response = request.send().await?;
    if !response.status().is_success() {
        let error = response.status();
        bail!("Failed to download movie: HTTP {error}")
    }
    // A provider may just ignore the Range header and answer 200 with the
    // full body instead of 206 - detected here rather than trusted, since
    // appending that onto an existing partial file would silently corrupt
    // it. Fall back to a clean full restart in that case.
    let resuming = resume_from.is_some() && response.status().as_u16() == 206;
    let mut downloaded = if resuming { resume_from.unwrap() } else { 0 };
    let total_size = if resuming {
        response
            .headers()
            .get("Content-Range")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    } else {
        response.content_length().unwrap_or(0)
    };

    // File::create truncates, so a resume the server didn't honor (fell
    // through to the `else` branch here since `resuming` is only true on an
    // actual 206) still gets a clean, empty file to restart into - downloaded
    // was already seeded at 0 for that case above.
    //
    // create(true) matters even in the resuming branch: "Restart" on a
    // cancelled download reuses this same path with resume_from=Some(0)
    // (its downloaded_bytes was zeroed on cancel), but cancelling also
    // deletes the file - if the server answers that Range request with 206
    // anyway (common even for "from byte 0"), append-opening a file that no
    // longer exists would fail with ENOENT without this.
    let mut file = if resuming {
        tokio::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .await?
    } else {
        tokio::fs::File::create(&path).await?
    };

    upsert_row(&DownloadHistoryItem {
        id: download_id.to_string(),
        channel_id: channel.id,
        source_id: Some(source_id),
        name: channel.name.clone(),
        path: path.clone(),
        status: "downloading".to_string(),
        downloaded_bytes: downloaded as i64,
        total_bytes: if total_size > 0 {
            Some(total_size as i64)
        } else {
            None
        },
        created_at: now(),
        updated_at: now(),
    });

    let mut send_threshold: f64 = 0.1;
    let outcome: Result<DownloadOutcome> = loop {
        tokio::select! {
          chunk = response.chunk() => {
               match chunk {
                   Ok(Some(chunk)) => {
                       if let Err(e) = file.write(&chunk).await {
                           break Err(e.into());
                       }
                       downloaded += chunk.len() as u64;
                       if total_size > 0 {
                           let progress: f64 = (downloaded as f64 / total_size as f64) * 100.0;
                           let progress = (progress * 10.0).trunc() / 10.0;
                           if progress > send_threshold {
                               events.download_progress(download_id, DownloadProgress {
                                   progress,
                                   downloaded_bytes: downloaded as i64,
                                   total_bytes: total_size as i64,
                               });
                               send_threshold = progress + 0.1 as f64;
                           }
                       }
                   }
                   Ok(None) => break Ok(DownloadOutcome::Completed),
                   Err(e) => break Err(e.into()),
               }
          }
          _ = token.cancelled() => {
               break Ok(DownloadOutcome::Cancelled);
          }
          _ = control_rx.changed() => {
               match *control_rx.borrow() {
                   DownloadControl::Pause => break Ok(DownloadOutcome::Paused),
                   DownloadControl::Cancel => break Ok(DownloadOutcome::Cancelled),
                   DownloadControl::Running => continue,
               }
          }
        }
    };

    drop(file);
    state.lock().await.download_controls.remove(download_id);
    _ = remove_from_play_stop(state, &source_id, &download_id.to_string())
        .await
        .map_err(|e| log(format!("{:?}", e)));

    match outcome {
        Ok(DownloadOutcome::Completed) => {
            upsert_row(&DownloadHistoryItem {
                id: download_id.to_string(),
                channel_id: channel.id,
                source_id: Some(source_id),
                name: channel.name,
                path,
                status: "completed".to_string(),
                downloaded_bytes: downloaded as i64,
                total_bytes: if total_size > 0 {
                    Some(total_size as i64)
                } else {
                    None
                },
                created_at: now(),
                updated_at: now(),
            });
            Ok(())
        }
        Ok(DownloadOutcome::Paused) => {
            upsert_row(&DownloadHistoryItem {
                id: download_id.to_string(),
                channel_id: channel.id,
                source_id: Some(source_id),
                name: channel.name,
                path,
                status: "paused".to_string(),
                downloaded_bytes: downloaded as i64,
                total_bytes: if total_size > 0 {
                    Some(total_size as i64)
                } else {
                    None
                },
                created_at: now(),
                updated_at: now(),
            });
            bail!("download paused")
        }
        Ok(DownloadOutcome::Cancelled) => {
            let _ = tokio::fs::remove_file(&path).await;
            upsert_row(&DownloadHistoryItem {
                id: download_id.to_string(),
                channel_id: channel.id,
                source_id: Some(source_id),
                name: channel.name,
                path,
                status: "cancelled".to_string(),
                downloaded_bytes: 0,
                total_bytes: if total_size > 0 {
                    Some(total_size as i64)
                } else {
                    None
                },
                created_at: now(),
                updated_at: now(),
            });
            bail!("download aborted")
        }
        Err(e) => {
            upsert_row(&DownloadHistoryItem {
                id: download_id.to_string(),
                channel_id: channel.id,
                source_id: Some(source_id),
                name: channel.name,
                path,
                status: "failed".to_string(),
                downloaded_bytes: downloaded as i64,
                total_bytes: if total_size > 0 {
                    Some(total_size as i64)
                } else {
                    None
                },
                created_at: now(),
                updated_at: now(),
            });
            Err(e)
        }
    }
}

enum DownloadOutcome {
    Completed,
    Paused,
    Cancelled,
}

fn upsert_row(item: &DownloadHistoryItem) {
    // Best-effort - a failure to persist history/progress shouldn't fail
    // the download itself.
    let _ = sql::upsert_download_row(item).map_err(|e| log(format!("{:?}", e)));
}

pub async fn pause(state: &Mutex<AppState>, download_id: &str) -> Result<()> {
    send_control(state, download_id, DownloadControl::Pause).await
}

// Unlike pause (only ever called while a download is actively transferring,
// so there's always a live control sender to reach), cancel is also called
// on an already-paused download - by the time it's paused, download()'s
// task has already exited and removed its control entry, so there's
// nothing left to signal. Fall back to discarding it directly in that case:
// delete the partial file and mark its history row cancelled.
pub async fn cancel(state: &Mutex<AppState>, download_id: &str) -> Result<()> {
    if send_control(state, download_id, DownloadControl::Cancel)
        .await
        .is_ok()
    {
        return Ok(());
    }
    if let Some(row) = sql::get_download_row(download_id)? {
        let _ = tokio::fs::remove_file(&row.path).await;
        upsert_row(&DownloadHistoryItem {
            status: "cancelled".to_string(),
            downloaded_bytes: 0,
            updated_at: now(),
            ..row
        });
    }
    Ok(())
}

async fn send_control(
    state: &Mutex<AppState>,
    download_id: &str,
    control: DownloadControl,
) -> Result<()> {
    let guard = state.lock().await;
    let tx = guard
        .download_controls
        .get(download_id)
        .context("no such active download")?;
    tx.send(control)
        .map_err(|_| anyhow!("download already finished"))?;
    Ok(())
}

pub async fn pause_all(state: &Mutex<AppState>) -> Result<()> {
    let guard = state.lock().await;
    for tx in guard.download_controls.values() {
        let _ = tx.send(DownloadControl::Pause);
    }
    Ok(())
}

pub async fn cancel_all(state: &Mutex<AppState>) -> Result<()> {
    let guard = state.lock().await;
    for tx in guard.download_controls.values() {
        let _ = tx.send(DownloadControl::Cancel);
    }
    Ok(())
}

pub async fn resume(
    state: &Mutex<AppState>,
    events: Events,
    download_id: String,
    channel: Channel,
) -> Result<()> {
    let row = sql::get_download_row(&download_id)?.context("no history for this download")?;
    start(
        state,
        events,
        channel,
        &download_id,
        Some(row.path),
        Some(row.downloaded_bytes as u64),
    )
    .await
}

pub fn get_history() -> Result<Vec<DownloadHistoryItem>> {
    sql::list_download_history(200)
}

// Deletes a completed download's actual file from disk, then its history
// row - unlike deleting a cancelled/failed entry (sql::delete_download_row,
// used directly since cancelling already deleted or never created that
// file), a completed entry's file is real and still on disk, so removing
// just the history row without this would just orphan it there silently.
pub async fn delete_completed_download(download_id: &str) -> Result<()> {
    if let Some(row) = sql::get_download_row(download_id)? {
        // Best-effort - a missing/already-removed file shouldn't block
        // clearing the history entry the user asked to delete.
        let _ = tokio::fs::remove_file(&row.path).await;
    }
    sql::delete_download_row(download_id)
}

// Plays an already-downloaded file directly (e.g. from the Downloads tab's
// History). Deliberately not routed through mpv.rs/vlc.rs's play() -
// those are built around a Channel/Source and the play_stop token map for
// enforcing a provider's connection limit and mid-buffer cancellation,
// neither of which applies to a file already sitting on disk - mpv/vlc's
// own window already provides stop control once it opens, same as any
// other playback from this app.
pub async fn play_file(path: String) -> Result<()> {
    let settings = crate::settings::get_settings()?;
    let player = settings.player.unwrap_or_else(|| "mpv".to_string());
    let title = std::path::Path::new(&path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());
    let mut args: Vec<String> = Vec::new();
    let bin = if player == "vlc" {
        args.push("--play-and-exit".to_string());
        args.push(format!("--meta-title={title}"));
        if let Some(volume) = settings.volume {
            args.push(format!("--gain={}", volume as f32 / 100.0));
        }
        crate::utils::get_bin("vlc")
    } else {
        args.push(format!("--title={title}"));
        args.push("--msg-level=all=error".to_string());
        if settings.enable_hwdec.unwrap_or(true) {
            args.push("--hwdec=auto".to_string());
        }
        if settings.enable_gpu.unwrap_or(false) {
            args.push("--vo=gpu-next".to_string());
            args.push("--profile=high-quality".to_string());
        }
        if let Some(volume) = settings.volume {
            args.push(format!("--volume={volume}"));
        }
        crate::utils::get_bin("mpv")
    };
    args.push("--".to_string());
    args.push(path);
    let status = tokio::process::Command::new(bin)
        .args(args)
        .kill_on_drop(true)
        .status()
        .await?;
    if status.success() {
        Ok(())
    } else {
        bail!("Player exited with an error")
    }
}

// Persists a "queued" row for an episode the moment a series/season batch
// registers it, well before its turn to actually start transferring -
// without this, an episode cancelled while still queued (the common case
// when cancelling a large batch partway through) never got a database row
// at all, since that only happened once start() actually began, and simply
// vanished instead of showing up in history as cancelled.
pub fn enqueue(download_id: &str, channel: &Channel, path: &str) -> Result<()> {
    upsert_row(&DownloadHistoryItem {
        id: download_id.to_string(),
        channel_id: channel.id,
        source_id: channel.source_id,
        name: channel.name.clone(),
        path: path.to_string(),
        status: "queued".to_string(),
        downloaded_bytes: 0,
        total_bytes: None,
        created_at: now(),
        updated_at: now(),
    });
    Ok(())
}

// A fresh, non-resuming download.
pub async fn download(
    state: &Mutex<AppState>,
    events: Events,
    channel: Channel,
    download_id: &str,
    path: Option<String>,
) -> Result<()> {
    start(state, events, channel, download_id, path, None).await
}
