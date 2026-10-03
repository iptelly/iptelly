// Adding a source. Ported from src/app/setup/setup.component.ts.

use std::cell::RefCell;
use std::time::Duration;

use anyhow::{Context, Result};
use iptelly_core::types::Source;
use iptelly_core::{api, m3u, settings, source_type, sql, xtream};
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
    Source {
        id: None,
        name: state.get_name().trim().to_string(),
        url: if has_url {
            optional(&state.get_url())
        } else {
            None
        },
        url_origin: None,
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
    }
}

fn submit(window: &AppWindow) {
    let state = window.global::<SetupState>();
    let mut source = source_from_form(&state);

    if source.source_type == source_type::XTREAM {
        if let Some(url) = source.url.as_mut() {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                *url = format!("http://{url}");
                crate::show_toast(
                    window,
                    "Since the given URL lacked a protocol, http was assumed",
                );
            }
        }
    }

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
                _ => crate::blocking(move || api::add_custom_source(source.name)).await?,
            }
            Ok(true)
        },
        move |window, result: Result<bool>| {
            window.global::<SetupState>().set_loading(false);
            match result {
                Ok(false) => {}
                Ok(true) => {
                    crate::show_toast(window, &format!("\"{name}\" successfully added"));
                    clear_form(window);
                    open_home();
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

fn open_home() {
    crate::spawn(
        crate::blocking(|| Ok((settings::get_settings()?, sql::get_sources()?))),
        |window, result| match result {
            Ok((settings, sources)) => crate::home::start(window, settings, sources),
            Err(e) => crate::show_error(window, &e),
        },
    );
}
