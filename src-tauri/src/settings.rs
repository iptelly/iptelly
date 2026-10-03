use std::{collections::HashMap, env::consts::OS};

use anyhow::{Context, Result};
use directories::UserDirs;
use sha2::{Digest, Sha256};

use crate::{sql, types::Settings};

pub const MPV_PARAMS: &str = "mpvParams";
pub const USE_STREAM_CACHING: &str = "useStreamingCaching";
pub const RECORDING_PATH: &str = "recordingPath";
pub const DEFAULT_VIEW: &str = "defaultView";
pub const VOLUME: &str = "volume";
pub const REFRESH_ON_START: &str = "refreshOnStart";
pub const RESTREAM_PORT: &str = "restreamPort";
pub const ENABLE_TRAY_ICON: &str = "enableTrayIcon";
pub const ZOOM: &str = "zoom";
pub const DEFAULT_SORT: &str = "defaultSort";
pub const ENABLE_HWDEC: &str = "enableHWDEC";
pub const ALWAYS_ASK_SAVE: &str = "alwaysAskSave";
pub const ENABLE_GPU: &str = "enableGPU";
pub const THEME: &str = "theme";
pub const PLAYER: &str = "player";
pub const VLC_PARAMS: &str = "vlcParams";
pub const NETWORK_INTERFACE: &str = "networkInterface";
pub const LIGHTWEIGHT_MODE: &str = "lightweightMode";

pub fn get_settings() -> Result<Settings> {
    let map = sql::get_settings()?;
    let settings = Settings {
        mpv_params: map.get(MPV_PARAMS).map(|s| s.to_string()),
        recording_path: map.get(RECORDING_PATH).map(|s| s.to_string()),
        use_stream_caching: map.get(USE_STREAM_CACHING).and_then(|s| s.parse().ok()),
        default_view: map.get(DEFAULT_VIEW).and_then(|s| s.parse().ok()),
        volume: map.get(VOLUME).and_then(|s| s.parse().ok()),
        refresh_on_start: map.get(REFRESH_ON_START).and_then(|s| s.parse().ok()),
        restream_port: map.get(RESTREAM_PORT).and_then(|s| s.parse().ok()),
        enable_tray_icon: if OS == "linux" {
            Some(false)
        } else {
            map.get(ENABLE_TRAY_ICON).and_then(|s| s.parse().ok())
        },
        zoom: map.get(ZOOM).and_then(|s| s.parse().ok()),
        default_sort: map.get(DEFAULT_SORT).and_then(|s| s.parse().ok()),
        enable_hwdec: map.get(ENABLE_HWDEC).and_then(|s| s.parse().ok()),
        always_ask_save: map.get(ALWAYS_ASK_SAVE).and_then(|s| s.parse().ok()),
        enable_gpu: map.get(ENABLE_GPU).and_then(|s| s.parse().ok()),
        theme: map.get(THEME).map(|s| s.to_string()),
        // VLC isn't bundled in the Flatpak (only mpv, via the io.mpv.Mpv
        // base app), so a stored "vlc" choice falls back to mpv there.
        player: if crate::utils::is_flatpak() {
            Some("mpv".to_string())
        } else {
            map.get(PLAYER).map(|s| s.to_string())
        },
        vlc_params: map.get(VLC_PARAMS).map(|s| s.to_string()),
        network_interface: map.get(NETWORK_INTERFACE).map(|s| s.to_string()),
        lightweight_mode: map.get(LIGHTWEIGHT_MODE).and_then(|s| s.parse().ok()),
    };
    Ok(settings)
}

