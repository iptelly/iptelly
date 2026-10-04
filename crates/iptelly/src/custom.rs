// Custom sources: adding, editing, sharing and deleting their channels and
// categories, and importing shared ones (ui/custom.slint).

use std::cell::RefCell;
use std::time::Duration;

use anyhow::Result;
use iptelly_core::types::{Channel, ChannelHttpHeaders, CustomChannel, Group};
use iptelly_core::{api, media_type, share, sql};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::settings_page::{open_dialog, save_dialog};
use crate::{AppWindow, CustomKind, CustomState, dialog};

const CHECK_DELAY: Duration = Duration::from_millis(300);

#[derive(Default)]
struct Custom {
    source_id: i64,
    // The channel or category being edited or deleted; None when adding.
    original: Option<Channel>,
    // The category id behind each entry of CustomState.groups.
    group_ids: Vec<Option<i64>>,
    check_timer: Timer,
    // Bumped on every check, so a slow answer can't overwrite a newer one.
    check_token: u64,
}

thread_local! {
    static CUSTOM: RefCell<Custom> = RefCell::new(Custom::default());
}

fn with_custom<R>(f: impl FnOnce(&mut Custom) -> R) -> R {
    CUSTOM.with_borrow_mut(f)
}

pub fn setup(window: &AppWindow) {
    let state = window.global::<CustomState>();
    state.on_name_edited(check_exists);
    state.on_url_edited(check_exists);

    let weak = window.as_weak();
    state.on_save(move || {
        if let Some(window) = weak.upgrade() {
            save(&window);
        }
    });

    let weak = window.as_weak();
    state.on_close(move || {
        if let Some(window) = weak.upgrade() {
            close(&window);
        }
    });
}

pub fn add_channel(window: &AppWindow, source_id: i64) {
    open(
        window,
        CustomKind::Channel,
        "Add a custom channel",
        source_id,
        None,
    );
    load_groups(source_id, None, None);
}

pub fn add_group(window: &AppWindow, source_id: i64) {
    open(
        window,
        CustomKind::Group,
        "Add a custom category",
        source_id,
        None,
    );
}

pub fn import(window: &AppWindow, source_id: i64) {
    open(
        window,
        CustomKind::Import,
        "Import a channel or category",
        source_id,
        None,
    );
}

/// Opens the channel or category editor.
pub fn edit(window: &AppWindow, channel: Channel) {
    let Some(source_id) = channel.source_id else {
        return;
    };
    let title = format!("Editing {}", channel.name);
    let state = window.global::<CustomState>();
    if channel.media_type == media_type::GROUP {
        open(
            window,
            CustomKind::Group,
            &title,
            source_id,
            Some(channel.clone()),
        );
        state.set_name(channel.name.as_str().into());
        state.set_image(channel.image.clone().unwrap_or_default().into());
        return;
    }
    open(
        window,
        CustomKind::Channel,
        &title,
        source_id,
        Some(channel.clone()),
    );
    state.set_name(channel.name.as_str().into());
    state.set_image(channel.image.clone().unwrap_or_default().into());
    state.set_url(channel.url.clone().unwrap_or_default().into());
    state.set_media_type(i32::from(channel.media_type == media_type::MOVIE));
    load_groups(source_id, channel.group_id, None);
    let Some(id) = channel.id else { return };
    let group_id = channel.group_id;
    crate::spawn(
        crate::blocking(move || sql::get_custom_channel_extra_data(id, group_id)),
        |window, result| {
            let headers = match result {
                Ok(data) => data.headers.unwrap_or_default(),
                Err(e) => return crate::show_error(window, &e),
            };
            let state = window.global::<CustomState>();
            state.set_user_agent(headers.user_agent.unwrap_or_default().into());
            state.set_origin(headers.http_origin.unwrap_or_default().into());
            state.set_referrer(headers.referrer.unwrap_or_default().into());
            state.set_ignore_ssl(headers.ignore_ssl == Some(true));
        },
    );
}

