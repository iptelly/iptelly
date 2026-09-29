use crate::types::{AppState, Source};
use crate::{log, types::Channel};
use anyhow::{Context, Result};

use std::process::Stdio;
use tauri::State;
use tokio::sync::Mutex;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use tokio_util::sync::CancellationToken;

// Generic spawn/wait/cancel/play_stop-token bookkeeping shared by every
// external player (mpv, vlc, ...) - only the binary path and CLI args
// differ between players, this part is identical for all of them.
pub async fn run(
    bin_path: &str,
    args: Vec<String>,
    channel: &Channel,
    source: &Option<Source>,
    state: &State<'_, Mutex<AppState>>,
) -> Result<()> {
    eprintln!("Running {bin_path} with args: {:?}", args);

    if let Some(source) = source.as_ref() {
        _ = crate::utils::handle_max_streams(source, state)
            .await
            .map_err(|e| log::log(format!("{:?}", e)));
    }

    let mut cmd = Command::new(bin_path)
        .args(args)
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let token = CancellationToken::new();
    let channel_id = channel.id.context("no channel id")?;
    if let Some(source_id) = source.as_ref().and_then(|s| s.id) {
        _ = crate::utils::insert_play_token(source_id, channel_id.to_string(), token.clone(), state)
            .await
            .map_err(|e| log::log(format!("{:?}", e)));
    }
    // Drains stdout continuously in the background instead of only after
    // cmd.wait() resolves - a pipe has a limited buffer (~64KB on Linux),
    // and if mpv/vlc (or a yt-dlp subprocess hook) writes more than that
    // before exiting, it blocks on write() forever waiting for the buffer
    // to drain. Nothing would read it until wait() returns, but wait()
    // can't return while the player is stuck blocked on that write - a
    // deadlock that freezes the whole app. Reading it as it's produced
    // means the buffer can never fill, so the player can never block.
    let stdout = cmd.stdout.take().context("no stdout")?;
    let output = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        let mut error = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            if !error.is_empty() {
                error.push('\n');
            }
            error.push_str(&line);
        }
        error
    });

    let result: Result<()> = tokio::select! {
        status = cmd.wait() => {
            let status = status?;
            if status.success() {
                Ok(())
            } else {
                let error = output.await.unwrap_or_default();
                if error != "" {
                    Err(anyhow::anyhow!(error))
                } else {
                    Err(anyhow::anyhow!("Player encountered an unknown error"))
                }
            }
        },
        _ = token.cancelled() => {
            cmd.kill().await?;
            Ok(())
        }
    };

    if let Some(source_id) = source.as_ref().and_then(|s| s.id) {
        _ = crate::utils::remove_from_play_stop(state.clone(), &source_id, &channel_id.to_string())
            .await
            .map_err(|e| log::log(format!("{:?}", e)));
    }
    result
}
