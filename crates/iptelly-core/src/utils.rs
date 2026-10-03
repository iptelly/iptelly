use crate::types::{AppState, ChannelPreserve, NetworkInterface};
use crate::{
    log::log,
    m3u,
    settings::{get_default_record_path, get_settings},
    source_type, sql,
    types::Source,
    xmltv, xtream,
};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Local, Utc};
use directories::ProjectDirs;
use indexmap::IndexMap;
use regex::Regex;
use reqwest::{Client, ClientBuilder};
use serde::Serialize;
use std::{
    collections::HashSet,
    env::{consts::OS, current_exe},
    fs::File,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::LazyLock,
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use which::which;

const MACOS_POTENTIAL_PATHS: [&str; 3] = [
    "/opt/local/bin",    // MacPorts
    "/opt/homebrew/bin", // Homebrew on AARCH64 Mac
    "/usr/local/bin",    // Homebrew on AMD64 Mac
];

const DEFAULT_USER_AGENT: &str = "IPTelly";

static ILLEGAL_CHARS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[<>:"/\\|?*\x00-\x1F]"#).unwrap());

static TVG_ID_QUALITY_SUFFIX_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@(sd|hd|fhd|uhd|4k)$").unwrap());

// Playlists and EPG guides rarely agree on casing or on whether a quality
// tag (e.g. "@SD"/"@HD") is part of the channel id, even when they're
// describing the exact same channel (confirmed against a real guide: a
// playlist's "BBCParliament.uk@SD" vs a guide's "bbcparliament.uk"). Used
// on both sides of every EPG channel-id comparison so they compare equal.
pub fn normalize_tvg_id(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    TVG_ID_QUALITY_SUFFIX_REGEX.replace(&lower, "").to_string()
}

pub async fn refresh_source(source: Source) -> Result<()> {
    let refresh_start = std::time::Instant::now();
    let id = source.id;
    let source_name = source.name.clone();
    let source_for_epg = source.clone();
    match source.source_type {
        source_type::M3U => m3u::read_m3u8(source, true)?,
        source_type::M3U_LINK => m3u::get_m3u8_from_link(source, true).await?,
        source_type::XTREAM => xtream::get_xtream(source, true).await?,
        source_type::CUSTOM => {}
        _ => return Err(anyhow!("invalid source_type")),
    }
    log(format!(
        "[perf] {}: channel refresh took {:?}",
        source_name,
        refresh_start.elapsed()
    ));
    let epg_start = std::time::Instant::now();
    if let Some(epg_url) = source_for_epg
        .epg_url
        .as_ref()
        .filter(|u| !u.trim().is_empty())
    {
        log(format!(
            "Refreshing EPG for source {} from {}",
            source_for_epg.name, epg_url
        ));
        if let Err(e) = xmltv::refresh_epg(source_for_epg.clone()).await {
            log(format!(
                "Failed to refresh EPG for source {}: {:?}",
                source_for_epg.name, e
            ));
        }
    } else if source_for_epg.source_type == source_type::XTREAM {
        log(format!(
            "Refreshing EPG for source {} from xmltv.php",
            source_for_epg.name
        ));
        if let Err(e) = xtream::refresh_xtream_epg(source_for_epg.clone()).await {
            log(format!(
                "Failed to refresh EPG for source {}: {:?}",
                source_for_epg.name, e
            ));
        }
    }
    log(format!(
        "[perf] {}: EPG refresh took {:?}",
        source_name,
        epg_start.elapsed()
    ));
    if let Some(id) = id {
        sql::update_source_last_updated(id)?;
    }
    log(format!(
        "[perf] {}: total refresh_source took {:?}",
        source_name,
        refresh_start.elapsed()
    ));
    Ok(())
}

pub async fn refresh_all() -> Result<()> {
    let sources = sql::get_sources()?;
    for source in sources {
        refresh_source(source).await?;
    }
    Ok(())
}

pub fn get_local_time(timestamp: i64) -> Result<DateTime<Local>> {
    let datetime = DateTime::<Utc>::from_timestamp(timestamp, 0).context("no time")?;
    Ok(DateTime::<Local>::from(datetime))
}

pub async fn remove_from_play_stop(
    state: &Mutex<AppState>,
    source_id: &i64,
    key: &str,
) -> Result<Option<CancellationToken>> {
    let mut state = state.lock().await;
    let map = state
        .play_stop
        .get_mut(&source_id)
        .context("no indexMap for sourceId")?;
    Ok(map.shift_remove(key))
}

pub async fn handle_max_streams(source: &Source, state: &Mutex<AppState>) -> Result<()> {
    let max_streams = source.max_streams.unwrap_or(1);
    let mut guard = state.lock().await;
    let channels = guard
        .play_stop
        .get_mut(source.id.as_ref().context("no id")?);
    if channels.is_none() {
        return Ok(());
    }
    let channels = channels.context("no channels")?;
    if channels.len() < max_streams.into() {
        return Ok(());
    }
    let (_, token) = channels
        .shift_remove_index(0)
        .context("failed to remove channel from indexMap")?;
    token.cancel();
    Ok(())
}

pub async fn insert_play_token(
    source_id: i64,
    key: String,
    token: CancellationToken,
    state: &Mutex<AppState>,
) -> Result<()> {
    let mut guard = state.lock().await;
    if guard.play_stop.get(&source_id).is_none() {
        guard
            .play_stop
            .insert(source_id, IndexMap::<String, CancellationToken>::new());
    }
    let map = guard
        .play_stop
        .get_mut(&source_id)
        .context("no indexMap found")?;
    map.insert(key, token);
    Ok(())
}

pub(crate) fn get_filename(channel_name: String, url: String) -> Result<String> {
    let extension = get_extension(url);
    let channel_name = sanitize(channel_name);
    let filename = format!("{channel_name}.{extension}").to_string();
    Ok(filename)
}

// Only the last path segment can hold the file's extension - dots in the
// host, query string or fragment don't count. PHP endpoints (e.g. Xtream's
// get.php) serve the stream rather than a file of that type, so they fall
// back to mp4 like URLs with no extension at all. Matches getExtension in
// the frontend's utils.ts.
fn get_extension(url: String) -> String {
    let without_query = url.split(['?', '#']).next().unwrap_or_default();
    let path = match without_query.split_once("://") {
        Some((_, rest)) => rest.find('/').map_or("", |i| &rest[i..]),
        None => without_query,
    };
    let file_name = path.rsplit('/').next().unwrap_or_default();
    match file_name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() && !ext.eq_ignore_ascii_case("php") => ext.to_string(),
        _ => "mp4".to_string(),
    }
}

pub fn sanitize(str: String) -> String {
    ILLEGAL_CHARS_REGEX.replace_all(&str, "").to_string()
}

// Exposed to the frontend (via a thin command wrapper) so "Download Series"
// can build its own <base>/<show>/<season>/<episode> paths - the plain
// single-file case still goes through get_download_path below, which just
// appends a flat filename to this same base.
pub fn get_download_base_path() -> Result<String> {
    let settings = get_settings()?;
    match settings.recording_path {
        Some(path) => Ok(path),
        None => get_default_record_path(),
    }
}

pub(crate) fn get_download_path(file_name: String) -> Result<String> {
    let mut path = Path::new(&get_download_base_path()?).to_path_buf();
    path.push(file_name);
    Ok(path.to_string_lossy().to_string())
}

// <base>/<show>/<season>/<file> - shared by single-episode downloads
// (download(), above) and the frontend's "Download Series"/"Download
// Season" loops, which build this same structure client-side per episode
// (see channel-tile.component.ts) since get_download_base_path() is
// exposed to them for exactly that.
pub(crate) fn get_series_download_path(
    series_name: &str,
    season_name: &str,
    file_name: &str,
) -> Result<String> {
    let mut path = Path::new(&get_download_base_path()?).to_path_buf();
    path.push(sanitize(series_name.to_string()));
    path.push(sanitize(season_name.to_string()));
    path.push(file_name);
    Ok(path.to_string_lossy().to_string())
}

// Flatpak creates this file in every sandbox.
pub fn is_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

pub fn get_bin(bin: &str) -> String {
    if OS == "linux" || which(bin).is_ok() {
        return bin.to_string();
    } else if OS == "macos" {
        return find_macos_bin(bin);
    }
    return get_bin_from_deps(bin);
}

fn get_bin_from_deps(bin: &str) -> String {
    let mut path = current_exe().unwrap();
    path.pop();
    path.push("deps");
    path.push(bin);
    return path.to_string_lossy().to_string();
}

pub fn find_macos_bin(bin: &str) -> String {
    return MACOS_POTENTIAL_PATHS
        .iter()
        .map(|path| {
            let mut path = Path::new(path).to_path_buf();
            path.push(bin);
            return path;
        })
        .find(|path| path.exists())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            log(format!("Could not find {} on MacOS host", bin));
            return bin.to_string();
        });
}

