use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use anyhow::{Context, Result};
use tokio::{
    fs,
    sync::{
        Mutex,
        oneshot::{self, Sender},
    },
};

use crate::{
    events::Events,
    mpv,
    settings::get_settings,
    sql,
    types::{AppState, Channel, CustomChannel, NetworkInfo},
    utils::{get_bin, serialize_to_file},
};

const WAN_IP_API: &str = "https://api.ipify.org";
pub const FFMPEG_BIN_NAME: &str = "ffmpeg";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

fn start_ffmpeg_listening(channel: Channel, restream_dir: PathBuf) -> Result<Child> {
    let headers =
        sql::get_channel_headers_by_id(channel.id.context("no channel id")?)?.unwrap_or_default();
    let source = channel
        .source_id
        .and_then(|id| sql::get_source_from_id(id).ok());
    let settings = get_settings()?;
    let playlist_dir = get_playlist_dir(restream_dir);
    let mut command = Command::new(get_bin(FFMPEG_BIN_NAME));
    // Only errors, so the pipe read when ffmpeg exits (see
    // ffmpeg_failure) can't fill up and stall it while it runs.
    command.args(["-hide_banner", "-nostats", "-loglevel", "error"]);

    // Everything up to -i applies to the input: the provider connection.
    // The same user agent and network interface mpv would use.
    if let Some(user_agent) = headers
        .user_agent
        .or_else(|| source.as_ref().and_then(|s| s.stream_user_agent.clone()))
    {
        command.arg("-user_agent").arg(user_agent);
    }
    // ffmpeg keeps only the last -headers, so they go in one value.
    let mut extra_headers = String::new();
    if let Some(referrer) = headers.referrer {
        extra_headers.push_str(&format!("Referer: {referrer}\r\n"));
    }
    if let Some(origin) = headers.http_origin {
        extra_headers.push_str(&format!("Origin: {origin}\r\n"));
    }
    if !extra_headers.is_empty() {
        command.arg("-headers").arg(extra_headers);
    }
    if headers.ignore_ssl == Some(true) {
        command.args(["-tls_verify", "0"]);
    }
    if let Some(address) = settings.network_interface {
        command.arg("-local_addr").arg(address);
    }
    command.args([
        "-reconnect",
        "1",
        "-reconnect_at_eof",
        "1",
        "-reconnect_streamed",
        "1",
        "-reconnect_on_network_error",
        "1",
    ]);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let child = command
        .arg("-i")
        .arg(channel.url.context("no channel url")?)
        .args([
            "-c",
            "copy",
            "-f",
            "hls",
            "-hls_time",
            "5",
            "-hls_list_size",
            "6",
            "-hls_flags",
            "delete_segments",
        ])
        .arg(playlist_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    Ok(child)
}

// What ffmpeg printed before exiting, for the error shown to the user.
fn ffmpeg_failure(child: &mut Child) -> anyhow::Error {
    let mut output = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        use std::io::Read;
        let _ = stderr.read_to_string(&mut output);
    }
    let output = output.trim();
    if output.is_empty() {
        anyhow::anyhow!("ffmpeg stopped before the re-stream started")
    } else {
        anyhow::anyhow!("ffmpeg stopped: {output}")
    }
}

async fn start_web_server(
    restream_dir: PathBuf,
    port: u16,
) -> Result<(Sender<bool>, tokio::task::JoinHandle<()>)> {
    let file_server = warp::fs::dir(restream_dir);
    let (tx, rx) = oneshot::channel::<bool>();
    let (_, server) =
        warp::serve(file_server).bind_with_graceful_shutdown(([0, 0, 0, 0], port), async {
            rx.await.ok();
        });
    let handle = tokio::spawn(server);
    return Ok((tx, handle));
}

