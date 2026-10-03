// The home screen: nav rail, playlist sidebar, search and the channel grid.
// Ported from src/app/home/home.component.ts and its children.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use iptelly_core::types::{Channel, Filters, Settings, Source};
use iptelly_core::{
    api, epg, media_type, mpv, settings, sort_type, source_type, sql, utils, view_type, xtream,
};
use slint::{ComponentHandle, Image, Model, ModelRc, Timer, TimerMode, VecModel};

use crate::{AppWindow, ChannelItem, HomeState, Page, Rail, STATE, SidebarRow};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);
// Decoded images are kept for the session so scrolling back doesn't
// re-decode them. Past this many the cache is simply emptied.
const IMAGE_CACHE_LIMIT: usize = 600;

#[derive(Default)]
struct Home {
    sources: Vec<Source>,
    settings: Option<Settings>,
    filters: Option<Filters>,
    channels: Vec<Channel>,
    model: Rc<VecModel<ChannelItem>>,
    reached_max: bool,
    loading: bool,
    // Bumped on every fresh load, so a slow response to an older search
    // can't overwrite a newer one.
    load_token: u64,
    // Drill-down stack (category, series, season): the filters to restore
    // on going back.
    nodes: Vec<Node>,
    search_timer: Timer,
    sidebar: Sidebar,
    images: HashMap<String, Image>,
    images_pending: HashSet<String>,
    // Series whose episodes were already fetched from the provider this run.
    series_refreshed: HashSet<i64>,
    // Channel ids whose player is starting; clicking again cancels.
    starting: HashSet<i64>,
    // Favourites' second section: favourite movies and series, below the
    // favourite channels.
    fav: FavMedia,
}

#[derive(Default)]
struct FavMedia {
    filters: Option<Filters>,
    channels: Vec<Channel>,
    model: Rc<VecModel<ChannelItem>>,
    reached_max: bool,
    loading: bool,
    load_token: u64,
}

/// Tile indexes from here on are rows of the Favourites movies and series
/// section (FAV + row), so tile callbacks work the same on both grids.
const FAV: usize = 1 << 20;

struct Node {
    filters: Filters,
    search_text: String,
    // Category nodes are left via the sidebar, so only series and seasons
    // get a back row (as in the Angular app).
    title: Option<String>,
}

#[derive(Default)]
struct Sidebar {
    // Media type the categories were loaded for.
    media_type: Option<u8>,
    expanded: HashSet<i64>,
    categories: HashMap<i64, Vec<Channel>>,
    loading: HashSet<i64>,
    model: Rc<VecModel<SidebarRow>>,
    // What each row of `model` stands for.
    actions: Vec<SidebarAction>,
}

#[derive(Clone, Copy)]
enum SidebarAction {
    AllSources,
    Source(i64),
    Category(i64, usize),
    None,
}

thread_local! {
    static HOME: RefCell<Home> = RefCell::new(Home::default());
}

fn with_home<R>(f: impl FnOnce(&mut Home) -> R) -> R {
    HOME.with_borrow_mut(f)
}