/// Lists network interfaces with their current IP, for display in Settings.
/// One entry per interface name, preferring its IPv4 address when it has both.
pub fn get_network_interfaces() -> Result<Vec<NetworkInterface>> {
    let mut addrs = if_addrs::get_if_addrs()?;
    addrs.retain(|i| !i.ip().is_loopback());
    addrs.sort_by_key(|i| i.ip().is_ipv6());
    let mut seen = HashSet::new();
    let mut interfaces: Vec<NetworkInterface> = addrs
        .into_iter()
        .filter(|i| seen.insert(i.name.clone()))
        .map(|i| {
            let ip = i.ip().to_string();
            NetworkInterface { name: i.name, ip }
        })
        .collect();
    interfaces.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(interfaces)
}

/// Binds to a specific, literal IP address. Note this doesn't get re-resolved:
/// if the address changes (e.g. a VPN reconnects with a new IP), requests will
/// fail until the user re-selects the new address in Settings.
fn bind_to_address(builder: ClientBuilder, address: &str) -> Result<ClientBuilder> {
    let ip: IpAddr = address.parse().with_context(|| {
        format!("Invalid IP address '{address}' in Settings > Network interface")
    })?;
    Ok(builder.local_address(ip))
}

/// Base builder for every outgoing HTTP client in the app. Binds to the
/// user's configured IP address (Settings > Network interface), if any.
///
/// A short connect_timeout is set because a connection bound to an address
/// without a working route to the destination (e.g. a split-tunnel VPN)
/// doesn't fail fast on its own; the connection just hangs until the OS gives up.
pub fn new_http_client_builder() -> Result<ClientBuilder> {
    let mut builder = Client::builder().connect_timeout(Duration::from_secs(10));
    if let Some(address) = get_settings()?.network_interface {
        builder = bind_to_address(builder, &address)?;
    }
    Ok(builder)
}