/// Exports a channel (.otv) or category (.otvg) for someone else to import.
pub fn share(channel: Channel) {
    let group = channel.media_type == media_type::GROUP;
    let extension = if group { "otvg" } else { "otv" };
    let file_name = format!("{}.{extension}", crate::file_name::sanitize(&channel.name));
    let title = if group {
        "Select where to export category"
    } else {
        "Select where to export channel"
    };
    crate::spawn(
        async move {
            let Some(path) = save_dialog(title, &file_name, extension).await else {
                return Ok(None);
            };
            let shown = path.clone();
            crate::blocking(move || {
                if group {
                    share::share_custom_group(channel, path)
                } else {
                    share::share_custom_channel(channel, path)
                }
            })
            .await?;
            Ok(Some(shown))
        },
        |window, result| match result {
            Ok(Some(path)) => crate::show_toast(window, &format!("Exported to {path}")),
            Ok(None) => {}
            Err(e) => crate::show_error(window, &e.context("Failed to export")),
        },
    );
}

/// Deletes a channel, or a category. A category with channels in it first
/// asks where its channels should go.
pub fn delete(window: &AppWindow, channel: Channel) {
    let Some(id) = channel.id else { return };
    let name = channel.name.clone();
    if channel.media_type != media_type::GROUP {
        dialog::confirm(
            window,
            &format!("Delete \"{name}\"?"),
            "This removes the channel from its custom source.",
            "Delete",
            move |_| {
                finish(
                    crate::blocking(move || sql::delete_custom_channel(id)),
                    "Channel deleted",
                )
            },
        );
        return;
    }
    let Some(source_id) = channel.source_id else {
        return;
    };
    crate::spawn(
        crate::blocking(move || sql::group_not_empty(id)),
        move |window, result| match result {
            Ok(true) => {
                open(
                    window,
                    CustomKind::DeleteGroup,
                    &format!("Delete \"{name}\""),
                    source_id,
                    Some(channel.clone()),
                );
                load_groups(source_id, None, Some(id));
            }
            Ok(false) => dialog::confirm(
                window,
                &format!("Delete \"{name}\"?"),
                "This category is empty.",
                "Delete",
                move |_| {
                    finish(
                        crate::blocking(move || sql::delete_custom_group(id, None, false)),
                        "Category deleted",
                    )
                },
            ),
            Err(e) => crate::show_error(window, &e),
        },
    );
}

fn open(
    window: &AppWindow,
    kind: CustomKind,
    title: &str,
    source_id: i64,
    original: Option<Channel>,
) {
    let state = window.global::<CustomState>();
    state.set_title(title.into());
    state.set_editing(original.is_some() && kind != CustomKind::DeleteGroup);
    for set in [
        CustomState::set_name,
        CustomState::set_image,
        CustomState::set_url,
        CustomState::set_user_agent,
        CustomState::set_origin,
        CustomState::set_referrer,
    ] {
        set(&state, SharedString::new());
    }
    state.set_media_type(0);
    state.set_groups(ModelRc::new(VecModel::from(vec![SharedString::from(
        "(None)",
    )])));
    state.set_group(0);
    state.set_ignore_ssl(false);
    state.set_exists(false);
    state.set_busy(false);
    state.set_kind(kind);
    with_custom(|custom| {
        custom.source_id = source_id;
        custom.original = original;
        custom.group_ids = vec![None];
        custom.check_timer.stop();
        custom.check_token += 1;
    });
}

fn close(window: &AppWindow) {
    window.global::<CustomState>().set_kind(CustomKind::None);
    with_custom(|custom| {
        custom.original = None;
        custom.check_timer.stop();
        custom.check_token += 1;
    });
}

/// Fills the category picker with the source's categories, "(None)" first,
/// selecting `selected` and leaving out `exclude`.
fn load_groups(source_id: i64, selected: Option<i64>, exclude: Option<i64>) {
    crate::spawn(
        crate::blocking(move || sql::group_auto_complete(None, source_id)),
        move |window, result| {
            let groups = match result {
                Ok(groups) => groups,
                Err(e) => {
                    return crate::show_error(window, &e.context("Failed to load categories"));
                }
            };
            let groups: Vec<_> = groups
                .into_iter()
                .filter(|g| Some(g.id) != exclude)
                .collect();
            let mut names = vec![SharedString::from("(None)")];
            let mut ids = vec![None];
            for group in groups {
                names.push(group.name.into());
                ids.push(Some(group.id));
            }
            let index = ids.iter().position(|id| *id == selected && id.is_some());
            let state = window.global::<CustomState>();
            state.set_groups(ModelRc::new(VecModel::from(names)));
            state.set_group(index.unwrap_or(0) as i32);
            with_custom(|custom| custom.group_ids = ids);
        },
    );
}