pub fn update_settings(settings: Settings) -> Result<()> {
    let mut map: HashMap<String, Option<String>> = HashMap::with_capacity(13);

    map.insert(MPV_PARAMS.to_string(), settings.mpv_params);

    if let Some(recording_path) = settings.recording_path {
        map.insert(RECORDING_PATH.to_string(), Some(recording_path));
    }
    if let Some(use_stream_caching) = settings.use_stream_caching {
        map.insert(
            USE_STREAM_CACHING.to_string(),
            Some(use_stream_caching.to_string()),
        );
    }
    if let Some(default_view) = settings.default_view {
        map.insert(DEFAULT_VIEW.to_string(), Some(default_view.to_string()));
    }
    if let Some(volume) = settings.volume {
        map.insert(VOLUME.to_string(), Some(volume.to_string()));
    }
    if let Some(refresh_on_start) = settings.refresh_on_start {
        map.insert(
            REFRESH_ON_START.to_string(),
            Some(refresh_on_start.to_string()),
        );
    }
    if let Some(port) = settings.restream_port {
        map.insert(RESTREAM_PORT.to_string(), Some(port.to_string()));
    }
    if let Some(enable_tray) = settings.enable_tray_icon {
        map.insert(ENABLE_TRAY_ICON.to_string(), Some(enable_tray.to_string()));
    }
    if let Some(zoom) = settings.zoom {
        map.insert(ZOOM.to_string(), Some(zoom.to_string()));
    }
    if let Some(sort) = settings.default_sort {
        map.insert(DEFAULT_SORT.to_string(), Some(sort.to_string()));
    }
    if let Some(hwdec) = settings.enable_hwdec {
        map.insert(ENABLE_HWDEC.to_string(), Some(hwdec.to_string()));
    }
    if let Some(save) = settings.always_ask_save {
        map.insert(ALWAYS_ASK_SAVE.to_string(), Some(save.to_string()));
    }
    if let Some(gpu) = settings.enable_gpu {
        map.insert(ENABLE_GPU.to_string(), Some(gpu.to_string()));
    }
    map.insert(THEME.to_string(), settings.theme);
    map.insert(PLAYER.to_string(), settings.player);
    map.insert(VLC_PARAMS.to_string(), settings.vlc_params);
    map.insert(NETWORK_INTERFACE.to_string(), settings.network_interface);
    if let Some(lightweight_mode) = settings.lightweight_mode {
        map.insert(
            LIGHTWEIGHT_MODE.to_string(),
            Some(lightweight_mode.to_string()),
        );
    }
    sql::update_settings(map)?;
    Ok(())
}

pub const ADULT_PIN_HASH: &str = "adultPinHash";
// A fixed app-specific pepper mixed into the hash - this is a casual
// content-hiding PIN, not a real auth secret, so the point isn't to defeat a
// targeted attacker (anyone with direct DB access has already bypassed the
// whole point of this feature) - just to avoid a plain, precomputable
// SHA-256 of a short numeric PIN sitting in the settings table as-is.
const ADULT_PIN_PEPPER: &str = "iptelly-adult-pin";

fn hash_adult_pin(pin: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ADULT_PIN_PEPPER.as_bytes());
    hasher.update(pin.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

pub fn has_adult_pin() -> Result<bool> {
    let map = sql::get_settings()?;
    Ok(map.get(ADULT_PIN_HASH).is_some())
}

pub fn set_adult_pin(pin: Option<String>) -> Result<()> {
    let mut map: HashMap<String, Option<String>> = HashMap::with_capacity(1);
    map.insert(ADULT_PIN_HASH.to_string(), pin.map(|p| hash_adult_pin(&p)));
    sql::update_settings(map)?;
    Ok(())
}

pub fn verify_adult_pin(pin: &str) -> Result<bool> {
    let map = sql::get_settings()?;
    Ok(map
        .get(ADULT_PIN_HASH)
        .is_some_and(|stored_hash| *stored_hash == hash_adult_pin(pin)))
}

// Used by app_data's export/import (Settings > Export/Import data) - reads
// and writes the already-hashed value directly, so a backup never needs to
// carry (or re-derive) the plaintext PIN.
pub fn get_adult_pin_hash() -> Result<Option<String>> {
    let map = sql::get_settings()?;
    Ok(map.get(ADULT_PIN_HASH).cloned())
}

pub fn set_adult_pin_hash(hash: Option<String>) -> Result<()> {
    let mut map: HashMap<String, Option<String>> = HashMap::with_capacity(1);
    map.insert(ADULT_PIN_HASH.to_string(), hash);
    sql::update_settings(map)?;
    Ok(())
}

pub fn get_default_record_path() -> Result<String> {
    let user_dirs = UserDirs::new().context("Failed to get user dirs")?;
    let mut path = user_dirs
        .video_dir()
        .context("No videos dir in ~, please set a recording path in Settings")?
        .to_owned();
    path.push("iptelly");
    std::fs::create_dir_all(&path)?;
    Ok(path.to_string_lossy().to_string())
}