// Every plain (no extra headers/SSL flags) outgoing client just sets a
// user agent on top of new_http_client_builder() - this covers that common
// case in one call. download() still builds on new_http_client_builder()
// directly since it also conditionally sets default_headers/
// danger_accept_invalid_certs and its user agent is optional.
pub fn new_http_client(user_agent: &str) -> Result<Client> {
    Ok(new_http_client_builder()?.user_agent(user_agent).build()?)
}

pub fn serialize_to_file<T: Serialize>(obj: T, path: String) -> Result<()> {
    let data = serde_json::to_string(&obj)?;
    std::fs::write(path, data)?;
    Ok(())
}

pub fn backup_favs(source_id: i64, path: String) -> Result<()> {
    sql::do_tx(|tx| {
        let preserve = sql::get_preserve(tx, source_id)?;
        serialize_to_file(preserve, path)?;
        Ok(())
    })?;
    Ok(())
}

pub fn restore_favs(source_id: i64, path: String) -> Result<()> {
    let data = std::fs::read_to_string(path)?;
    let preserve: Vec<ChannelPreserve> = serde_json::from_str(&data)?;
    sql::do_tx(|tx| {
        sql::restore_preserve(tx, source_id, preserve)?;
        Ok(())
    })?;
    Ok(())
}

pub fn is_container() -> bool {
    std::env::var("container").is_ok()
}

pub fn create_nuke_request() -> Result<()> {
    let path = get_nuke_path()?;
    File::create(path)?;
    std::process::exit(0);
}