pub fn setup(window: &AppWindow) {
    let state = window.global::<HomeState>();
    with_home(|home| {
        state.set_channels(ModelRc::from(home.model.clone()));
        state.set_fav_media(ModelRc::from(home.fav.model.clone()));
        state.set_sidebar(ModelRc::from(home.sidebar.model.clone()));
    });
    state.set_fav_offset(FAV as i32);

    let weak = window.as_weak();
    state.on_rail_selected(move |rail| {
        if let Some(window) = weak.upgrade() {
            select_rail(&window, rail);
        }
    });

    let weak = window.as_weak();
    state.on_toggle_adult_lock(move || {
        if let Some(window) = weak.upgrade() {
            toggle_adult_lock(&window);
        }
    });

    let weak = window.as_weak();
    state.on_search_edited(move |text| {
        let Some(window) = weak.upgrade() else { return };
        window.global::<HomeState>().set_channels_visible(false);
        let weak = weak.clone();
        with_home(|home| {
            home.search_timer
                .start(TimerMode::SingleShot, SEARCH_DEBOUNCE, move || {
                    let Some(window) = weak.upgrade() else { return };
                    let query = text.trim().to_string();
                    update_filters(|f| f.query = (!query.is_empty()).then_some(query));
                    load(&window, false);
                })
        });
    });

    let weak = window.as_weak();
    state.on_toggle_keywords(move || {
        let Some(window) = weak.upgrade() else { return };
        let on = !window.global::<HomeState>().get_use_keywords();
        window.global::<HomeState>().set_use_keywords(on);
        update_filters(|f| f.use_keywords = on);
        load(&window, false);
    });

    let weak = window.as_weak();
    state.on_sort_selected(move |sort| {
        let Some(window) = weak.upgrade() else { return };
        window.global::<HomeState>().set_sort(sort);
        update_filters(|f| f.sort = sort as u8);
        load(&window, false);
    });

    let weak = window.as_weak();
    state.on_load_more(move || {
        if let Some(window) = weak.upgrade() {
            load(&window, true);
        }
    });

    state.on_load_more_fav(|| load_fav(true));

    let weak = window.as_weak();
    state.on_go_back(move || {
        if let Some(window) = weak.upgrade() {
            go_back(&window);
        }
    });

    let weak = window.as_weak();
    state.on_escape_pressed(move || {
        if let Some(window) = weak.upgrade() {
            escape(&window);
        }
    });

    let weak = window.as_weak();
    state.on_toggle_media_type(move |media_type| {
        if let Some(window) = weak.upgrade() {
            toggle_media_type(&window, media_type as u8);
        }
    });

    state.on_clear_history(|| {
        crate::spawn(crate::blocking(sql::clear_history), |window, result| {
            match result {
                Ok(()) => crate::show_toast(window, "History cleared successfully"),
                Err(e) => crate::show_error(window, &e.context("Failed to clear history")),
            }
            load(window, false);
        });
    });

    let weak = window.as_weak();
    state.on_sidebar_clicked(move |row| {
        if let Some(window) = weak.upgrade() {
            sidebar_clicked(&window, row as usize);
        }
    });

    state.on_sidebar_toggled(|row| sidebar_toggled(row as usize));

    let weak = window.as_weak();
    state.on_tile_activated(move |index| {
        if let Some(window) = weak.upgrade() {
            tile_activated(&window, index as usize);
        }
    });

    state.on_tile_favorite(|index| toggle_favorite(index as usize));
    state.on_tile_hide(|index| toggle_hidden(index as usize));

    state.on_tile_remove_history(|index| {
        let Some(channel) = channel_at(index as usize) else {
            return;
        };
        let Some(id) = channel.id else { return };
        crate::spawn(
            crate::blocking(move || sql::remove_last_watched(id)),
            move |window, result| match result {
                Ok(()) => {
                    crate::show_toast(
                        window,
                        &format!("Removed \"{}\" from history", channel.name),
                    );
                    load(window, false);
                }
                Err(e) => crate::show_error(window, &e),
            },
        );
    });

    let weak = window.as_weak();
    state.on_tile_copy_url(move |index| {
        let Some(window) = weak.upgrade() else { return };
        let Some(url) = channel_at(index as usize).and_then(|c| c.url) else {
            return;
        };
        match crate::copy_to_clipboard(url) {
            Ok(()) => crate::show_toast(&window, "Copied channel URL"),
            Err(e) => crate::show_error(&window, &e.context("Failed to copy the URL")),
        }
    });

    let weak = window.as_weak();
    state.on_tile_restream(move |index| {
        let Some(window) = weak.upgrade() else { return };
        if let Some(channel) = channel_at(index as usize) {
            crate::restream::open(&window, channel);
        }
    });

    state.on_bulk_action(|action| {
        let Some(filters) = with_home(|home| home.filters.clone()) else {
            return;
        };
        let name = ["hide", "unhide", "favorite", "unfavorite"]
            .get(action as usize)
            .copied()
            .unwrap_or_default();
        crate::spawn(
            crate::blocking(move || sql::bulk_update(filters, action as u8)),
            move |window, result| match result {
                Ok(()) => {
                    crate::show_toast(
                        window,
                        &format!("Successfully executed bulk update: {name}"),
                    );
                    load(window, false);
                }
                Err(e) => crate::show_error(window, &e),
            },
        );
    });

    state.on_sidebar_hide_category(|row| {
        let Some(category) = sidebar_category(row as usize) else {
            return;
        };
        let (Some(id), Some(source_id)) = (category.id, category.source_id) else {
            return;
        };
        crate::spawn(
            crate::blocking(move || sql::hide_group(id, true)),
            move |window, result| {
                if let Err(e) = result {
                    crate::show_error(window, &e);
                    return;
                }
                with_home(|home| {
                    if let Some(list) = home.sidebar.categories.get_mut(&source_id) {
                        list.retain(|c| c.id != Some(id));
                    }
                });
                rebuild_sidebar();
            },
        );
    });

    let weak = window.as_weak();
    state.on_sidebar_edit_category(move |row| {
        if let (Some(window), Some(category)) = (weak.upgrade(), sidebar_category(row as usize)) {
            crate::custom::edit(&window, category);
        }
    });
    state.on_sidebar_share_category(|row| {
        if let Some(category) = sidebar_category(row as usize) {
            crate::custom::share(category);
        }
    });
    let weak = window.as_weak();
    state.on_sidebar_delete_category(move |row| {
        if let (Some(window), Some(category)) = (weak.upgrade(), sidebar_category(row as usize)) {
            crate::custom::delete(&window, category);
        }
    });

    let weak = window.as_weak();
    state.on_tile_edit(move |index| {
        if let (Some(window), Some(channel)) = (weak.upgrade(), channel_at(index as usize)) {
            crate::custom::edit(&window, channel);
        }
    });
    state.on_tile_share(|index| {
        if let Some(channel) = channel_at(index as usize) {
            crate::custom::share(channel);
        }
    });
    let weak = window.as_weak();
    state.on_tile_delete(move |index| {
        if let (Some(window), Some(channel)) = (weak.upgrade(), channel_at(index as usize)) {
            crate::custom::delete(&window, channel);
        }
    });

    state.on_tile_record(|index| {
        if let Some(channel) = channel_at(index as usize) {
            record(channel);
        }
    });
    state.on_tile_download(|index| {
        if let Some(channel) = channel_at(index as usize) {
            download_movie(channel);
        }
    });
    state.on_tile_cancel_download(|index| {
        if let Some(id) = channel_at(index as usize).and_then(|c| c.id) {
            crate::downloads::cancel(id.to_string());
        }
    });
    state.on_tile_download_all(|index| {
        if let Some(channel) = channel_at(index as usize) {
            download_all(channel);
        }
    });
}

