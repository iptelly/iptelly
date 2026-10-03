// Manage Categories: hiding and unhiding categories, and unhiding channels
// hidden one by one. Ported from src/app/manage-categories/.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use iptelly_core::types::{Channel, Group, Source};
use iptelly_core::{media_type, sql};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{AppWindow, CategoriesState, CategoryRow};

const SECTIONS: [(u8, &str); 3] = [
    (media_type::LIVESTREAM, "Channels"),
    (media_type::MOVIE, "Movies"),
    (media_type::SERIE, "Series"),
];
const CHANNELS_SECTION: i32 = 3;

#[derive(Clone, Copy, PartialEq)]
enum RowRef {
    Header(i32),
    Group(i64),
    Channel(i64),
    None,
}

#[derive(Default)]
struct Page {
    sources: Vec<Source>,
    groups: Vec<Group>,
    hidden_channels: Vec<Channel>,
    filter: String,
    selected: HashSet<i64>,
    selected_channels: HashSet<i64>,
    expanded: HashSet<i32>,
    last_clicked: Option<usize>,
    rows: Vec<RowRef>,
    model: Rc<VecModel<CategoryRow>>,
}

thread_local! {
    static PAGE: RefCell<Page> = RefCell::new(Page::default());
}

fn with_page<R>(f: impl FnOnce(&mut Page) -> R) -> R {
    PAGE.with_borrow_mut(f)
}

pub fn setup(window: &AppWindow) {
    let state = window.global::<CategoriesState>();
    with_page(|page| state.set_rows(ModelRc::from(page.model.clone())));

    state.on_source_selected(|index| {
        with_page(|page| {
            page.selected.clear();
            page.selected_channels.clear();
            page.last_clicked = None;
        });
        load_hidden_channels(index as usize);
    });

    let weak = window.as_weak();
    state.on_filter_edited(move |text| {
        with_page(|page| page.filter = text.trim().to_lowercase());
        if let Some(window) = weak.upgrade() {
            rebuild(&window);
        }
    });

    let weak = window.as_weak();
    state.on_row_clicked(move |index, shift| {
        if let Some(window) = weak.upgrade() {
            row_clicked(&window, index as usize, shift);
        }
    });

    state.on_hide_selected(|| set_groups_hidden(true));
    state.on_unhide_selected(|| set_groups_hidden(false));

    state.on_unhide_channels(|| {
        let ids: Vec<i64> = with_page(|page| page.selected_channels.iter().copied().collect());
        let count = ids.len();
        busy(true);
        let unhidden = ids.clone();
        crate::spawn(
            crate::blocking(move || sql::set_channels_hidden(&ids, false)),
            move |window, result| {
                busy(false);
                if let Err(e) = result {
                    crate::show_error(window, &e);
                    return;
                }
                with_page(|page| {
                    page.hidden_channels
                        .retain(|c| !c.id.is_some_and(|id| unhidden.contains(&id)));
                    page.selected_channels.clear();
                });
                let plural = if count == 1 { "" } else { "s" };
                crate::show_toast(
                    window,
                    &format!("Unhid {count} channel{plural} successfully"),
                );
                rebuild(window);
            },
        );
    });
}

/// Loads the sources and categories. Called each time the rail item is
/// selected.
pub fn show(window: &AppWindow) {
    let state = window.global::<CategoriesState>();
    state.set_filter("".into());
    with_page(|page| {
        page.filter.clear();
        page.selected.clear();
        page.selected_channels.clear();
        page.last_clicked = None;
    });
    crate::spawn(
        crate::blocking(|| Ok((sql::get_enabled_sources()?, sql::get_all_groups()?))),
        |window, result| {
            let (sources, groups) = match result {
                Ok(loaded) => loaded,
                Err(e) => {
                    crate::show_error(window, &e);
                    return;
                }
            };
            let names: Vec<SharedString> = sources.iter().map(|s| s.name.as_str().into()).collect();
            let state = window.global::<CategoriesState>();
            let index = (state.get_source() as usize).min(names.len().saturating_sub(1));
            state.set_sources(ModelRc::new(VecModel::from(names)));
            state.set_source(index as i32);
            with_page(|page| {
                page.sources = sources;
                page.groups = groups;
                page.hidden_channels.clear();
            });
            rebuild(window);
            load_hidden_channels(index);
        },
    );
}

fn load_hidden_channels(index: usize) {
    let Some(source_id) = with_page(|page| page.sources.get(index).and_then(|s| s.id)) else {
        return;
    };
    crate::spawn(
        crate::blocking(move || sql::get_hidden_channels(source_id)),
        |window, result| match result {
            Ok(channels) => {
                with_page(|page| page.hidden_channels = channels);
                rebuild(window);
            }
            Err(e) => crate::show_error(window, &e),
        },
    );
}

fn busy(busy: bool) {
    if let Some(window) = crate::window() {
        window.global::<CategoriesState>().set_busy(busy);
    }
}