/// Checks, after a pause in typing, whether the channel (name and URL) or
/// category name is already in the source.
fn check_exists() {
    let Some(window) = crate::window() else {
        return;
    };
    let state = window.global::<CustomState>();
    state.set_exists(false);
    let kind = state.get_kind();
    let name = state.get_name().trim().to_string();
    let url = state.get_url().trim().to_string();
    let (source_id, original, token) = with_custom(|custom| {
        custom.check_token += 1;
        (
            custom.source_id,
            custom.original.clone(),
            custom.check_token,
        )
    });
    if !needs_check(kind, &name, &url, original.as_ref()) {
        return;
    }
    with_custom(|custom| {
        custom
            .check_timer
            .start(TimerMode::SingleShot, CHECK_DELAY, move || {
                let (name, url) = (name.clone(), url.clone());
                crate::spawn(
                    crate::blocking(move || match kind {
                        CustomKind::Channel => sql::channel_exists(&name, &url, source_id),
                        CustomKind::Group => sql::group_exists(&name, source_id),
                        _ => Ok(false),
                    }),
                    move |window, result| {
                        if with_custom(|custom| custom.check_token) != token {
                            return;
                        }
                        match result {
                            Ok(exists) => window.global::<CustomState>().set_exists(exists),
                            Err(e) => crate::show_error(window, &e),
                        }
                    },
                )
            })
    });
}

// Whether to ask the database if the trimmed name (and URL) is taken: not
// while a required field is blank, nor when editing leaves them unchanged.
fn needs_check(kind: CustomKind, name: &str, url: &str, original: Option<&Channel>) -> bool {
    let unchanged = original.is_some_and(|o| {
        o.name == name && (kind == CustomKind::Group || o.url.as_deref() == Some(url))
    });
    !(name.is_empty() || (kind == CustomKind::Channel && url.is_empty()) || unchanged)
}

fn save(window: &AppWindow) {
    let state = window.global::<CustomState>();
    let (source_id, original, group_ids) = with_custom(|custom| {
        (
            custom.source_id,
            custom.original.clone(),
            custom.group_ids.clone(),
        )
    });
    let group_id = group_ids.get(state.get_group() as usize).copied().flatten();
    let name = state.get_name().trim().to_string();
    let image = optional(&state.get_image());
    state.set_busy(true);
    match state.get_kind() {
        CustomKind::Channel => {
            let headers = ChannelHttpHeaders {
                id: None,
                channel_id: None,
                referrer: optional(&state.get_referrer()),
                user_agent: optional(&state.get_user_agent()),
                http_origin: optional(&state.get_origin()),
                ignore_ssl: Some(state.get_ignore_ssl()),
            };
            let empty = headers.referrer.is_none()
                && headers.user_agent.is_none()
                && headers.http_origin.is_none()
                && !state.get_ignore_ssl();
            let editing = original.is_some();
            let mut data = original.unwrap_or_else(|| new_channel(source_id));
            data.name = name;
            data.image = image;
            data.url = optional(&state.get_url());
            data.media_type = if state.get_media_type() == 1 {
                media_type::MOVIE
            } else {
                media_type::LIVESTREAM
            };
            data.group_id = group_id;
            let channel = CustomChannel {
                data,
                headers: (!empty).then_some(headers),
            };
            if editing {
                finish(
                    crate::blocking(move || sql::edit_custom_channel(channel)),
                    "Channel updated",
                );
            } else {
                finish(
                    crate::blocking(move || api::add_custom_channel(channel)),
                    "Channel added",
                );
            }
        }
        CustomKind::Group => {
            let group = Group {
                id: original.as_ref().and_then(|o| o.id),
                name,
                image,
                source_id: Some(source_id),
                hidden: None,
                media_type: None,
            };
            if original.is_some() {
                finish(
                    crate::blocking(move || sql::edit_custom_group(group)),
                    "Category updated",
                );
            } else {
                finish(
                    crate::blocking(move || api::add_custom_group(group)),
                    "Category added",
                );
            }
        }
        CustomKind::DeleteGroup => {
            let Some(id) = original.and_then(|o| o.id) else {
                return;
            };
            finish(
                crate::blocking(move || sql::delete_custom_group(id, group_id, true)),
                "Category deleted",
            );
        }
        CustomKind::Import => {
            let name = (!name.is_empty()).then_some(name);
            crate::spawn(
                async move {
                    let Some(path) =
                        open_dialog("Select an IPTelly export file", &["otv", "otvg"]).await
                    else {
                        return Ok(false);
                    };
                    crate::blocking(move || share::import(path, Some(source_id), name)).await?;
                    Ok(true)
                },
                |window, result| match result {
                    Ok(true) => done(window, "Import finished"),
                    Ok(false) => window.global::<CustomState>().set_busy(false),
                    Err(e) => failed(window, &e.context("Failed to import the file")),
                },
            );
        }
        CustomKind::None => {}
    }
}