/// Called once the sources and settings are known (at startup, or after
/// adding the first source).
pub fn start(window: &AppWindow, settings: Settings, sources: Vec<Source>) {
    let enabled: Vec<Source> = sources.into_iter().filter(|s| s.enabled).collect();
    let has_xtream = enabled.iter().any(|s| s.source_type == source_type::XTREAM);
    let first_start = with_home(|home| home.settings.is_none());
    let sort = settings.default_sort.unwrap_or(sort_type::PROVIDER);
    let rail = rail_from_number(settings.default_view.unwrap_or(1));
    let refresh_on_start = settings.refresh_on_start == Some(true);
    with_home(|home| {
        home.sources = enabled;
        home.settings = Some(settings);
        home.sidebar = Sidebar {
            model: home.sidebar.model.clone(),
            ..Default::default()
        };
    });
    window.global::<HomeState>().set_sort(sort as i32);
    window.set_page(Page::Home);
    crate::spawn(
        crate::blocking(settings::has_adult_pin),
        |window, result| match result {
            Ok(pin_set) => set_adult_pin(window, pin_set, false),
            Err(e) => crate::show_error(window, &e),
        },
    );

    if first_start {
        if has_xtream {
            crate::spawn(
                epg::on_start_check_epg(&STATE, crate::events()),
                |window, result| {
                    if let Err(e) = result {
                        crate::show_error(window, &e);
                    }
                },
            );
        }
        if refresh_on_start {
            crate::show_toast(
                window,
                "Refreshing all sources... (refresh on start enabled)",
            );
            crate::spawn(utils::refresh_all(), |window, result| match result {
                Ok(()) => {
                    crate::show_toast(window, "Successfully refreshed all sources");
                    load(window, false);
                }
                Err(e) => crate::show_error(window, &e.context("Failed to refresh sources")),
            });
        }
    }
    select_rail(window, rail);
}

pub fn set_settings(settings: Settings) {
    with_home(|home| home.settings = Some(settings));
}

/// Forgets which series' episodes were fetched, so they're fetched again
/// (after the provider was refreshed).
pub fn clear_series_cache() {
    with_home(|home| home.series_refreshed.clear());
}

/// After a custom channel or category was added, changed or deleted:
/// reloads the sidebar's categories and, on a channel view, the grid.
pub fn refresh(window: &AppWindow) {
    let media_type = with_home(|home| {
        home.sidebar.categories.clear();
        home.sidebar.media_type
    });
    let state = window.global::<HomeState>();
    if state.get_show_sidebar()
        && let Some(media_type) = media_type
    {
        load_sidebar(media_type);
    }
    if !matches!(
        state.get_rail(),
        Rail::Settings | Rail::ManageCategories | Rail::Downloads
    ) {
        load(window, false);
    }
}

/// After a source was added, changed, enabled, disabled or deleted. The
/// grid reloads when a rail item is next selected.
pub fn reload_sources() {
    crate::spawn(crate::blocking(sql::get_sources), |window, result| {
        let sources = match result {
            Ok(sources) => sources,
            Err(e) => {
                crate::show_error(window, &e);
                return;
            }
        };
        if sources.is_empty() {
            crate::setup::show(window, false);
            return;
        }
        with_home(|home| {
            home.sources = sources.into_iter().filter(|s| s.enabled).collect();
            home.sidebar = Sidebar {
                model: home.sidebar.model.clone(),
                ..Default::default()
            };
        });
    });
}

pub fn set_adult_pin(window: &AppWindow, pin_set: bool, unlocked: bool) {
    let state = window.global::<HomeState>();
    state.set_adult_pin_set(pin_set);
    state.set_adult_unlocked(unlocked);
}

fn toggle_adult_lock(window: &AppWindow) {
    if window.global::<HomeState>().get_adult_unlocked() {
        crate::spawn(
            async {
                api::lock_adult_content(&STATE).await;
                Ok(())
            },
            |window, _| {
                window.global::<HomeState>().set_adult_unlocked(false);
                load(window, false);
            },
        );
        return;
    }
    crate::dialog::ask_pin(
        window,
        "Enter PIN",
        "Adult content is locked. Enter your PIN to show it.",
        |_, pin| {
            crate::spawn(
                async move { api::verify_adult_pin(&pin, &STATE).await },
                |window, result| match result {
                    Ok(true) => {
                        crate::dialog::close(window);
                        window.global::<HomeState>().set_adult_unlocked(true);
                        load(window, false);
                    }
                    Ok(false) => crate::dialog::pin_rejected(window),
                    Err(e) => crate::show_error(window, &e),
                },
            );
        },
    );
}

fn rail_from_number(number: u8) -> Rail {
    match number {
        0 => Rail::Favourites,
        2 => Rail::Movies,
        3 => Rail::Series,
        4 => Rail::History,
        _ => Rail::Channels,
    }
}

fn select_rail(window: &AppWindow, rail: Rail) {
    let state = window.global::<HomeState>();
    state.set_rail(rail);
    if rail == Rail::Settings {
        state.set_show_sidebar(false);
        crate::settings_page::show(window);
        return;
    }
    if rail == Rail::ManageCategories {
        state.set_show_sidebar(false);
        crate::categories::show(window);
        return;
    }
    if rail == Rail::Downloads {
        state.set_show_sidebar(false);
        crate::downloads::refresh_history();
        return;
    }
    let (view, media_types) = match rail {
        // Favourite movies and series are in a second section (load_fav).
        Rail::Favourites => (view_type::FAVORITES, vec![media_type::LIVESTREAM]),
        Rail::Movies => (view_type::ALL, vec![media_type::MOVIE]),
        Rail::Series => (view_type::ALL, vec![media_type::SERIE]),
        Rail::History => (
            view_type::HISTORY,
            vec![media_type::LIVESTREAM, media_type::MOVIE, media_type::SERIE],
        ),
        _ => (view_type::ALL, vec![media_type::LIVESTREAM]),
    };
    let show_sidebar = matches!(rail, Rail::Channels | Rail::Movies | Rail::Series);
    with_home(|home| {
        home.filters = Some(Filters {
            query: None,
            source_ids: home.sources.iter().filter_map(|s| s.id).collect(),
            media_types: Some(media_types.clone()),
            view_type: view,
            page: 1,
            series_id: None,
            group_id: None,
            use_keywords: state.get_use_keywords(),
            sort: state.get_sort() as u8,
            season: None,
        });
        home.nodes.clear();
    });
    state.set_search_text("".into());
    state.set_back_title("".into());
    state.set_view_type(view as i32);
    state.set_show_sidebar(show_sidebar);
    if show_sidebar {
        load_sidebar(media_types[0]);
    }
    load(window, false);
}

