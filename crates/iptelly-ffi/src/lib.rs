// iptelly-core for apps written in other languages. uniffi generates the
// bindings from what's exported here; uniffi-bindgen-react-native turns
// them into a React Native module for the TV app.
//
// Everything that touches the database or the network is async, so the app's
// UI thread never waits on it. The numeric codes (media_type, source_type,
// view_type, sort_type) are the core's constants; uniffi can't export
// constants, so the app keeps its own copy of them.

use std::sync::LazyLock;

use iptelly_core::playback::PlayRequest;
use iptelly_core::types::{
    AppState, Channel, EPG, Filters, MediaInfo, Settings, Source, SourceCounts,
};
use iptelly_core::{
    api, app_data, m3u, paths, playback, settings, source_type, sql, utils, xmltv, xtream,
};
use tokio::sync::Mutex;

uniffi::setup_scaffolding!();

static STATE: LazyLock<Mutex<AppState>> = LazyLock::new(Default::default);

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum IptellyError {
    #[error("{message}")]
    Failed { message: String },
}

impl From<anyhow::Error> for IptellyError {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed {
            message: format!("{error:#}"),
        }
    }
}

type Result<T> = std::result::Result<T, IptellyError>;

/// Runs a database call off the async workers.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Result<T> {
    let result = tokio::task::spawn_blocking(f)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(result?)
}

/// Call once at startup, before anything else, with the folders the OS gave
/// the app for its data and its cache. Creates or updates the database.
#[uniffi::export(async_runtime = "tokio")]
pub async fn init(data_dir: String, cache_dir: String) -> Result<()> {
    blocking(move || {
        paths::set_dirs(data_dir.into(), cache_dir.into())?;
        // A failed reset leaves the old database in place, which still works.
        if let Err(e) = utils::check_nuke() {
            iptelly_core::log::log(format!("Failed to delete db after nuke request: {e:?}"));
        }
        sql::create_or_initialize_db()
    })
    .await
}

// Sources

