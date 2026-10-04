// The Settings page and its source tiles.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate, TimeZone};
use iptelly_core::types::{NetworkInterface, Settings, Source};
use iptelly_core::{app_data, settings, share, sort_type, source_type, sql, utils, xmltv, xtream};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::{AppWindow, SettingsState, SourceAction, SourceItem, dialog, file_name};

const SAVE_DELAY: Duration = Duration::from_millis(400);

#[derive(Default)]
struct Page {
    // Last loaded or saved settings; keeps the values this page has no
    // control for.
    settings: Option<Settings>,
    // The IP behind each entry of the interfaces combo box ("" = system
    // default).
    interface_ips: Vec<String>,
    sources: Vec<Source>,
    expiries: HashMap<i64, i64>,
    timezones: HashMap<i64, String>,
    save_timer: Timer,
}

thread_local! {
    static PAGE: RefCell<Page> = RefCell::new(Page::default());
}

fn with_page<R>(f: impl FnOnce(&mut Page) -> R) -> R {
    PAGE.with_borrow_mut(f)
}

pub fn setup(window: &AppWindow) {
    let state = window.global::<SettingsState>();
    state.set_is_flatpak(utils::is_flatpak());
    state.set_has_tray(cfg!(any(target_os = "macos", target_os = "windows")));

    let weak = window.as_weak();
    state.on_changed(move || {
        let weak = weak.clone();
        with_page(|page| {
            page.save_timer
                .start(TimerMode::SingleShot, SAVE_DELAY, move || {
                    if let Some(window) = weak.upgrade() {
                        save(&window);
                    }
                })
        });
    });

    state.on_browse_recording_path(|| {
        crate::spawn(
            async {
                Ok(rfd::AsyncFileDialog::new()
                    .set_title("Select where recordings are saved")
                    .pick_folder()
                    .await
                    .map(|f| f.path().to_string_lossy().into_owned()))
            },
            |window, result| {
                if let Ok(Some(path)) = result {
                    window
                        .global::<SettingsState>()
                        .set_recording_path(path.into());
                    save(window);
                }
            },
        );
    });

    state.on_refresh_interfaces(|| {
        crate::spawn(
            crate::blocking(utils::get_network_interfaces),
            |window, result| match result {
                Ok(interfaces) => show_interfaces(window, interfaces),
                Err(e) => crate::show_error(window, &e),
            },
        );
    });

    let weak = window.as_weak();
    state.on_set_pin(move || {
        if let Some(window) = weak.upgrade() {
            set_pin(&window);
        }
    });

    let weak = window.as_weak();
    state.on_remove_pin(move || {
        let Some(window) = weak.upgrade() else { return };
        dialog::ask_pin(
            &window,
            "Enter PIN",
            "Enter the current PIN to remove it.",
            |_, pin| {
                crate::spawn(
                    async move {
                        if !iptelly_core::api::verify_adult_pin(&pin, &crate::STATE).await? {
                            return Ok(false);
                        }
                        crate::blocking(|| settings::set_adult_pin(None)).await?;
                        Ok(true)
                    },
                    |window, result| match result {
                        Ok(true) => {
                            dialog::close(window);
                            window.global::<SettingsState>().set_pin_set(false);
                            crate::home::set_adult_pin(window, false, true);
                            crate::show_toast(window, "PIN removed");
                        }
                        Ok(false) => dialog::pin_rejected(window),
                        Err(e) => crate::show_error(window, &e),
                    },
                );
            },
        );
    });

    let weak = window.as_weak();
    state.on_add_source(move || {
        if let Some(window) = weak.upgrade() {
            crate::setup::show(&window, true);
        }
    });

    state.on_refresh_all(|| {
        crate::home::clear_series_cache();
        run(
            "Refreshing all sources...",
            utils::refresh_all(),
            "Successfully refreshed all sources",
            "Failed to refresh sources",
        );
    });

    state.on_clear_history(|| {
        run(
            "",
            crate::blocking(sql::clear_history),
            "History cleared successfully",
            "Failed to clear history",
        );
    });

    state.on_clear_epg_cache(|| {
        run(
            "",
            crate::blocking(sql::clear_epg_cache),
            "EPG cache cleared successfully",
            "Failed to clear the EPG cache",
        );
    });

    state.on_export_data(|| {
        crate::spawn(
            async {
                let Some(path) = save_dialog(
                    "Select where to save your exported data",
                    "iptelly_backup.otva",
                    "otva",
                )
                .await
                else {
                    return Ok(false);
                };
                crate::blocking(move || app_data::export_app_data(path)).await?;
                Ok(true)
            },
            |window, result| match result {
                Ok(true) => crate::show_toast(window, "Data exported successfully"),
                Ok(false) => {}
                Err(e) => crate::show_error(window, &e.context("Failed to export data")),
            },
        );
    });

    state.on_import_data(|| {
        crate::spawn(
            async {
                let Some(path) = open_dialog("Select an exported data file", &["otva"]).await
                else {
                    return Ok(false);
                };
                crate::blocking(move || app_data::import_app_data(path)).await?;
                Ok(true)
            },
            |window, result| match result {
                // Settings, sources and favourites may all have changed, so
                // start over as if the app had just opened.
                Ok(true) => {
                    crate::show_toast(window, "Data imported successfully");
                    crate::start();
                }
                Ok(false) => {}
                Err(e) => crate::show_error(window, &e.context("Failed to import data")),
            },
        );
    });

    let weak = window.as_weak();
    state.on_delete_everything(move || {
        if let Some(window) = weak.upgrade() {
            confirm_delete_everything(&window);
        }
    });

    let weak = window.as_weak();
    state.on_source_action(move |index, action| {
        if let Some(window) = weak.upgrade() {
            source_action(&window, index as usize, action);
        }
    });

    let weak = window.as_weak();
    state.on_add_channel(move |index| {
        if let (Some(window), Some(id)) = (weak.upgrade(), source_id(index)) {
            crate::custom::add_channel(&window, id);
        }
    });
    let weak = window.as_weak();
    state.on_add_group(move |index| {
        if let (Some(window), Some(id)) = (weak.upgrade(), source_id(index)) {
            crate::custom::add_group(&window, id);
        }
    });
    let weak = window.as_weak();
    state.on_import(move |index| {
        if let (Some(window), Some(id)) = (weak.upgrade(), source_id(index)) {
            crate::custom::import(&window, id);
        }
    });
}