/// Runs a change, then closes the dialog and refreshes the home screen, or
/// shows the error and leaves the dialog open.
fn finish(future: impl Future<Output = Result<()>> + Send + 'static, success: &'static str) {
    crate::spawn(future, move |window, result| match result {
        Ok(()) => done(window, success),
        Err(e) => failed(window, &e),
    });
}

fn done(window: &AppWindow, message: &str) {
    close(window);
    crate::show_toast(window, message);
    crate::home::refresh(window);
}

fn failed(window: &AppWindow, error: &anyhow::Error) {
    window.global::<CustomState>().set_busy(false);
    crate::show_error(window, error);
}

fn new_channel(source_id: i64) -> Channel {
    Channel {
        id: None,
        name: String::new(),
        url: None,
        group: None,
        image: None,
        media_type: media_type::LIVESTREAM,
        source_id: Some(source_id),
        series_id: None,
        group_id: None,
        favorite: false,
        stream_id: None,
        tv_archive: None,
        season_id: None,
        episode_num: None,
        hidden: None,
        tvg_id: None,
        is_adult: false,
    }
}

fn optional(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "http://example.com/1.ts";

    fn channel(name: &str, url: Option<&str>) -> Channel {
        Channel {
            name: name.into(),
            url: url.map(Into::into),
            ..Default::default()
        }
    }

    #[test]
    fn skips_the_check_while_a_required_field_is_blank() {
        assert!(!needs_check(CustomKind::Group, "", "", None));
        assert!(!needs_check(CustomKind::Channel, "", URL, None));
        assert!(!needs_check(CustomKind::Channel, "News", "", None));
    }

    #[test]
    fn checks_a_new_channel_or_category() {
        assert!(needs_check(CustomKind::Group, "Sport", "", None));
        assert!(needs_check(CustomKind::Channel, "News", URL, None));
    }

    #[test]
    fn skips_the_check_when_editing_leaves_the_name_unchanged() {
        let group = channel("Sport", None);
        assert!(!needs_check(CustomKind::Group, "Sport", "", Some(&group)));
        assert!(needs_check(CustomKind::Group, "Sports", "", Some(&group)));
    }

    #[test]
    fn checks_an_edited_channel_when_its_name_or_url_changes() {
        let news = channel("News", Some(URL));
        assert!(!needs_check(CustomKind::Channel, "News", URL, Some(&news)));
        assert!(needs_check(
            CustomKind::Channel,
            "News 24",
            URL,
            Some(&news)
        ));
        let other = "http://example.com/2.ts";
        assert!(needs_check(CustomKind::Channel, "News", other, Some(&news)));
    }

    #[test]
    fn optional_trims_and_drops_blank_text() {
        assert_eq!(optional("  a b  "), Some("a b".into()));
        assert_eq!(optional("   "), None);
        assert_eq!(optional(""), None);
    }
}
