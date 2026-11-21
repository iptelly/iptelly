// Adding a source.

use std::cell::RefCell;
use std::time::Duration;

use anyhow::{Context, Result};
use iptelly_core::types::Source;
use iptelly_core::{api, app_data, m3u, share, source_type, sql, xtream};
use slint::{ComponentHandle, Timer, TimerMode};

use crate::{AppWindow, Page, SetupState};

thread_local! {
    static NAME_CHECK: RefCell<Timer> = RefCell::new(Timer::default());
}

pub fn setup(window: &AppWindow) {
    let state = window.global::<SetupState>();

    let weak = window.as_weak();
    state.on_name_edited(move |name| {
        let weak = weak.clone();
        let name = name.trim().to_string();
        NAME_CHECK.with_borrow(|timer| {
            timer.start(
                TimerMode::SingleShot,
                Duration::from_millis(300),
                move || {
                    let name = name.clone();
                    if name.is_empty() {
                        if let Some(window) = weak.upgrade() {
                            window.global::<SetupState>().set_name_taken(false);
                        }
                        return;
                    }
                    let checked = name.clone();
                    crate::spawn(
                        crate::blocking(move || sql::source_name_exists(&checked)),
                        move |window, result| {
                            let state = window.global::<SetupState>();
                            // Ignore the answer if the name has changed since.
                            if state.get_name().trim() == name {
                                state.set_name_taken(result.unwrap_or(false));
                            }
                        },
                    );
                },
            )
        });
    });

    let weak = window.as_weak();
    state.on_submit(move || {
        if let Some(window) = weak.upgrade() {
            submit(&window);
        }
    });

    let weak = window.as_weak();
    state.on_back(move || {
        if let Some(window) = weak.upgrade() {
            window.set_page(Page::Home);
        }
    });

    state.on_import_backup(|| {
        crate::spawn(
            async {
                let Some(path) = rfd::AsyncFileDialog::new()
                    .set_title("Select an exported data file")
                    .add_filter("IPTelly backup", &["otva"])
                    .pick_file()
                    .await
                else {
                    return Ok(false);
                };
                let path = path.path().to_string_lossy().into_owned();
                crate::blocking(move || app_data::import_app_data(path)).await?;
                Ok(true)
            },
            |window, result| match result {
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
            crate::settings_page::confirm_delete_everything(&window);
        }
    });
}

pub fn show(window: &AppWindow, can_go_back: bool) {
    let state = window.global::<SetupState>();
    state.set_can_go_back(can_go_back);
    state.set_loading(false);
    window.set_page(Page::Setup);
}

fn optional(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn source_from_form(state: &SetupState) -> Source {
    let source_type = state.get_source_type() as u8;
    let has_login = source_type == source_type::XTREAM;
    let has_url = matches!(source_type, source_type::M3U_LINK | source_type::XTREAM);
    let url = |text: slint::SharedString| if has_url { optional(&text) } else { None };
    Source {
        id: None,
        name: state.get_name().trim().to_string(),
        url: url(state.get_url()),
        url_2: url(state.get_url_2()),
        url_3: url(state.get_url_3()),
        username: if has_login {
            optional(&state.get_username())
        } else {
            None
        },
        password: if has_login {
            optional(&state.get_password())
        } else {
            None
        },
        source_type,
        use_tvg_id: matches!(source_type, source_type::M3U | source_type::M3U_LINK)
            .then(|| state.get_use_tvg_id()),
        enabled: true,
        user_agent: if has_url {
            optional(&state.get_user_agent())
        } else {
            None
        },
        max_streams: None,
        stream_user_agent: None,
        last_updated: None,
        epg_url: if source_type == source_type::CUSTOM {
            None
        } else {
            optional(&state.get_epg_url())
        },
        timezone: None,
        epg_retention_days: None,
        output_format: None,
        include_live: None,
        include_vod: None,
    }
}

fn submit(window: &AppWindow) {
    let state = window.global::<SetupState>();
    let mut source = source_from_form(&state);
    if source.source_type == source_type::XTREAM {
        tidy_servers(&mut source);
    }
    add_source(window, source);
}

/// Keeps just the server of each of an Xtream source's URLs
/// ("http://host:port"): people often paste the whole API URL, and the
/// API path is added when it's called. Text that isn't a URL is left for
/// the refresh to report.
pub fn tidy_servers(source: &mut Source) {
    for url in [&mut source.url, &mut source.url_2, &mut source.url_3] {
        if let Some(server) = url.as_deref().and_then(xtream::server_address) {
            *url = Some(server);
        }
    }
}

fn add_source(window: &AppWindow, mut source: Source) {
    let state = window.global::<SetupState>();
    let name_override = optional(&state.get_name());
    state.set_loading(true);
    let name = source.name.clone();
    crate::spawn(
        async move {
            match source.source_type {
                source_type::M3U => {
                    let Some(path) = rfd::AsyncFileDialog::new()
                        .set_title("Select an m3u file")
                        .add_filter("Playlist", &["m3u", "m3u8"])
                        .pick_file()
                        .await
                    else {
                        return Ok(false);
                    };
                    source.url = Some(path.path().to_string_lossy().into_owned());
                    crate::blocking(move || m3u::read_m3u8(source, false))
                        .await
                        .context("Could not parse selected file")?;
                }
                source_type::M3U_LINK => m3u::get_m3u8_from_link(source, false).await?,
                source_type::XTREAM => xtream::get_xtream(source, false).await?,
                source_type::CUSTOM => {
                    crate::blocking(move || api::add_custom_source(source.name)).await?
                }
                // Custom import: a custom source someone shared as .otvp.
                _ => {
                    let Some(path) = rfd::AsyncFileDialog::new()
                        .set_title("Select IPTelly export file (.otvp)")
                        .add_filter("IPTelly playlist", &["otvp"])
                        .pick_file()
                        .await
                    else {
                        return Ok(false);
                    };
                    let path = path.path().to_string_lossy().into_owned();
                    crate::blocking(move || share::import(path, None, name_override)).await?;
                }
            }
            Ok(true)
        },
        move |window, result: Result<bool>| {
            window.global::<SetupState>().set_loading(false);
            match result {
                Ok(false) => {}
                Ok(true) => {
                    let message = if name.is_empty() {
                        "Source successfully imported".to_string()
                    } else {
                        format!("\"{name}\" successfully added")
                    };
                    crate::show_toast(window, &message);
                    clear_form(window);
                    crate::start();
                }
                Err(e) => crate::show_error(window, &e),
            }
        },
    );
}

fn clear_form(window: &AppWindow) {
    let state = window.global::<SetupState>();
    for set in [
        SetupState::set_name,
        SetupState::set_url,
        SetupState::set_url_2,
        SetupState::set_url_3,
        SetupState::set_user_agent,
        SetupState::set_epg_url,
        SetupState::set_username,
        SetupState::set_password,
    ] {
        set(&state, "".into());
    }
    state.set_use_tvg_id(false);
    state.set_name_taken(false);
}