fn update_filters(f: impl FnOnce(&mut Filters)) {
    with_home(|home| {
        if let Some(filters) = home.filters.as_mut() {
            f(filters);
        }
    });
}

pub fn channel_at(index: usize) -> Option<Channel> {
    with_home(|home| {
        if index >= FAV {
            home.fav.channels.get(index - FAV).cloned()
        } else {
            home.channels.get(index).cloned()
        }
    })
}

/// A loaded channel in either section, to update after a change.
fn channel_mut(home: &mut Home, id: i64) -> Option<&mut Channel> {
    home.channels
        .iter_mut()
        .chain(home.fav.channels.iter_mut())
        .find(|c| c.id == Some(id))
}

/// Whether the filters are Favourites' top level, which has the second
/// section for movies and series.
fn shows_fav_media(filters: &Filters) -> bool {
    filters.view_type == view_type::FAVORITES
        && filters.series_id.is_none()
        && filters.group_id.is_none()
        && filters.season.is_none()
}

// Loads the first page (more = false) or the next one.
fn load(window: &AppWindow, more: bool) {
    let request = with_home(|home| {
        let filters = home.filters.as_mut()?;
        if more {
            if home.loading || home.reached_max {
                return None;
            }
            filters.page = filters.page.checked_add(1)?;
        } else {
            filters.page = 1;
            home.load_token += 1;
        }
        home.loading = true;
        Some((filters.clone(), home.load_token))
    });
    let Some((filters, token)) = request else {
        return;
    };
    window.global::<HomeState>().set_loading(true);
    if !more {
        let fav = shows_fav_media(&filters);
        window.global::<HomeState>().set_show_fav_media(fav);
        if fav {
            load_fav(false);
        }
    }

    crate::spawn(
        api::search(filters.clone(), &STATE),
        move |window, result| {
            let current = with_home(|home| home.load_token == token);
            if !current {
                return;
            }
            with_home(|home| home.loading = false);
            let state = window.global::<HomeState>();
            state.set_loading(false);
            state.set_channels_visible(true);
            let channels = match result {
                Ok(channels) => channels,
                Err(e) => {
                    crate::show_error(window, &e);
                    return;
                }
            };
            let reached_max = channels.len() < sql::PAGE_SIZE as usize || filters.page == u8::MAX;
            state.set_channels_complete(reached_max);
            let items: Vec<ChannelItem> =
                with_home(|home| channels.iter().map(|c| item_for(home, c)).collect());
            with_home(|home| {
                home.reached_max = reached_max;
                if more {
                    home.channels.extend(channels.iter().cloned());
                    for item in items {
                        home.model.push(item);
                    }
                } else {
                    home.channels = channels.clone();
                    home.model.set_vec(items);
                }
            });
            if !more {
                state.set_scroll_reset(state.get_scroll_reset() + 1);
                state.set_empty_text(if shows_fav_media(&filters) {
                    "No favourite channels.".into()
                } else {
                    empty_text(&filters).into()
                });
                // Live channels only get the list layout with EPG timelines.
                let live_only = filters.media_types.as_deref() == Some(&[media_type::LIVESTREAM]);
                state.set_list_mode(live_only);
                state.set_show_epg(live_only && !lightweight());
                state.set_bulk_disabled(filters.series_id.is_some() && filters.season.is_none());
            }
            if state.get_list_mode() && state.get_show_epg() {
                crate::epg::fetch(channels.clone());
            }
            request_images(&channels);
            refresh_download_progress(window);
        },
    );
}

/// Loads the first page (more = false) or the next one of Favourites'
/// movies and series. They follow the main grid's search, sort and sources.
fn load_fav(more: bool) {
    let request = with_home(|home| {
        if more {
            if home.fav.loading || home.fav.reached_max {
                return None;
            }
            let filters = home.fav.filters.as_mut()?;
            filters.page = filters.page.checked_add(1)?;
        } else {
            let mut filters = home.filters.clone()?;
            filters.media_types = Some(vec![media_type::MOVIE, media_type::SERIE]);
            filters.page = 1;
            home.fav.filters = Some(filters);
            home.fav.load_token += 1;
        }
        home.fav.loading = true;
        Some((home.fav.filters.clone()?, home.fav.load_token))
    });
    let Some((filters, token)) = request else {
        return;
    };
    crate::spawn(
        api::search(filters.clone(), &STATE),
        move |window, result| {
            if with_home(|home| home.fav.load_token != token) {
                return;
            }
            with_home(|home| home.fav.loading = false);
            let channels = match result {
                Ok(channels) => channels,
                Err(e) => return crate::show_error(window, &e),
            };
            let reached_max = channels.len() < sql::PAGE_SIZE as usize || filters.page == u8::MAX;
            let state = window.global::<HomeState>();
            state.set_fav_complete(reached_max);
            let items: Vec<ChannelItem> =
                with_home(|home| channels.iter().map(|c| item_for(home, c)).collect());
            with_home(|home| {
                home.fav.reached_max = reached_max;
                if more {
                    home.fav.channels.extend(channels.iter().cloned());
                    for item in items {
                        home.fav.model.push(item);
                    }
                } else {
                    home.fav.channels = channels.clone();
                    home.fav.model.set_vec(items);
                }
            });
            if !more {
                state.set_fav_scroll_reset(state.get_fav_scroll_reset() + 1);
            }
            request_images(&channels);
            refresh_download_progress(window);
        },
    );
}