fn rebuild(window: &AppWindow) {
    let source_index = window.global::<CategoriesState>().get_source() as usize;
    let (any_selected, any_channels_selected) = with_page(|page| {
        let source_id = page.sources.get(source_index).and_then(|s| s.id);
        let filter = page.filter.clone();
        let matches = |name: &str| filter.is_empty() || name.to_lowercase().contains(&filter);
        let mut rows = Vec::new();
        let mut refs = Vec::new();

        let mut section =
            |page: &Page, index: i32, title: &str, items: Vec<(RowRef, CategoryRow)>| {
                let expanded = page.expanded.contains(&index) || !filter.is_empty();
                rows.push(CategoryRow {
                    kind: 0,
                    section: index,
                    name: format!("{title} ({})", items.len()).into(),
                    checked: false,
                    hidden: false,
                    expanded,
                });
                refs.push(RowRef::Header(index));
                if !expanded {
                    return;
                }
                if items.is_empty() {
                    rows.push(CategoryRow {
                        kind: 3,
                        section: index,
                        name: if index == CHANNELS_SECTION {
                            "No hidden channels"
                        } else {
                            "No categories"
                        }
                        .into(),
                        checked: false,
                        hidden: false,
                        expanded: false,
                    });
                    refs.push(RowRef::None);
                }
                for (row_ref, row) in items {
                    rows.push(row);
                    refs.push(row_ref);
                }
            };

        for (index, (media_type, title)) in SECTIONS.iter().enumerate() {
            let items = page
                .groups
                .iter()
                .filter(|g| {
                    g.source_id == source_id
                        && g.media_type == Some(*media_type)
                        && matches(&g.name)
                })
                .filter_map(|g| {
                    let id = g.id?;
                    Some((
                        RowRef::Group(id),
                        CategoryRow {
                            kind: 1,
                            section: index as i32,
                            name: g.name.as_str().into(),
                            checked: page.selected.contains(&id),
                            hidden: g.hidden == Some(true),
                            expanded: false,
                        },
                    ))
                })
                .collect();
            section(page, index as i32, title, items);
        }
        let items = page
            .hidden_channels
            .iter()
            .filter(|c| matches(&c.name))
            .filter_map(|c| {
                let id = c.id?;
                Some((
                    RowRef::Channel(id),
                    CategoryRow {
                        kind: 2,
                        section: CHANNELS_SECTION,
                        name: c.name.as_str().into(),
                        checked: page.selected_channels.contains(&id),
                        hidden: true,
                        expanded: false,
                    },
                ))
            })
            .collect();
        section(page, CHANNELS_SECTION, "Individual Channels", items);

        page.model.set_vec(rows);
        page.rows = refs;
        (
            !page.selected.is_empty(),
            !page.selected_channels.is_empty(),
        )
    });
    let state = window.global::<CategoriesState>();
    state.set_any_selected(any_selected);
    state.set_any_channels_selected(any_channels_selected);
}

fn row_clicked(window: &AppWindow, index: usize, shift: bool) {
    with_page(|page| {
        let Some(&clicked) = page.rows.get(index) else {
            return;
        };
        if let RowRef::Header(section) = clicked {
            if !page.expanded.remove(&section) {
                page.expanded.insert(section);
            }
            return;
        }
        let is_channel = matches!(clicked, RowRef::Channel(_));
        let checked = match clicked {
            RowRef::Group(id) => !page.selected.contains(&id),
            RowRef::Channel(id) => !page.selected_channels.contains(&id),
            _ => return,
        };
        // Shift+click gives every row between the last clicked one and this
        // one the same state, as long as both are the same kind of row.
        let range = match page.last_clicked {
            Some(last)
                if shift
                    && matches!(page.rows.get(last), Some(r) if matches!(r, RowRef::Channel(_)) == is_channel) =>
            {
                last.min(index)..=last.max(index)
            }
            _ => index..=index,
        };
        let rows: Vec<RowRef> = page.rows[range].to_vec();
        for row in rows {
            let (set, id) = match row {
                RowRef::Group(id) if !is_channel => (&mut page.selected, id),
                RowRef::Channel(id) if is_channel => (&mut page.selected_channels, id),
                _ => continue,
            };
            if checked {
                set.insert(id);
            } else {
                set.remove(&id);
            }
        }
        page.last_clicked = Some(index);
    });
    rebuild(window);
}

fn set_groups_hidden(hidden: bool) {
    let ids: Vec<i64> = with_page(|page| page.selected.iter().copied().collect());
    let count = ids.len();
    busy(true);
    let changed = ids.clone();
    crate::spawn(
        crate::blocking(move || sql::set_groups_hidden(&ids, hidden)),
        move |window, result| {
            busy(false);
            if let Err(e) = result {
                crate::show_error(window, &e);
                return;
            }
            with_page(|page| {
                for group in page.groups.iter_mut() {
                    if group.id.is_some_and(|id| changed.contains(&id)) {
                        group.hidden = Some(hidden);
                    }
                }
                page.selected.clear();
                page.last_clicked = None;
            });
            let verb = if hidden { "Hid" } else { "Unhid" };
            let noun = if count == 1 { "category" } else { "categories" };
            crate::show_toast(window, &format!("{verb} {count} {noun} successfully"));
            // The home sidebar caches its category lists.
            crate::home::reload_sources();
            rebuild(window);
        },
    );
}
