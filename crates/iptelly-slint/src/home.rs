// The home screen: nav rail, playlist sidebar, search and the channel grid.
// Ported from src/app/home/home.component.ts and its children.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use iptelly_core::types::{Channel, Filters, Settings, Source};
use iptelly_core::{
    api, epg, media_type, mpv, sort_type, source_type, sql, utils, view_type, xtream,
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
    // Kept alive because on X11 the clipboard contents vanish with it.
    clipboard: Option<arboard::Clipboard>,
}

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
        state.set_sidebar(ModelRc::from(home.sidebar.model.clone()));
    });

    let weak = window.as_weak();
    state.on_rail_selected(move |rail| {
        if let Some(window) = weak.upgrade() {
            select_rail(&window, rail);
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

    let weak = window.as_weak();
    state.on_go_back(move || {
        if let Some(window) = weak.upgrade() {
            go_back(&window);
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
        let result = with_home(|home| -> Result<()> {
            if home.clipboard.is_none() {
                home.clipboard = Some(arboard::Clipboard::new()?);
            }
            home.clipboard.as_mut().unwrap().set_text(url)?;
            Ok(())
        });
        match result {
            Ok(()) => crate::show_toast(&window, "Copied channel URL"),
            Err(e) => crate::show_error(&window, &e.context("Failed to copy the URL")),
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

    if first_start {
        if has_xtream {
            let events = crate::events::new(window);
            crate::spawn(epg::on_start_check_epg(&STATE, events), |window, result| {
                if let Err(e) = result {
                    crate::show_error(window, &e);
                }
            });
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
    // Not ported yet: these show a placeholder (see PORTING.md).
    if matches!(
        rail,
        Rail::Downloads | Rail::ManageCategories | Rail::Settings
    ) {
        state.set_show_sidebar(false);
        return;
    }
    let (view, media_types) = match rail {
        // The Angular app shows favourite movies and series in a second
        // section below the channels; until that's ported they share one grid.
        Rail::Favourites => (
            view_type::FAVORITES,
            vec![media_type::LIVESTREAM, media_type::MOVIE, media_type::SERIE],
        ),
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

fn channel_at(index: usize) -> Option<Channel> {
    with_home(|home| home.channels.get(index).cloned())
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
                state.set_empty_text(empty_text(&filters).into());
            }
            request_images(&channels);
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
    }
}

fn request_images(channels: &[Channel]) {
    let lightweight = with_home(|home| {
        home.settings
            .as_ref()
            .is_some_and(|s| s.lightweight_mode == Some(true))
    });
    if lightweight {
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
                for (i, channel) in home.channels.iter().enumerate() {
                    if channel.image.as_deref() == Some(url.as_str()) {
                        if let Some(mut item) = home.model.row_data(i) {
                            item.logo = image.clone();
                            item.has_logo = true;
                            home.model.set_row_data(i, item);
                        }
                    }
                }
            });
        });
    }
}

fn update_item(index: usize, f: impl FnOnce(&mut ChannelItem)) {
    with_home(|home| {
        if let Some(mut item) = home.model.row_data(index) {
            f(&mut item);
            home.model.set_row_data(index, item);
        }
    });
}

// Finds a channel's current row; the list may have reloaded since an
// action on it started.
fn index_of(id: i64) -> Option<usize> {
    with_home(|home| home.channels.iter().position(|c| c.id == Some(id)))
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
    load(window, false);
}

fn refresh_back_title(window: &AppWindow) {
    let title = with_home(|home| home.nodes.last().and_then(|n| n.title.clone()));
    window
        .global::<HomeState>()
        .set_back_title(title.unwrap_or_default().into());
}

fn play(channel: Channel) {
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
            api::play(channel, false, None, &STATE).await?;
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
                if let Some(c) = home.channels.iter_mut().find(|c| c.id == Some(id)) {
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
                if let Some(c) = home.channels.iter_mut().find(|c| c.id == Some(id)) {
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
                });
                actions.push(SidebarAction::Category(id, i));
            }
        }
        sidebar.model.set_vec(rows);
        sidebar.actions = actions;
    });
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
