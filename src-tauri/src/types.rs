use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
    thread::JoinHandle,
};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct Channel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub name: String,
    pub url: Option<String>,
    pub group: Option<String>,
    pub image: Option<String>,
    pub media_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<i64>,
    pub favorite: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tv_archive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_num: Option<i64>,
    pub hidden: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tvg_id: Option<String>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize, Default)]
pub struct Season {
    pub id: Option<i64>,
    pub name: String,
    pub season_number: i64,
    pub image: Option<String>,
    pub series_id: u64,
    pub source_id: i64,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct SeriesEpisode {
    pub channel: Channel,
    pub season_name: String,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct SeasonDownloadInfo {
    pub series_name: String,
    pub episodes: Vec<Channel>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct Source {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    pub source_type: u8,
    pub use_tvg_id: Option<bool>,
    pub enabled: bool,
    pub user_agent: Option<String>,
    pub max_streams: Option<u8>,
    pub stream_user_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epg_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epg_retention_days: Option<u16>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct XtreamStatus {
    pub user_info: XtreamStatusUserInfo,
    #[serde(default)]
    pub server_info: Option<XtreamServerInfo>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct XtreamStatusUserInfo {
    pub exp_date: serde_json::Value,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct XtreamServerInfo {
    pub timezone: Option<String>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct Settings {
    pub recording_path: Option<String>,
    pub mpv_params: Option<String>,
    pub use_stream_caching: Option<bool>,
    pub default_view: Option<u8>,
    pub volume: Option<u8>,
    pub refresh_on_start: Option<bool>,
    pub restream_port: Option<u16>,
    pub enable_tray_icon: Option<bool>,
    pub zoom: Option<u16>,
    pub default_sort: Option<u8>,
    pub enable_hwdec: Option<bool>,
    pub always_ask_save: Option<bool>,
    pub enable_gpu: Option<bool>,
    // "modern" (default, unset) or "classic" - toggles which CSS custom
    // property values apply, see styles.css's :root[data-theme="classic"].
    pub theme: Option<String>,
    // "mpv" (default, unset) or "vlc" - which player handles playback.
    // Recording always uses mpv regardless of this.
    pub player: Option<String>,
    pub vlc_params: Option<String>,
    // A literal local IP address (not an interface name) every outgoing
    // HTTP request (playlists, EPG, downloads) binds to - see
    // utils::new_http_client_builder(). mpv also gets told to use it for
    // stream traffic itself (see mpv.rs).
    pub network_interface: Option<String>,
    // Skips loading channel/movie/series poster images and EPG data.
    pub lightweight_mode: Option<bool>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct Filters {
    pub query: Option<String>,
    pub source_ids: Vec<i64>,
    pub media_types: Option<Vec<u8>>,
    pub view_type: u8,
    pub page: u8,
    pub series_id: Option<i64>,
    pub group_id: Option<i64>,
    pub use_keywords: bool,
    pub sort: u8,
    pub season: Option<i64>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize, Default)]
pub struct ChannelHttpHeaders {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<i64>,
    pub referrer: Option<String>,
    pub user_agent: Option<String>,
    pub http_origin: Option<String>,
    pub ignore_ssl: Option<bool>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct CustomChannel {
    pub data: Channel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<ChannelHttpHeaders>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct Group {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub name: String,
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<i64>,
    pub hidden: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<u8>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct IdName {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct CustomChannelExtraData {
    pub headers: Option<ChannelHttpHeaders>,
    pub group: Option<Group>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct ExportedGroup {
    pub group: Group,
    pub channels: Vec<CustomChannel>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct ExportedSource {
    pub source: Source,
    pub groups: Vec<ExportedGroup>,
    pub channels: Vec<CustomChannel>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct EPG {
    pub epg_id: String,
    pub title: String,
    pub description: String,
    pub start_time: String,
    pub start_timestamp: i64,
    pub end_time: String,
    pub end_timestamp: i64,
    pub timeshift_url: Option<String>,
    pub has_archive: bool,
    pub now_playing: bool,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct EPGNotify {
    pub epg_id: String,
    pub title: String,
    pub start_timestamp: i64,
    pub channel_name: String,
}

#[derive(Debug, Default)]
pub struct AppState {
    pub notify_stop: Arc<AtomicBool>,
    pub thread_handle: Option<JoinHandle<Result<(), anyhow::Error>>>,
    pub restream_stop_signal: Arc<AtomicBool>,

    pub play_stop: HashMap<i64, IndexMap<String, CancellationToken>>,
    // Explicit user pause/cancel for downloads, separate from play_stop
    // above (which still handles eviction by handle_max_streams's
    // per-source connection cap, unchanged) - keyed by download_id, one
    // entry per currently-running download.
    pub download_controls: HashMap<String, tokio::sync::watch::Sender<DownloadControl>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadControl {
    Running,
    Pause,
    Cancel,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct DownloadHistoryItem {
    pub id: String,
    pub channel_id: Option<i64>,
    pub source_id: Option<i64>,
    pub name: String,
    pub path: String,
    pub status: String,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

// Emitted on the "progress-{download_id}" event - byte counts included
// alongside the percentage so the UI can show a "123 MB / 1.2 GB" style
// readout, not just a bare percentage.
#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct DownloadProgress {
    pub progress: f64,
    pub downloaded_bytes: i64,
    pub total_bytes: i64,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct NetworkInfo {
    pub port: u16,
    pub local_ips: Vec<String>,
    pub wan_ip: String,
}

// A pickable local interface/address for Settings > Network interface -
// distinct from NetworkInfo above, which is about the re-stream feature's
// LAN-facing address, not which local address outgoing requests bind to.
#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct NetworkInterface {
    pub name: String,
    pub ip: String,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct ChannelPreserve {
    pub name: String,
    pub favorite: bool,
    pub last_watched: Option<usize>,
    pub hidden: Option<bool>,
    #[serde(default)]
    pub is_group: bool,
}