fn source_id(index: i32) -> Option<i64> {
    with_page(|page| page.sources.get(index as usize)?.id)
}

/// Asks before wiping all data (also offered on the first-run setup page).
pub fn confirm_delete_everything(window: &AppWindow) {
    dialog::confirm(
        window,
        "Confirm deletion of all user data",
        "This deletes all your sources, channels, favourites, history and settings. \
         IPTelly closes now and the data is deleted the next time it starts.",
        "Confirm delete",
        |window| {
            // Exits the process on success.
            if let Err(e) = utils::create_nuke_request() {
                crate::show_error(window, &e);
            }
        },
    );
}

/// Fills the page with the stored settings and sources. Called each time
/// the Settings rail item is selected.
pub fn show(window: &AppWindow) {
    crate::spawn(
        crate::blocking(|| {
            Ok((
                settings::get_settings()?,
                settings::has_adult_pin()?,
                utils::get_network_interfaces().unwrap_or_default(),
            ))
        }),
        |window, result| match result {
            Ok((settings, pin_set, interfaces)) => {
                show_settings(window, &settings);
                window.global::<SettingsState>().set_pin_set(pin_set);
                with_page(|page| page.settings = Some(settings));
                show_interfaces(window, interfaces);
            }
            Err(e) => crate::show_error(window, &e),
        },
    );
    let state = window.global::<SettingsState>();
    state.set_editing(-1);
    state.set_new_pin("".into());
    state.set_confirm_pin("".into());
    state.set_pin_error("".into());
    reload_sources();
}