pub async fn start_restream(
    port: u16,
    state: &Mutex<AppState>,
    events: Events,
    channel: Channel,
) -> Result<()> {
    let stop = state.lock().await.restream_stop_signal.clone();
    stop.store(false, std::sync::atomic::Ordering::Relaxed);
    let restream_dir = get_restream_folder()?;
    delete_old_segments(&restream_dir).await?;
    let playlist = PathBuf::from(get_playlist_dir(restream_dir.clone()));
    let mut ffmpeg_child = start_ffmpeg_listening(channel, restream_dir.clone())?;
    let (web_server_tx, web_server_handle) = start_web_server(restream_dir, port).await?;

    // Only report it started once there's something to watch: ffmpeg
    // writes the playlist after its first segment.
    let mut started = false;
    let mut failure = None;
    while !stop.load(std::sync::atomic::Ordering::Relaxed) && !web_server_handle.is_finished() {
        match ffmpeg_child.try_wait() {
            Ok(None) => {}
            Ok(Some(status)) => {
                if !status.success() || !started {
                    failure = Some(ffmpeg_failure(&mut ffmpeg_child));
                }
                break;
            }
            Err(e) => {
                failure = Some(e.into());
                break;
            }
        }
        if !started && playlist.exists() {
            started = true;
            events.restream_started();
        }
        tokio::time::sleep(Duration::from_millis(500)).await
    }
    let _ = ffmpeg_child.kill();
    let _ = web_server_tx.send(true);
    let _ = ffmpeg_child.wait();
    let _ = web_server_handle.await;
    match failure {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

pub async fn stop_restream(state: &Mutex<AppState>) -> Result<()> {
    let state = state.lock().await;
    state
        .restream_stop_signal
        .store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

fn get_playlist_dir(mut folder: PathBuf) -> String {
    folder.push("stream.m3u8");
    folder.to_string_lossy().to_string()
}

fn get_restream_folder() -> Result<PathBuf> {
    let mut path = crate::paths::cache_dir()?;
    path.push("restream");
    if !path.exists() {
        std::fs::create_dir_all(&path).context("Failed to create restream cache directory")?;
    }
    Ok(path)
}

async fn delete_old_segments(dir: &Path) -> Result<()> {
    fs::remove_dir_all(dir).await?;
    fs::create_dir_all(dir).await?;
    Ok(())
}

pub async fn watch_self(port: u16, state: &Mutex<AppState>) -> Result<()> {
    let channel = Channel {
        url: Some(format!("http://127.0.0.1:{port}/stream.m3u8").to_string()),
        name: "Local livestream".to_string(),
        favorite: false,
        group: None,
        group_id: None,
        id: Some(-1),
        image: None,
        media_type: crate::media_type::LIVESTREAM,
        series_id: None,
        source_id: None,
        stream_id: None,
        tv_archive: None,
        tvg_id: None,
        season_id: None,
        episode_num: None,
        hidden: Some(false),
        is_adult: false,
    };
    mpv::play(channel, false, None, state).await
}

pub fn share_restream(address: String, channel: Channel, path: String) -> Result<()> {
    let channel = CustomChannel {
        headers: sql::get_channel_headers_by_id(channel.id.context("No id on channel?")?)?,
        data: Channel {
            id: Some(-1),
            name: format!("RST | {}", channel.name).to_string(),
            url: Some(address),
            group: None,
            image: channel.image,
            media_type: crate::media_type::LIVESTREAM,
            source_id: None,
            series_id: None,
            group_id: None,
            favorite: false,
            stream_id: None,
            tv_archive: None,
            tvg_id: None,
            season_id: None,
            episode_num: None,
            hidden: Some(false),
            is_adult: false,
        },
    };
    serialize_to_file(channel, path)
}

pub async fn get_network_info() -> Result<NetworkInfo> {
    let port = get_settings()?.restream_port.unwrap_or(3000);
    Ok(NetworkInfo {
        port,
        local_ips: get_ips(port)?,
        wan_ip: get_wan_ip(port).await?,
    })
}

fn get_ips(port: u16) -> Result<Vec<String>> {
    Ok(if_addrs::get_if_addrs()?
        .iter()
        .filter(|i| i.ip().is_ipv4() && !i.ip().is_loopback())
        .map(|i| format!("http://{}:{port}/stream.m3u8", i.ip().to_string()))
        .collect())
}

async fn get_wan_ip(port: u16) -> Result<String> {
    let client = crate::utils::new_http_client_builder()?.build()?;
    Ok(format!(
        "http://{}:{port}/stream.m3u8",
        client.get(WAN_IP_API).send().await?.text().await?
    ))
}