fn empty_text(filters: &Filters) -> &'static str {
    if filters.season.is_some() {
        "This season has no episodes."
    } else if filters.series_id.is_some() {
        "No seasons found for this series."
    } else if filters.query.is_some() {
        "No results found."
    } else {
        "Nothing to show here."
    }
}

fn item_for(home: &Home, channel: &Channel) -> ChannelItem {
    let source = home.sources.iter().find(|s| s.id == channel.source_id);
    let logo = channel
        .image
        .as_ref()
        .and_then(|url| home.images.get(url).cloned());
    ChannelItem {
        name: channel.name.as_str().into(),
        source_name: source.map(|s| s.name.as_str()).unwrap_or_default().into(),
        has_logo: logo.is_some(),
        logo: logo.unwrap_or_default(),
        media_type: channel.media_type as i32,
        favorite: channel.favorite,
        hidden: channel.hidden == Some(true),
        archive: channel.tv_archive == Some(true),
        custom: source.is_some_and(|s| s.source_type == source_type::CUSTOM),
        starting: channel.id.is_some_and(|id| home.starting.contains(&id)),
        faded: false,
        downloading: false,
        progress: 0.0,
        epg: ModelRc::default(),
        epg_loaded: false,
    }
}

pub fn loaded_channels() -> Vec<Channel> {
    with_home(|home| home.channels.clone())
}

/// Shows a channel's programmes on its row.
pub fn set_epg(id: i64, blocks: ModelRc<crate::EpgBlock>) {
    if let Some(index) = index_of(id) {
        update_item(index, |item| {
            item.epg = blocks;
            item.epg_loaded = true;
        });
    }
}

fn lightweight() -> bool {
    with_home(|home| {
        home.settings
            .as_ref()
            .is_some_and(|s| s.lightweight_mode == Some(true))
    })
}

/// Updates the tiles' download progress bars from the active downloads
/// (a movie's download id is its channel id).
pub fn refresh_download_progress(window: &AppWindow) {
    let progress: HashMap<String, f64> = crate::downloads::progress_by_id().into_iter().collect();
    window
        .global::<HomeState>()
        .set_batch_running(crate::downloads::batch_running());
    with_home(|home| {
        for (channels, model) in sections(home) {
            for (i, channel) in channels.iter().enumerate() {
                let Some(mut item) = model.row_data(i) else {
                    continue;
                };
                let current = channel.id.and_then(|id| progress.get(&id.to_string()));
                let downloading = current.is_some();
                let value = current.map(|p| (p / 100.0) as f32).unwrap_or(0.0);
                if item.downloading != downloading || item.progress != value {
                    item.downloading = downloading;
                    item.progress = value;
                    model.set_row_data(i, item);
                }
            }
        }
    });
}

/// The main grid's channels and tiles, then Favourites' movies and series.
fn sections(home: &Home) -> [(&[Channel], &VecModel<ChannelItem>); 2] {
    [
        (&home.channels, &home.model),
        (&home.fav.channels, &home.fav.model),
    ]
}

pub fn always_ask_save() -> bool {
    with_home(|home| {
        home.settings
            .as_ref()
            .is_some_and(|s| s.always_ask_save == Some(true))
    })
}

fn download_movie(channel: Channel) {
    let Some(id) = channel.id else { return };
    let ask = crate::downloads::ask_where_to_save();
    crate::spawn(
        async move {
            let path = if ask {
                let file_name = utils::get_filename(
                    channel.name.clone(),
                    channel.url.clone().unwrap_or_default(),
                )?;
                let picked = rfd::AsyncFileDialog::new()
                    .set_title("Select where to download movie")
                    .set_file_name(file_name)
                    .save_file()
                    .await;
                let Some(picked) = picked else { return Ok(()) };
                Some(picked.path().to_string_lossy().into_owned())
            } else {
                None
            };
            crate::downloads::download(id.to_string(), channel, path).await;
            Ok(())
        },
        |window, result| {
            if let Err(e) = result {
                crate::show_error(window, &e);
            }
        },
    );
}

// "Download Series" or "Download Season": every episode, queued.
fn download_all(channel: Channel) {
    let ask = crate::downloads::ask_where_to_save();
    let what = if channel.media_type == media_type::SERIE {
        "series"
    } else {
        "season"
    };
    crate::spawn(
        async move {
            let base = if ask {
                let picked = rfd::AsyncFileDialog::new()
                    .set_title(format!("Select where to download the {what}"))
                    .pick_folder()
                    .await;
                let Some(picked) = picked else { return Ok(()) };
                picked.path().to_string_lossy().into_owned()
            } else {
                crate::blocking(utils::get_download_base_path).await?
            };
            let (show, episodes) = if channel.media_type == media_type::SERIE {
                let episodes = xtream::get_series_episodes_for_download(channel.clone()).await?;
                let episodes = episodes
                    .into_iter()
                    .map(|e| (e.channel, e.season_name))
                    .collect();
                (channel.name, episodes)
            } else {
                let season = channel.name.clone();
                let info =
                    crate::blocking(move || xtream::get_season_episodes_for_download(channel))
                        .await?;
                let episodes = info
                    .episodes
                    .into_iter()
                    .map(|e| (e, season.clone()))
                    .collect();
                (info.series_name, episodes)
            };
            crate::downloads::enqueue_series(base, show, episodes).await
        },
        move |window, result| {
            if let Err(e) = result {
                crate::show_error(window, &e.context(format!("Failed to download the {what}")));
            }
        },
    );
}