fn show_settings(window: &AppWindow, s: &Settings) {
    let state = window.global::<SettingsState>();
    state.set_recording_path(s.recording_path.clone().unwrap_or_default().into());
    state.set_mpv_params(s.mpv_params.clone().unwrap_or_default().into());
    state.set_vlc_params(s.vlc_params.clone().unwrap_or_default().into());
    state.set_theme((s.theme.as_deref() == Some("classic")) as i32);
    state.set_zoom(i32::from(
        s.zoom
            .unwrap_or(100)
            .clamp(crate::zoom::MIN, crate::zoom::MAX),
    ));
    state.set_player((s.player.as_deref() == Some("vlc")) as i32);
    state.set_default_view(s.default_view.unwrap_or(1).min(4) as i32);
    state.set_default_sort(s.default_sort.unwrap_or(sort_type::PROVIDER) as i32);
    state.set_volume(s.volume.unwrap_or(100) as i32);
    state.set_restream_port(s.restream_port.unwrap_or(3000).to_string().into());
    state.set_stream_caching(s.use_stream_caching.unwrap_or(true));
    state.set_hwdec(s.enable_hwdec.unwrap_or(true));
    state.set_gpu(s.enable_gpu.unwrap_or(false));
    state.set_always_ask_save(s.always_ask_save.unwrap_or(false));
    state.set_refresh_on_start(s.refresh_on_start.unwrap_or(false));
    state.set_tray(s.enable_tray_icon.unwrap_or(true));
    state.set_lightweight(s.lightweight_mode.unwrap_or(false));
}

fn show_interfaces(window: &AppWindow, interfaces: Vec<NetworkInterface>) {
    let state = window.global::<SettingsState>();
    let saved = with_page(|page| {
        page.settings
            .as_ref()
            .and_then(|s| s.network_interface.clone())
    });
    let mut labels: Vec<SharedString> = vec!["System default".into()];
    let mut ips = vec![String::new()];
    for interface in interfaces {
        labels.push(format!("{} ({})", interface.name, interface.ip).into());
        ips.push(interface.ip);
    }
    let mut warning = String::new();
    if let Some(saved) = saved.filter(|ip| !ips.contains(ip)) {
        // Keep the saved address selectable, so saving other settings
        // doesn't silently switch it back to the system default.
        warning = format!("\"{saved}\" was not found. Is it connected?");
        labels.push(format!("{saved} (not found)").into());
        ips.push(saved);
    }
    let selected = with_page(|page| {
        let saved = page
            .settings
            .as_ref()
            .and_then(|s| s.network_interface.as_deref());
        page.interface_ips = ips.clone();
        ips.iter()
            .position(|ip| Some(ip.as_str()) == saved)
            .unwrap_or(0)
    });
    state.set_interfaces(ModelRc::new(VecModel::from(labels)));
    state.set_interface(selected as i32);
    state.set_interface_warning(warning.into());
}