fn get_nuke_path() -> Result<PathBuf> {
    let path = ProjectDirs::from("dev", "iptelly", "iptelly").context("project dir not found")?;
    let path = path.cache_dir();
    let path = path.join("nuke.txt");
    Ok(path)
}

pub fn check_nuke() -> Result<()> {
    let path = get_nuke_path()?;
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(path)?;
    let path = ProjectDirs::from("dev", "iptelly", "iptelly").context("project dir not found")?;
    let path = path.data_dir();
    let path = path.join(sql::DB_NAME);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn get_user_agent_from_source(source: &Source) -> Result<String> {
    let user_agent: &str = source
        .user_agent
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(DEFAULT_USER_AGENT);
    Ok(user_agent.to_string())
}

#[cfg(test)]
mod test_utils {
    use super::{get_extension, get_filename, normalize_tvg_id, sanitize};

    #[test]
    fn test_sanitize() {
        assert_eq!(
            "SuperShow Who will win the million".to_string(),
            sanitize("SuperShow: Who will win the million?".to_string())
        );
    }

    #[test]
    fn sanitize_strips_every_illegal_character() {
        assert_eq!(sanitize(r#"a<b>c:d"e/f\g|h?i*j"#.to_string()), "abcdefghij");
        assert_eq!(sanitize("a\x00b\x1Fc".to_string()), "abc");
        assert_eq!(sanitize("Plain name.mkv".to_string()), "Plain name.mkv");
    }

    #[test]
    fn normalize_tvg_id_lowercases_and_trims() {
        assert_eq!(normalize_tvg_id("  BBCOne.UK  "), "bbcone.uk");
    }

    #[test]
    fn normalize_tvg_id_strips_quality_suffixes() {
        for suffix in ["@SD", "@HD", "@FHD", "@UHD", "@4K", "@hd"] {
            assert_eq!(
                normalize_tvg_id(&format!("BBCParliament.uk{suffix}")),
                "bbcparliament.uk"
            );
        }
    }

    #[test]
    fn normalize_tvg_id_keeps_other_suffixes() {
        assert_eq!(normalize_tvg_id("Channel.uk@Plus1"), "channel.uk@plus1");
        assert_eq!(normalize_tvg_id("a@hd.b"), "a@hd.b");
        assert_eq!(normalize_tvg_id(""), "");
    }

    #[test]
    fn get_filename_uses_the_url_extension() {
        assert_eq!(
            get_filename(
                "Movie".to_string(),
                "http://example.com:8080/movie/u/p/1.mkv".to_string()
            )
            .unwrap(),
            "Movie.mkv"
        );
    }

    #[test]
    fn get_filename_sanitizes_the_channel_name() {
        assert_eq!(
            get_filename(
                "Show: Part 1?".to_string(),
                "http://example.com/1.ts".to_string()
            )
            .unwrap(),
            "Show Part 1.ts"
        );
    }

    #[test]
    fn get_filename_defaults_to_mp4_for_php_query_urls() {
        assert_eq!(
            get_filename(
                "Live".to_string(),
                "http://example.com/get.php?username=a&password=b".to_string()
            )
            .unwrap(),
            "Live.mp4"
        );
    }

    #[test]
    fn get_extension_defaults_to_mp4_when_the_path_has_no_extension() {
        for url in [
            "stream",
            "http://example.com/stream",
            "http://example.com:8080/live/u/p/123",
            "http://example.com",
            "http://example.com/",
            "http://192.168.1.10/movies.dir/stream",
        ] {
            assert_eq!(get_extension(url.to_string()), "mp4", "{url}");
        }
    }

    #[test]
    fn get_extension_defaults_to_mp4_for_php_endpoints() {
        for url in [
            "http://example.com/stream.php",
            "http://example.com/stream.PHP",
        ] {
            assert_eq!(get_extension(url.to_string()), "mp4", "{url}");
        }
    }

    #[test]
    fn get_extension_ignores_the_query_string_and_fragment() {
        for (url, expected) in [
            ("http://example.com/movie.mkv?token=a.b", "mkv"),
            ("http://example.com/movie.mp4#t=1.5", "mp4"),
            ("http://example.com/play?file=movie.avi", "mp4"),
        ] {
            assert_eq!(get_extension(url.to_string()), expected, "{url}");
        }
    }
}