fn request_images(channels: &[Channel]) {
    if lightweight() {
        return;
    }
    for url in channels.iter().filter_map(|c| c.image.clone()) {
        if url.is_empty() {
            continue;
        }
        let wanted = with_home(|home| {
            !home.images.contains_key(&url) && home.images_pending.insert(url.clone())
        });
        if !wanted {
            continue;
        }
        crate::spawn(crate::images::load(url.clone()), move |_, result| {
            with_home(|home| home.images_pending.remove(&url));
            // A missing or broken image just leaves the tile without one.
            let Ok(buffer) = result else { return };
            let image = Image::from_rgba8(buffer);
            with_home(|home| {
                if home.images.len() >= IMAGE_CACHE_LIMIT {
                    home.images.clear();
                }
                home.images.insert(url.clone(), image.clone());
                for (channels, model) in sections(home) {
                    for (i, channel) in channels.iter().enumerate() {
                        if channel.image.as_deref() == Some(url.as_str())
                            && let Some(mut item) = model.row_data(i)
                        {
                            item.logo = image.clone();
                            item.has_logo = true;
                            model.set_row_data(i, item);
                        }
                    }
                }
            });
        });
    }
}

fn update_item(index: usize, f: impl FnOnce(&mut ChannelItem)) {
    with_home(|home| {
        let (model, row) = if index >= FAV {
            (&home.fav.model, index - FAV)
        } else {
            (&home.model, index)
        };
        if let Some(mut item) = model.row_data(row) {
            f(&mut item);
            model.set_row_data(row, item);
        }
    });
}

// Finds a channel's current tile index (FAV + row in Favourites' second
// section); the list may have reloaded since an action on it started.
fn index_of(id: i64) -> Option<usize> {
    with_home(|home| {
        home.channels
            .iter()
            .position(|c| c.id == Some(id))
            .or_else(|| {
                let row = home.fav.channels.iter().position(|c| c.id == Some(id))?;
                Some(FAV + row)
            })
    })
}

fn tile_activated(window: &AppWindow, index: usize) {
    let Some(channel) = channel_at(index) else {
        return;
    };
    match channel.media_type {
        media_type::SERIE => open_series(window, channel),
        media_type::GROUP => {
            let Some(id) = channel.id else { return };
            push_node(window, None, |f| {
                f.group_id = Some(id);
                f.series_id = None;
                f.season = None;
                f.source_ids = channel.source_id.into_iter().collect();
            });
        }
        media_type::SEASON => {
            let Some(id) = channel.id else { return };
            push_node(window, Some(channel.name.clone()), |f| f.season = Some(id));
        }
        _ => play(channel),
    }
}

fn open_series(window: &AppWindow, channel: Channel) {
    // A series' url holds its provider series id.
    let Some(series_id) = channel.url.as_deref().and_then(|u| u.parse::<i64>().ok()) else {
        crate::show_error(window, &anyhow::anyhow!("This series has no series id"));
        return;
    };
    let open = move |window: &AppWindow, channel: Channel| {
        push_node(window, Some(channel.name.clone()), |f| {
            f.series_id = Some(series_id);
            f.source_ids = channel.source_id.into_iter().collect();
            f.media_types = Some(vec![media_type::SERIE]);
        });
    };
    if with_home(|home| home.series_refreshed.contains(&series_id)) {
        open(window, channel);
        return;
    }
    window.global::<HomeState>().set_channels_visible(false);
    let fetched = channel.clone();
    crate::spawn(
        xtream::get_episodes(fetched),
        move |window, result| match result {
            Ok(()) => {
                with_home(|home| home.series_refreshed.insert(series_id));
                open(window, channel);
            }
            Err(e) => {
                window.global::<HomeState>().set_channels_visible(true);
                crate::show_error(window, &e.context("Failed to fetch series"));
            }
        },
    );
}

fn push_node(window: &AppWindow, title: Option<String>, change: impl FnOnce(&mut Filters)) {
    let state = window.global::<HomeState>();
    let pushed = with_home(|home| {
        let filters = home.filters.as_mut()?;
        home.nodes.push(Node {
            filters: filters.clone(),
            search_text: state.get_search_text().into(),
            title: title.clone(),
        });
        change(filters);
        filters.query = None;
        if filters.view_type == view_type::HIDDEN {
            filters.view_type = view_type::CATEGORIES;
        }
        Some(())
    });
    if pushed.is_none() {
        return;
    }
    state.set_search_text("".into());
    refresh_back_title(window);
    load(window, false);
}

fn go_back(window: &AppWindow) {
    let node = with_home(|home| {
        let node = home.nodes.pop()?;
        home.filters = Some(node.filters.clone());
        Some(node)
    });
    let Some(node) = node else { return };
    window
        .global::<HomeState>()
        .set_search_text(node.search_text.into());
    refresh_back_title(window);
    rebuild_sidebar();
    load(window, false);
}

/// Esc: clears the search if there is one, or else goes back a level.
fn escape(window: &AppWindow) {
    let state = window.global::<HomeState>();
    let searching = !state.get_search_text().is_empty()
        || with_home(|home| home.filters.as_ref().is_some_and(|f| f.query.is_some()));
    if !searching {
        go_back(window);
        return;
    }
    with_home(|home| home.search_timer.stop());
    state.set_search_text("".into());
    update_filters(|f| f.query = None);
    load(window, false);
}

/// Ctrl+Q/W/E: adds a media type to the grid, or removes it (unless it's
/// the only one).
fn toggle_media_type(window: &AppWindow, media_type: u8) {
    let changed = with_home(|home| {
        let Some(types) = home.filters.as_mut().and_then(|f| f.media_types.as_mut()) else {
            return false;
        };
        match types.iter().position(|t| *t == media_type) {
            Some(_) if types.len() == 1 => return false,
            Some(i) => {
                types.remove(i);
            }
            None => {
                types.push(media_type);
                types.sort();
            }
        }
        true
    });
    if changed {
        load(window, false);
    }
}

