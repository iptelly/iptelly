use crate::external_player;
use crate::types::{AppState, ChannelHttpHeaders, Source};
use crate::utils::get_bin;
use crate::{media_type, settings::get_settings, sql, types::Channel};
use anyhow::{Context, Result};

use std::sync::LazyLock;
use tauri::State;
use tokio::sync::Mutex;

const ARG_PLAY_AND_EXIT: &str = "--play-and-exit";
const ARG_TITLE: &str = "--meta-title=";
const ARG_GAIN: &str = "--gain=";
const ARG_HWDEC_ON: &str = "--avcodec-hw=any";
const ARG_HWDEC_OFF: &str = "--avcodec-hw=none";
const ARG_NETWORK_CACHING: &str = "--network-caching=";
const ARG_USER_AGENT: &str = "--http-user-agent=";
const ARG_REFERRER: &str = "--http-referrer=";
const VLC_BIN_NAME: &str = "vlc";
static VLC_PATH: LazyLock<String> = LazyLock::new(|| get_bin(VLC_BIN_NAME));

// VLC is never asked to record (see lib.rs's play command - recording
// always routes to mpv, which is the only one of the three players with a
// working --stream-record equivalent wired up), so this is narrower than
// mpv::play: no record/record_path params.
pub async fn play(channel: Channel, state: State<'_, Mutex<AppState>>) -> Result<()> {
    let source = channel
        .source_id
        .and_then(|id| {
            sql::get_source_from_id(id)
                .with_context(|| format!("failed to fetch source with id {}", id))
                .ok()
        })
        .or(None);
    let args = get_play_args(&channel, &source)?;
    external_player::run(&VLC_PATH, args, &channel, &source, &state).await
}

fn get_play_args(channel: &Channel, source: &Option<Source>) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let settings = get_settings()?;
    let headers = sql::get_channel_headers_by_id(channel.id.context("no channel id?")?)?;
    args.push(ARG_PLAY_AND_EXIT.to_string());
    args.push(format!("{ARG_TITLE}{}", channel.name));
    if settings.use_stream_caching == Some(false) {
        args.push(format!("{ARG_NETWORK_CACHING}300"));
    }
    if settings.enable_hwdec.unwrap_or(true) {
        args.push(ARG_HWDEC_ON.to_string());
    } else {
        args.push(ARG_HWDEC_OFF.to_string());
    }
    if channel.media_type == media_type::LIVESTREAM {
        args.push("--loop".to_string());
    }
    if let Some(volume) = settings.volume {
        args.push(format!("{ARG_GAIN}{}", volume as f32 / 100.0));
    }
    set_headers(headers, &mut args, source);
    if let Some(vlc_params) = settings.vlc_params {
        #[cfg(not(target_os = "windows"))]
        let mut params = shell_words::split(&vlc_params)?;
        #[cfg(target_os = "windows")]
        let mut params = winsplit::split(&vlc_params);
        args.append(&mut params);
    }
    // Everything after this point is treated as a URL, never an option -
    // protects against a malicious playlist/provider crafting a channel
    // URL that looks like a VLC flag.
    args.push("--".to_string());
    args.push(channel.url.clone().context("no url")?);
    if channel.episode_num.is_some() {
        for url in sql::find_all_episodes_after(channel)? {
            args.push(url);
        }
    }
    Ok(args)
}

fn set_headers(headers: Option<ChannelHttpHeaders>, args: &mut Vec<String>, source: &Option<Source>) {
    let headers = headers.unwrap_or_default();
    if let Some(referrer) = headers.referrer {
        args.push(format!("{ARG_REFERRER}{referrer}"));
    }
    if let Some(user_agent) = headers
        .user_agent
        .or_else(|| source.as_ref().and_then(|f| f.stream_user_agent.clone()))
    {
        args.push(format!("{ARG_USER_AGENT}{user_agent}"));
    }
}