fn optional(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn save(window: &AppWindow) {
    let state = window.global::<SettingsState>();
    let Some(mut s) = with_page(|page| page.settings.clone()) else {
        return;
    };
    s.recording_path = optional(&state.get_recording_path());
    s.mpv_params = optional(&state.get_mpv_params());
    s.vlc_params = optional(&state.get_vlc_params());
    s.zoom = u16::try_from(state.get_zoom()).ok().or(s.zoom);
    s.theme = Some(
        if state.get_theme() == 1 {
            "classic"
        } else {
            "modern"
        }
        .into(),
    );
    if !state.get_is_flatpak() {
        s.player = Some(
            if state.get_player() == 1 {
                "vlc"
            } else {
                "mpv"
            }
            .into(),
        );
    }
    s.default_view = Some(state.get_default_view() as u8);
    s.default_sort = Some(state.get_default_sort() as u8);
    s.volume = Some(state.get_volume().clamp(0, 100) as u8);
    s.restream_port = state
        .get_restream_port()
        .trim()
        .parse()
        .ok()
        .or(s.restream_port);
    s.network_interface = with_page(|page| {
        page.interface_ips
            .get(state.get_interface() as usize)
            .cloned()
            .filter(|ip| !ip.is_empty())
    });
    s.use_stream_caching = Some(state.get_stream_caching());
    s.enable_hwdec = Some(state.get_hwdec());
    s.enable_gpu = Some(state.get_gpu());
    s.always_ask_save = Some(state.get_always_ask_save());
    s.refresh_on_start = Some(state.get_refresh_on_start());
    if state.get_has_tray() {
        s.enable_tray_icon = Some(state.get_tray());
    }
    s.lightweight_mode = Some(state.get_lightweight());

    with_page(|page| page.settings = Some(s.clone()));
    window
        .global::<crate::Theme>()
        .set_classic(s.theme.as_deref() == Some("classic"));
    crate::zoom::set(s.zoom.unwrap_or(100));
    crate::tray::set(s.enable_tray_icon.unwrap_or(true));
    crate::home::set_settings(s.clone());
    crate::spawn(
        crate::blocking(move || settings::update_settings(s)),
        |window, result| {
            if let Err(e) = result {
                crate::show_error(window, &e.context("Failed to save settings"));
            }
        },
    );
}

fn set_pin(window: &AppWindow) {
    let state = window.global::<SettingsState>();
    let pin = state.get_new_pin().to_string();
    let error = if pin.len() < 4 || !pin.chars().all(|c| c.is_ascii_digit()) {
        "PIN must be at least 4 digits."
    } else if pin != state.get_confirm_pin().as_str() {
        "PINs don't match."
    } else {
        ""
    };
    state.set_pin_error(error.into());
    if !error.is_empty() {
        return;
    }
    crate::spawn(
        crate::blocking(move || settings::set_adult_pin(Some(pin))),
        |window, result| match result {
            Ok(()) => {
                let state = window.global::<SettingsState>();
                state.set_pin_set(true);
                state.set_new_pin("".into());
                state.set_confirm_pin("".into());
                crate::home::set_adult_pin(window, true, false);
                crate::show_toast(window, "PIN set");
            }
            Err(e) => crate::show_error(window, &e),
        },
    );
}

/// Runs a long operation with the page's buttons disabled, then toasts how
/// it went and reloads the sources (their "Refreshed" times may change).
fn run<F>(start_message: &'static str, future: F, success: &'static str, failure: &'static str)
where
    F: Future<Output = Result<()>> + Send + 'static,
{
    if let Some(window) = crate::window() {
        window.global::<SettingsState>().set_busy(true);
        if !start_message.is_empty() {
            crate::show_toast(&window, start_message);
        }
    }
    crate::spawn(future, move |window, result| {
        window.global::<SettingsState>().set_busy(false);
        match result {
            Ok(()) => crate::show_toast(window, success),
            Err(e) => crate::show_error(window, &e.context(failure)),
        }
        reload_sources();
        crate::home::reload_sources();
    });
}

/// Reloads the source tiles, then the Xtream expiry dates and timezones
/// (which come from each provider, so they arrive later).
pub fn reload_sources() {
    crate::spawn(crate::blocking(sql::get_sources), |window, result| {
        let sources = match result {
            Ok(sources) => sources,
            Err(e) => {
                crate::show_error(window, &e);
                return;
            }
        };
        let has_xtream = sources.iter().any(|s| s.source_type == source_type::XTREAM);
        with_page(|page| page.sources = sources);
        refresh_tiles(window);
        if has_xtream {
            crate::spawn(
                async {
                    Ok((
                        xtream::get_all_expiries().await?,
                        xtream::get_all_timezones().await?,
                    ))
                },
                |window, result| {
                    // Not worth an error toast: the tiles just show no expiry.
                    if let Ok((expiries, timezones)) = result {
                        with_page(|page| {
                            page.expiries = expiries;
                            page.timezones = timezones;
                        });
                        refresh_tiles(window);
                    }
                },
            );
        }
    });
}

fn refresh_tiles(window: &AppWindow) {
    let now = Local::now().timestamp();
    let items: Vec<SourceItem> = with_page(|page| {
        page.sources
            .iter()
            .map(|s| {
                let id = s.id.unwrap_or_default();
                SourceItem {
                    name: s.name.as_str().into(),
                    source_type: s.source_type as i32,
                    type_name: type_name(s.source_type).into(),
                    url: s.url.clone().unwrap_or_default().into(),
                    user_agent: s.user_agent.clone().unwrap_or_default().into(),
                    stream_user_agent: s.stream_user_agent.clone().unwrap_or_default().into(),
                    max_streams: s
                        .max_streams
                        .map(|n| n.to_string())
                        .unwrap_or_default()
                        .into(),
                    username: s.username.clone().unwrap_or_default().into(),
                    password: s.password.clone().unwrap_or_default().into(),
                    expires: page
                        .expiries
                        .get(&id)
                        .map(|t| time_until(*t, now))
                        .unwrap_or_default()
                        .into(),
                    timezone: page.timezones.get(&id).cloned().unwrap_or_default().into(),
                    epg_url: s.epg_url.clone().unwrap_or_default().into(),
                    epg_retention: s
                        .epg_retention_days
                        .map(|d| d.to_string())
                        .unwrap_or_default()
                        .into(),
                    use_tvg_id: s.use_tvg_id == Some(true),
                    refreshed: s
                        .last_updated
                        .map(|t| time_ago(t, now))
                        .unwrap_or_else(|| "Never".into())
                        .into(),
                    enabled: s.enabled,
                }
            })
            .collect()
    });
    window
        .global::<SettingsState>()
        .set_sources(ModelRc::new(VecModel::from(items)));
}

fn type_name(source_type: u8) -> &'static str {
    match source_type {
        source_type::M3U => "M3U file",
        source_type::M3U_LINK => "M3U URL",
        source_type::XTREAM => "Xtream",
        _ => "Custom",
    }
}

fn plural(n: i64, unit: &str) -> String {
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
}

fn time_ago(timestamp: i64, now: i64) -> String {
    let seconds = now - timestamp;
    if seconds < 29 {
        return "Just now".into();
    }
    for (size, unit) in [
        (86400, "day"),
        (3600, "hour"),
        (60, "minute"),
        (1, "second"),
    ] {
        if seconds >= size {
            return format!("{} ago", plural(seconds / size, unit));
        }
    }
    "Just now".into()
}

fn time_until(timestamp: i64, now: i64) -> String {
    let date = Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|d| exact_date(d.date_naive()))
        .unwrap_or_default();
    let seconds = timestamp - now;
    if seconds <= 0 {
        return format!("Expired ({date})");
    }
    for (size, unit) in [
        (365 * 86400, "year"),
        (30 * 86400, "month"),
        (7 * 86400, "week"),
        (86400, "day"),
        (3600, "hour"),
    ] {
        if seconds >= size {
            return format!("In {} ({date})", plural(seconds / size, unit));
        }
    }
    format!("In less than an hour ({date})")
}