fn refresh_back_title(window: &AppWindow) {
    let title = with_home(|home| home.nodes.last().and_then(|n| n.title.clone()));
    window
        .global::<HomeState>()
        .set_back_title(title.unwrap_or_default().into());
}

fn play(channel: Channel) {
    start_player(channel, false, None);
}

fn record(channel: Channel) {
    if !crate::downloads::ask_where_to_save() {
        // The core records into the recording folder.
        start_player(channel, true, None);
        return;
    }
    let date = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S");
    let file_name = format!("{}_{date}.mp4", utils::sanitize(channel.name.clone()));
    crate::spawn(
        async move {
            Ok(rfd::AsyncFileDialog::new()
                .set_title("Select where to save recording")
                .set_file_name(file_name)
                .save_file()
                .await
                .map(|f| f.path().to_string_lossy().into_owned()))
        },
        move |_, result| {
            if let Ok(Some(path)) = result {
                start_player(channel, true, Some(path));
            }
        },
    );
}

fn start_player(channel: Channel, record: bool, record_path: Option<String>) {
    let Some(id) = channel.id else { return };
    let already_starting = with_home(|home| !home.starting.insert(id));
    if already_starting {
        // Second click while the player starts: cancel it.
        let source_id = channel.source_id.unwrap_or_default();
        crate::spawn(
            async move { mpv::cancel_play(source_id, id.to_string(), &STATE).await },
            |window, result| {
                if let Err(e) = result {
                    crate::show_error(window, &e);
                }
            },
        );
        return;
    }
    if let Some(index) = index_of(id) {
        update_item(index, |item| item.starting = true);
    }
    crate::spawn(
        async move {
            // Returns once the player has exited.
            api::play(channel, record, record_path, &STATE).await?;
            crate::blocking(move || sql::add_last_watched(id)).await
        },
        move |window, result| {
            with_home(|home| home.starting.remove(&id));
            if let Some(index) = index_of(id) {
                update_item(index, |item| item.starting = false);
            }
            if let Err(e) = result {
                crate::show_error(window, &e);
            }
        },
    );
}

fn toggle_favorite(index: usize) {
    let Some(channel) = channel_at(index) else {
        return;
    };
    let Some(id) = channel.id else { return };
    let favorite = !channel.favorite;
    crate::spawn(
        crate::blocking(move || sql::favorite_channel(id, favorite)),
        move |window, result| {
            if let Err(e) = result {
                crate::show_error(window, &e);
                return;
            }
            let in_favorites =
                window.global::<HomeState>().get_view_type() == view_type::FAVORITES as i32;
            with_home(|home| {
                if let Some(c) = channel_mut(home, id) {
                    c.favorite = favorite;
                }
            });
            if let Some(index) = index_of(id) {
                update_item(index, |item| {
                    item.favorite = favorite;
                    item.faded = in_favorites && !favorite;
                });
            }
            let message = if favorite {
                format!("Added \"{}\" to favorites", channel.name)
            } else if in_favorites {
                format!(
                    "Removed \"{}\" from favorites (updates on reload)",
                    channel.name
                )
            } else {
                format!("Removed \"{}\" from favorites", channel.name)
            };
            crate::show_toast(window, &message);
        },
    );
}

fn toggle_hidden(index: usize) {
    let Some(channel) = channel_at(index) else {
        return;
    };
    let Some(id) = channel.id else { return };
    let hidden = channel.hidden != Some(true);
    let is_group = channel.media_type == media_type::GROUP;
    crate::spawn(
        crate::blocking(move || {
            if is_group {
                sql::hide_group(id, hidden)
            } else {
                sql::hide_channel(id, hidden)
            }
        }),
        move |window, result| {
            if let Err(e) = result {
                crate::show_error(window, &e);
                return;
            }
            let in_hidden_view =
                window.global::<HomeState>().get_view_type() == view_type::HIDDEN as i32;
            with_home(|home| {
                if let Some(c) = channel_mut(home, id) {
                    c.hidden = Some(hidden);
                }
            });
            if let Some(index) = index_of(id) {
                update_item(index, |item| {
                    item.hidden = hidden;
                    item.faded = hidden != in_hidden_view;
                });
            }
            let what = if is_group { "group " } else { "" };
            let verb = if hidden { "Hidden" } else { "Unhidden" };
            crate::show_toast(
                window,
                &format!("{verb} {what}\"{}\" (updates on reload)", channel.name),
            );
        },
    );
}

// Playlist sidebar

fn load_sidebar(media_type: u8) {
    let source_ids: Vec<i64> = with_home(|home| {
        let sidebar = &mut home.sidebar;
        if sidebar.media_type != Some(media_type) {
            // Categories are per media type, so the cache is stale.
            sidebar.media_type = Some(media_type);
            sidebar.categories.clear();
            sidebar.expanded = home.sources.iter().filter_map(|s| s.id).collect();
        }
        home.sources.iter().filter_map(|s| s.id).collect()
    });
    for id in source_ids {
        let needed = with_home(|home| {
            home.sidebar.expanded.contains(&id) && !home.sidebar.categories.contains_key(&id)
        });
        if needed {
            load_categories(id, media_type);
        }
    }
    rebuild_sidebar();
}