#[uniffi::export(async_runtime = "tokio")]
pub async fn get_sources() -> Result<Vec<Source>> {
    blocking(sql::get_sources).await
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn source_name_exists(name: String) -> Result<bool> {
    blocking(move || sql::source_name_exists(&name)).await
}

/// Adds a source and loads its channels. For an M3U file, `url` is the
/// file's path. An Xtream server's address can be given as typed
/// ("host:port"); it's turned into its API address.
#[uniffi::export(async_runtime = "tokio")]
pub async fn add_source(mut source: Source) -> Result<()> {
    if source.source_type == source_type::XTREAM {
        source.url = source.url.as_deref().map(xtream::api_url);
    }
    match source.source_type {
        source_type::M3U => blocking(move || m3u::read_m3u8(source, false)).await,
        source_type::M3U_LINK => Ok(m3u::get_m3u8_from_link(source, false).await?),
        source_type::XTREAM => Ok(xtream::get_xtream(source, false).await?),
        source_type::CUSTOM => blocking(move || api::add_custom_source(source.name)).await,
        other => Err(anyhow::anyhow!("unknown source type {other}").into()),
    }
}

/// Reloads a source's channels and guide.
#[uniffi::export(async_runtime = "tokio")]
pub async fn refresh_source(source_id: i64) -> Result<()> {
    let source = blocking(move || sql::get_source_from_id(source_id)).await?;
    Ok(utils::refresh_source(source).await?)
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn refresh_all() -> Result<()> {
    Ok(utils::refresh_all().await?)
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn delete_source(source_id: i64) -> Result<()> {
    blocking(move || sql::delete_source(source_id)).await
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn set_source_enabled(source_id: i64, enabled: bool) -> Result<()> {
    blocking(move || sql::set_source_enabled(enabled, source_id)).await
}

// Browsing

/// A page of channels, categories, series or seasons, depending on
/// `filters`. Adult content is left out while the adult PIN is locked.
#[uniffi::export(async_runtime = "tokio")]
pub async fn search(filters: Filters) -> Result<Vec<Channel>> {
    Ok(api::search(filters, &STATE).await?)
}

/// Downloads an Xtream series' seasons and episodes, if they aren't already
/// loaded. Call before searching inside the series.
#[uniffi::export(async_runtime = "tokio")]
pub async fn load_episodes(series: Channel) -> Result<()> {
    Ok(xtream::get_episodes(series).await?)
}

/// A movie's or series' plot, cast, rating, backdrop and so on, fetched from
/// its Xtream info page. Other sources have no details, so this is empty
/// for them.
#[uniffi::export(async_runtime = "tokio")]
pub async fn get_media_info(channel: Channel) -> Result<MediaInfo> {
    let source_id = channel
        .source_id
        .ok_or_else(|| anyhow::anyhow!("The movie has no playlist."))?;
    let source = blocking(move || sql::get_source_from_id(source_id)).await?;
    if source.source_type != source_type::XTREAM {
        return Ok(MediaInfo::default());
    }
    Ok(xtream::get_media_info(channel).await?)
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn set_favorite(channel_id: i64, favorite: bool) -> Result<()> {
    blocking(move || sql::favorite_channel(channel_id, favorite)).await
}

/// Puts the channel at the top of the history.
#[uniffi::export(async_runtime = "tokio")]
pub async fn add_to_history(channel_id: i64) -> Result<()> {
    blocking(move || sql::add_last_watched(channel_id)).await
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn remove_from_history(channel_id: i64) -> Result<()> {
    blocking(move || sql::remove_last_watched(channel_id)).await
}

// Playback

/// Everything the app's player needs to play `channel`.
#[uniffi::export(async_runtime = "tokio")]
pub async fn play_request(channel: Channel) -> Result<PlayRequest> {
    blocking(move || playback::request(&channel)).await
}

// EPG

/// The channel's programmes between two Unix timestamps.
#[uniffi::export(async_runtime = "tokio")]
pub async fn get_epg(channel: Channel, start: i64, end: i64) -> Result<Vec<EPG>> {
    blocking(move || api::get_epg(channel, start, end)).await
}

/// The programmes of several channels between two Unix timestamps, one list
/// per channel in the same order, for the TV guide. Channels without a guide
/// get an empty list.
#[uniffi::export(async_runtime = "tokio")]
pub async fn get_guide(channels: Vec<Channel>, start: i64, end: i64) -> Result<Vec<Vec<EPG>>> {
    blocking(move || api::get_guide(channels, start, end)).await
}

/// Reloads a source's guide: from its EPG URL (or local file) if it has
/// one, otherwise from the Xtream server. Adding a source doesn't load it.
#[uniffi::export(async_runtime = "tokio")]
pub async fn refresh_epg(source_id: i64) -> Result<()> {
    let source = blocking(move || sql::get_source_from_id(source_id)).await?;
    if source
        .epg_url
        .as_ref()
        .is_some_and(|url| !url.trim().is_empty())
    {
        Ok(xmltv::refresh_epg(source).await?)
    } else if source.source_type == source_type::XTREAM {
        Ok(xtream::refresh_xtream_epg(source).await?)
    } else {
        Ok(())
    }
}

/// Every programme kept for the channel.
#[uniffi::export(async_runtime = "tokio")]
pub async fn get_epg_schedule(channel: Channel) -> Result<Vec<EPG>> {
    blocking(move || api::get_epg_schedule(channel)).await
}

// Settings

/// How many live channels, movies and series a source has.
#[uniffi::export(async_runtime = "tokio")]
pub async fn get_source_counts(source_id: i64) -> Result<SourceCounts> {
    blocking(move || sql::get_source_counts(source_id)).await
}

/// Writes a backup of the playlists, favourites, history and settings to
/// `path`. It includes the playlists' logins.
#[uniffi::export(async_runtime = "tokio")]
pub async fn export_app_data(path: String) -> Result<()> {
    blocking(move || app_data::export_app_data(path)).await
}

/// Restores a backup from `path`. Restored playlists' channels load, and
/// their favourites and history come back, when they're next updated.
#[uniffi::export(async_runtime = "tokio")]
pub async fn import_app_data(path: String) -> Result<()> {
    blocking(move || app_data::import_app_data(path)).await
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn get_settings() -> Result<Settings> {
    blocking(settings::get_settings).await
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn update_settings(settings: Settings) -> Result<()> {
    blocking(move || settings::update_settings(settings)).await
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn has_adult_pin() -> Result<bool> {
    blocking(settings::has_adult_pin).await
}

/// Unlocks adult content until `lock_adult_content` or the app restarts.
#[uniffi::export(async_runtime = "tokio")]
pub async fn verify_adult_pin(pin: String) -> Result<bool> {
    Ok(api::verify_adult_pin(&pin, &STATE).await?)
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn lock_adult_content() {
    api::lock_adult_content(&STATE).await
}

// The core's types, described for uniffi. The fields have to match the
// core's, which the compiler checks.

#[uniffi::remote(Record)]
pub struct Channel {
    pub id: Option<i64>,
    pub name: String,
    pub url: Option<String>,
    pub group: Option<String>,
    pub image: Option<String>,
    pub media_type: u8,
    pub source_id: Option<i64>,
    pub series_id: Option<u64>,
    pub group_id: Option<i64>,
    pub favorite: bool,
    pub stream_id: Option<u64>,
    pub tv_archive: Option<bool>,
    pub season_id: Option<i64>,
    pub episode_num: Option<i64>,
    pub hidden: Option<bool>,
    pub tvg_id: Option<String>,
    pub is_adult: bool,
    pub rating: Option<f64>,
}

#[uniffi::remote(Record)]
pub struct SourceCounts {
    pub channels: u64,
    pub movies: u64,
    pub series: u64,
}

#[uniffi::remote(Record)]
pub struct MediaInfo {
    pub plot: Option<String>,
    pub cast: Option<String>,
    pub director: Option<String>,
    pub genre: Option<String>,
    pub year: Option<String>,
    pub duration_secs: Option<u64>,
    pub rating: Option<f64>,
    pub backdrop: Option<String>,
}

#[uniffi::remote(Record)]
pub struct Source {
    pub id: Option<i64>,
    pub name: String,
    pub url: Option<String>,
    pub url_origin: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub source_type: u8,
    pub use_tvg_id: Option<bool>,
    pub enabled: bool,
    pub user_agent: Option<String>,
    pub max_streams: Option<u8>,
    pub stream_user_agent: Option<String>,
    pub last_updated: Option<i64>,
    pub epg_url: Option<String>,
    pub timezone: Option<String>,
    pub epg_retention_days: Option<u16>,
}

#[uniffi::remote(Record)]
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

#[uniffi::remote(Record)]
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
    pub theme: Option<String>,
    pub player: Option<String>,
    pub vlc_params: Option<String>,
    pub network_interface: Option<String>,
    pub lightweight_mode: Option<bool>,
}

#[uniffi::remote(Record)]
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

#[uniffi::remote(Record)]
pub struct PlayRequest {
    pub title: String,
    pub urls: Vec<String>,
    pub user_agent: Option<String>,
    pub referrer: Option<String>,
    pub origin: Option<String>,
    pub ignore_ssl: bool,
    pub live: bool,
    pub save_position: bool,
    pub resume: bool,
    pub hardware_decoding: bool,
    pub stream_caching: bool,
    pub volume: Option<u8>,
}