// "October 4th 2026".
fn exact_date(date: NaiveDate) -> String {
    let day = date.day();
    let suffix = match day {
        11..=13 => "th",
        _ if day % 10 == 1 => "st",
        _ if day % 10 == 2 => "nd",
        _ if day % 10 == 3 => "rd",
        _ => "th",
    };
    format!("{} {day}{suffix} {}", date.format("%B"), date.year())
}

pub async fn save_dialog(title: &str, file_name: &str, extension: &str) -> Option<String> {
    rfd::AsyncFileDialog::new()
        .set_title(title)
        .set_file_name(file_name)
        .add_filter(extension, &[extension])
        .save_file()
        .await
        .map(|f| f.path().to_string_lossy().into_owned())
}

pub async fn open_dialog(title: &str, extensions: &[&str]) -> Option<String> {
    rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter(extensions.join(", "), extensions)
        .pick_file()
        .await
        .map(|f| f.path().to_string_lossy().into_owned())
}

fn source_action(window: &AppWindow, index: usize, action: SourceAction) {
    let Some(source) = with_page(|page| page.sources.get(index).cloned()) else {
        return;
    };
    let Some(id) = source.id else { return };
    let state = window.global::<SettingsState>();
    let name = source.name.clone();
    match action {
        SourceAction::Toggle => {
            let enabled = !source.enabled;
            run(
                "",
                crate::blocking(move || sql::set_source_enabled(enabled, id)),
                if enabled {
                    "Source enabled"
                } else {
                    "Source disabled"
                },
                "Failed to change the source",
            );
        }
        SourceAction::Edit => {
            state.set_edit_url(source.url.clone().unwrap_or_default().into());
            state.set_edit_user_agent(source.user_agent.clone().unwrap_or_default().into());
            state.set_edit_stream_user_agent(
                source.stream_user_agent.clone().unwrap_or_default().into(),
            );
            state.set_edit_max_streams(
                source
                    .max_streams
                    .map(|n| n.to_string())
                    .unwrap_or_default()
                    .into(),
            );
            state.set_edit_username(source.username.clone().unwrap_or_default().into());
            state.set_edit_password(source.password.clone().unwrap_or_default().into());
            state.set_edit_epg_url(source.epg_url.clone().unwrap_or_default().into());
            state.set_edit_epg_retention(
                source
                    .epg_retention_days
                    .map(|d| d.to_string())
                    .unwrap_or_default()
                    .into(),
            );
            state.set_edit_use_tvg_id(source.use_tvg_id == Some(true));
            state.set_editing(index as i32);
        }
        SourceAction::Cancel => state.set_editing(-1),
        SourceAction::Browse => crate::spawn(
            async { Ok(open_dialog("Select a new m3u file for source", &["m3u", "m3u8"]).await) },
            |window, result| {
                if let Ok(Some(path)) = result {
                    window.global::<SettingsState>().set_edit_url(path.into());
                }
            },
        ),
        SourceAction::Save => {
            let mut edited = source.clone();
            edited.url = optional(&state.get_edit_url());
            edited.user_agent = optional(&state.get_edit_user_agent());
            edited.stream_user_agent = optional(&state.get_edit_stream_user_agent());
            edited.max_streams = state.get_edit_max_streams().trim().parse().ok();
            if source.source_type == source_type::XTREAM {
                edited.username = optional(&state.get_edit_username());
                edited.password = optional(&state.get_edit_password());
            }
            edited.epg_url = optional(&state.get_edit_epg_url());
            edited.epg_retention_days = state
                .get_edit_epg_retention()
                .trim()
                .parse()
                .ok()
                .filter(|d| *d >= 1);
            if matches!(source.source_type, source_type::M3U | source_type::M3U_LINK) {
                edited.use_tvg_id = Some(state.get_edit_use_tvg_id());
            }
            state.set_editing(-1);
            run(
                "",
                crate::blocking(move || sql::update_source(edited)),
                "Source updated",
                "Failed to update the source",
            );
        }
        SourceAction::Refresh => {
            if source.source_type == source_type::XTREAM {
                crate::home::clear_series_cache();
            }
            run(
                "Refreshing...",
                utils::refresh_source(source),
                "Source refreshed",
                "Failed to refresh the source",
            );
        }
        SourceAction::RefreshEpg => {
            let future = async move {
                if source.epg_url.is_some() {
                    xmltv::refresh_epg(source).await
                } else {
                    xtream::refresh_xtream_epg(source).await
                }
            };
            run(
                "Refreshing EPG...",
                future,
                "EPG refreshed",
                "Failed to refresh the EPG",
            );
        }
        SourceAction::RefreshProviderEpg => run(
            "Refreshing provider EPG...",
            xtream::refresh_xtream_epg(source),
            "Provider EPG refreshed",
            "Failed to refresh the provider EPG",
        ),
        SourceAction::PruneEpg => run(
            "",
            crate::blocking(move || xmltv::prune_old_epg(source)),
            "Old EPG data cleared",
            "Failed to clear old EPG data",
        ),
        SourceAction::BackupFavs => {
            let file_name = format!("{}_favs.otvf", file_name::sanitize(&name));
            crate::spawn(
                async move {
                    let Some(path) =
                        save_dialog("Select where to save favorites", &file_name, "otvf").await
                    else {
                        return Ok(false);
                    };
                    crate::blocking(move || utils::backup_favs(id, path)).await?;
                    Ok(true)
                },
                |window, result| match result {
                    Ok(true) => crate::show_toast(window, "Favorites saved"),
                    Ok(false) => {}
                    Err(e) => crate::show_error(window, &e.context("Failed to back up favorites")),
                },
            );
        }
        SourceAction::RestoreFavs => crate::spawn(
            async move {
                let Some(path) = open_dialog("Select a favorites backup", &["otvf"]).await else {
                    return Ok(false);
                };
                crate::blocking(move || utils::restore_favs(id, path)).await?;
                Ok(true)
            },
            |window, result| match result {
                Ok(true) => crate::show_toast(window, "Favorites restored"),
                Ok(false) => {}
                Err(e) => crate::show_error(window, &e.context("Failed to restore favorites")),
            },
        ),
        SourceAction::Share => {
            let file_name = format!("{}.otvp", file_name::sanitize(&name));
            crate::spawn(
                async move {
                    let Some(path) =
                        save_dialog("Select where to export custom source", &file_name, "otvp")
                            .await
                    else {
                        return Ok(false);
                    };
                    crate::blocking(move || share::share_custom_source(source, path)).await?;
                    Ok(true)
                },
                |window, result| match result {
                    Ok(true) => crate::show_toast(window, "Source exported"),
                    Ok(false) => {}
                    Err(e) => crate::show_error(window, &e.context("Failed to export the source")),
                },
            );
        }
        SourceAction::Delete => dialog::confirm(
            window,
            &format!("Delete \"{name}\"?"),
            "This removes the source with all its channels, favourites and history.",
            "Delete",
            move |_| {
                run(
                    "",
                    crate::blocking(move || sql::delete_source(id)),
                    "Source deleted",
                    "Failed to delete the source",
                )
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_790_000_000;
    const MINUTE: i64 = 60;
    const HOUR: i64 = 3600;
    const DAY: i64 = 86400;

    #[test]
    fn time_ago_says_just_now_for_under_29_seconds() {
        assert_eq!(time_ago(NOW, NOW), "Just now");
        assert_eq!(time_ago(NOW - 28, NOW), "Just now");
        // A clock that has gone backwards.
        assert_eq!(time_ago(NOW + 60, NOW), "Just now");
    }

    #[test]
    fn time_ago_counts_seconds_from_29_seconds() {
        assert_eq!(time_ago(NOW - 29, NOW), "29 seconds ago");
        assert_eq!(time_ago(NOW - 59, NOW), "59 seconds ago");
    }

    #[test]
    fn time_ago_uses_the_singular_for_a_count_of_one() {
        assert_eq!(time_ago(NOW - MINUTE, NOW), "1 minute ago");
        assert_eq!(time_ago(NOW - HOUR, NOW), "1 hour ago");
        assert_eq!(time_ago(NOW - DAY, NOW), "1 day ago");
    }

    #[test]
    fn time_ago_uses_the_largest_whole_unit() {
        assert_eq!(time_ago(NOW - 5 * MINUTE - 30, NOW), "5 minutes ago");
        assert_eq!(time_ago(NOW - 2 * HOUR - 59 * MINUTE, NOW), "2 hours ago");
        assert_eq!(time_ago(NOW - 40 * DAY, NOW), "40 days ago");
    }

    #[test]
    fn time_until_marks_past_dates_as_expired() {
        assert!(time_until(NOW - DAY, NOW).starts_with("Expired ("));
        assert!(time_until(NOW, NOW).starts_with("Expired ("));
    }

    #[test]
    fn time_until_says_less_than_an_hour_for_under_an_hour_away() {
        assert!(time_until(NOW + 59 * MINUTE, NOW).starts_with("In less than an hour ("));
    }

    #[test]
    fn time_until_uses_the_singular_for_a_count_of_one() {
        assert!(time_until(NOW + HOUR, NOW).starts_with("In 1 hour ("));
        assert!(time_until(NOW + DAY, NOW).starts_with("In 1 day ("));
        assert!(time_until(NOW + 7 * DAY, NOW).starts_with("In 1 week ("));
        assert!(time_until(NOW + 30 * DAY, NOW).starts_with("In 1 month ("));
        assert!(time_until(NOW + 365 * DAY, NOW).starts_with("In 1 year ("));
    }

    #[test]
    fn time_until_uses_the_largest_whole_unit() {
        assert!(time_until(NOW + 5 * HOUR, NOW).starts_with("In 5 hours ("));
        assert!(time_until(NOW + 3 * DAY, NOW).starts_with("In 3 days ("));
        assert!(time_until(NOW + 20 * DAY, NOW).starts_with("In 2 weeks ("));
        assert!(time_until(NOW + 100 * DAY, NOW).starts_with("In 3 months ("));
        assert!(time_until(NOW + 800 * DAY, NOW).starts_with("In 2 years ("));
    }

    #[test]
    fn time_until_ends_with_the_exact_date() {
        let expiry = Local.with_ymd_and_hms(2027, 3, 2, 12, 0, 0).unwrap();
        let text = time_until(expiry.timestamp(), NOW);
        assert!(text.ends_with("(March 2nd 2027)"), "{text}");
    }

    #[test]
    fn exact_date_uses_the_right_ordinal_suffix_for_each_day() {
        let suffixes = [
            (1, "1st"),
            (2, "2nd"),
            (3, "3rd"),
            (4, "4th"),
            (11, "11th"),
            (12, "12th"),
            (13, "13th"),
            (20, "20th"),
            (21, "21st"),
            (22, "22nd"),
            (23, "23rd"),
            (24, "24th"),
            (30, "30th"),
            (31, "31st"),
        ];
        for (day, expected) in suffixes {
            let date = NaiveDate::from_ymd_opt(2026, 1, day).unwrap();
            assert_eq!(exact_date(date), format!("January {expected} 2026"));
        }
    }
}