fn load_categories(source_id: i64, media_type: u8) {
    with_home(|home| home.sidebar.loading.insert(source_id));
    crate::spawn(
        async move {
            let mut all = Vec::new();
            for page in 1..=u8::MAX {
                let filters = Filters {
                    query: None,
                    source_ids: vec![source_id],
                    media_types: Some(vec![media_type]),
                    view_type: view_type::CATEGORIES,
                    page,
                    series_id: None,
                    group_id: None,
                    use_keywords: false,
                    sort: sort_type::PROVIDER,
                    season: None,
                };
                let groups = api::search(filters, &STATE).await?;
                let done = groups.len() < sql::PAGE_SIZE as usize;
                all.extend(groups);
                if done {
                    break;
                }
            }
            Ok(all)
        },
        move |window, result| {
            let current = with_home(|home| {
                home.sidebar.loading.remove(&source_id);
                home.sidebar.media_type == Some(media_type)
            });
            match result {
                Ok(groups) if current => {
                    with_home(|home| home.sidebar.categories.insert(source_id, groups));
                }
                Ok(_) => {}
                Err(e) => crate::show_error(window, &e.context("Failed to load categories")),
            }
            rebuild_sidebar();
        },
    );
}

fn rebuild_sidebar() {
    with_home(|home| {
        let filters = home.filters.as_ref();
        let selected_sources = filters.map(|f| f.source_ids.clone()).unwrap_or_default();
        let selected_group = filters.and_then(|f| f.group_id);
        let single_source = (selected_sources.len() == 1).then(|| selected_sources[0]);
        let sidebar = &mut home.sidebar;
        let mut rows = vec![SidebarRow {
            kind: 0,
            name: "All playlists".into(),
            selected: single_source.is_none() && selected_group.is_none(),
            expanded: false,
            custom: false,
        }];
        let mut actions = vec![SidebarAction::AllSources];
        for source in &home.sources {
            let Some(id) = source.id else { continue };
            let expanded = sidebar.expanded.contains(&id);
            rows.push(SidebarRow {
                kind: 1,
                name: source.name.as_str().into(),
                selected: single_source == Some(id) && selected_group.is_none(),
                expanded,
                custom: false,
            });
            actions.push(SidebarAction::Source(id));
            if !expanded {
                continue;
            }
            let message = |text: &str| SidebarRow {
                kind: 3,
                name: text.into(),
                selected: false,
                expanded: false,
                custom: false,
            };
            if sidebar.loading.contains(&id) {
                rows.push(message("Loading..."));
                actions.push(SidebarAction::None);
                continue;
            }
            let categories = sidebar
                .categories
                .get(&id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            if categories.is_empty() {
                rows.push(message("No categories"));
                actions.push(SidebarAction::None);
            }
            for (i, category) in categories.iter().enumerate() {
                rows.push(SidebarRow {
                    kind: 2,
                    name: category.name.as_str().into(),
                    selected: category.id.is_some() && category.id == selected_group,
                    expanded: false,
                    custom: source.source_type == source_type::CUSTOM,
                });
                actions.push(SidebarAction::Category(id, i));
            }
        }
        sidebar.model.set_vec(rows);
        sidebar.actions = actions;
    });
}

/// The category on a sidebar row, with its source id filled in.
fn sidebar_category(row: usize) -> Option<Channel> {
    let Some(SidebarAction::Category(source_id, i)) =
        with_home(|home| home.sidebar.actions.get(row).copied())
    else {
        return None;
    };
    let mut category = with_home(|home| home.sidebar.categories.get(&source_id)?.get(i).cloned())?;
    category.source_id = Some(source_id);
    category.media_type = media_type::GROUP;
    Some(category)
}

fn sidebar_clicked(window: &AppWindow, row: usize) {
    let action = with_home(|home| home.sidebar.actions.get(row).copied());
    match action {
        Some(SidebarAction::AllSources) => select_sources(window, None),
        Some(SidebarAction::Source(id)) => select_sources(window, Some(id)),
        Some(SidebarAction::Category(source_id, i)) => {
            let category = with_home(|home| {
                home.sidebar
                    .categories
                    .get(&source_id)
                    .and_then(|c| c.get(i))
                    .cloned()
            });
            let Some(Some(id)) = category.map(|c| c.id) else {
                return;
            };
            let media_type = with_home(|home| home.sidebar.media_type);
            // Selecting another category replaces this one rather than
            // stacking on it.
            with_home(|home| {
                if let Some(node) = home.nodes.drain(..).next() {
                    home.filters = Some(node.filters);
                }
            });
            push_node(window, None, |f| {
                f.group_id = Some(id);
                f.series_id = None;
                f.season = None;
                f.source_ids = vec![source_id];
                if let Some(media_type) = media_type {
                    f.media_types = Some(vec![media_type]);
                }
            });
            rebuild_sidebar();
        }
        Some(SidebarAction::None) | None => {}
    }
}

fn select_sources(window: &AppWindow, source_id: Option<i64>) {
    with_home(|home| {
        if let Some(node) = home.nodes.drain(..).next() {
            home.filters = Some(node.filters);
        }
        let all: Vec<i64> = home.sources.iter().filter_map(|s| s.id).collect();
        if let Some(filters) = home.filters.as_mut() {
            filters.source_ids = source_id.map(|id| vec![id]).unwrap_or(all);
            filters.group_id = None;
        }
    });
    refresh_back_title(window);
    rebuild_sidebar();
    load(window, false);
}

fn sidebar_toggled(row: usize) {
    let Some(SidebarAction::Source(id)) = with_home(|home| home.sidebar.actions.get(row).copied())
    else {
        return;
    };
    let (expanded, media_type) = with_home(|home| {
        let sidebar = &mut home.sidebar;
        let expanded = if sidebar.expanded.remove(&id) {
            false
        } else {
            sidebar.expanded.insert(id)
        };
        (
            expanded && !sidebar.categories.contains_key(&id),
            sidebar.media_type,
        )
    });
    if let (true, Some(media_type)) = (expanded, media_type) {
        load_categories(id, media_type);
    }
    rebuild_sidebar();
}
