// The EPG: timelines on live-channel rows, the header that pans them, and
// the programme dialog with catch-up and reminders.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::time::Duration;

use chrono::{Local, TimeZone};
use iptelly_core::types::{Channel, EPG, EPGNotify};
use iptelly_core::{api, epg, media_type, sql, xtream};
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};

use crate::{AppWindow, EpgBlock, EpgState, EpgTick, HomeState, STATE};

const LEAD: i64 = 30 * 60;
const DURATION: i64 = 4 * 3600;
const PAN_STEP: i64 = 3 * 3600;
const FETCH_AROUND: i64 = 24 * 3600;
const TICK: i64 = 30 * 60;

// Slint's int is 32-bit, so times go to the UI as seconds since startup.
static BASE: LazyLock<i64> = LazyLock::new(|| Local::now().timestamp());

fn now() -> i64 {
    Local::now().timestamp()
}

fn relative(timestamp: i64) -> i32 {
    (timestamp - *BASE).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

#[derive(Default)]
struct Epg {
    offset: i64,
    // Each loaded channel's programmes, as last fetched.
    programmes: HashMap<i64, Vec<EPG>>,
    clock: Timer,
    dialog: Option<Dialog>,
    // Programmes with a reminder set.
    reminders: HashSet<String>,
}

struct Dialog {
    channel: Channel,
    schedule: Vec<EPG>,
    index: usize,
    // Indexes already checked for catch-up.
    checked: HashSet<usize>,
    playing: bool,
}

thread_local! {
    static EPG_STATE: RefCell<Epg> = RefCell::new(Epg::default());
}

fn with_epg<R>(f: impl FnOnce(&mut Epg) -> R) -> R {
    EPG_STATE.with_borrow_mut(f)
}

pub fn setup(window: &AppWindow) {
    tick(window);
    let weak = window.as_weak();
    with_epg(|e| {
        e.clock
            .start(TimerMode::Repeated, Duration::from_secs(30), move || {
                if let Some(window) = weak.upgrade() {
                    tick(&window);
                }
            })
    });

    let state = window.global::<HomeState>();
    let weak = window.as_weak();
    state.on_epg_pan(move |direction| {
        let Some(window) = weak.upgrade() else { return };
        let offset = match direction {
            0 => 0,
            d => with_epg(|e| e.offset) + d as i64 * PAN_STEP,
        };
        set_offset(&window, offset);
    });

    let weak = window.as_weak();
    state.on_guide_key(move |row, key| {
        weak.upgrade()
            .is_some_and(|window| guide_key(&window, row, key))
    });

    let weak = window.as_weak();
    state.on_epg_clicked(move |row, index| {
        let Some(window) = weak.upgrade() else { return };
        let Some(channel) = crate::home::channel_at(row as usize) else {
            return;
        };
        let start = with_epg(|e| {
            let id = channel.id?;
            e.programmes
                .get(&id)?
                .get(index as usize)
                .map(|p| p.start_timestamp)
        });
        if let Some(start) = start {
            open(&window, channel, start);
        }
    });

    let dialog = window.global::<EpgState>();
    let weak = window.as_weak();
    dialog.on_close(move || {
        if let Some(window) = weak.upgrade() {
            with_epg(|e| e.dialog = None);
            window.global::<EpgState>().set_open(false);
        }
    });
    let weak = window.as_weak();
    dialog.on_prev(move || {
        if let Some(window) = weak.upgrade() {
            step(&window, -1);
        }
    });
    let weak = window.as_weak();
    dialog.on_next(move || {
        if let Some(window) = weak.upgrade() {
            step(&window, 1);
        }
    });
    dialog.on_play(play_catch_up);
    dialog.on_download(download_catch_up);
    dialog.on_cancel_download(|| {
        if let Some((id, _, _)) = current_download() {
            crate::downloads::cancel(id);
        }
    });
    dialog.on_toggle_reminder(toggle_reminder);
}

/// Pans the timeline (seconds from now) and fetches the programmes there.
fn set_offset(window: &AppWindow, offset: i64) {
    with_epg(|e| e.offset = offset);
    window.global::<HomeState>().set_epg_offset(offset as i32);
    tick(window);
    fetch(crate::home::loaded_channels());
}

// Keys for guide_key (HomeState.guide-key).
const GUIDE_RIGHT: i32 = 0;
const GUIDE_LEFT: i32 = 1;
const GUIDE_EXIT: i32 = 2;
const GUIDE_OPEN: i32 = 3;

/// Keyboard guide mode on a list row:
/// Right enters it on the programme on now, Left and Right step through
/// the programmes, panning the timeline to keep the cursor in view, and
/// Enter opens the programme. The cursor follows the programme's start
/// time, since a pan replaces the row's programmes.
fn guide_key(window: &AppWindow, row: i32, key: i32) -> bool {
    let state = window.global::<HomeState>();
    let Some(channel) = crate::home::channel_at(row as usize) else {
        return false;
    };
    let Some(id) = channel.id else { return false };
    let programmes = with_epg(|e| e.programmes.get(&id).cloned()).unwrap_or_default();
    let active = state.get_guide_row() == row;
    if !active {
        if key != GUIDE_RIGHT || programmes.is_empty() {
            return false;
        }
        let now = now();
        let on_now = programmes
            .iter()
            .find(|p| p.start_timestamp <= now && p.end_timestamp > now)
            .unwrap_or(&programmes[0]);
        show_guide(window, row, on_now);
        return true;
    }
    let current = programmes
        .iter()
        .position(|p| relative(p.start_timestamp) == state.get_guide_start());
    match key {
        GUIDE_RIGHT | GUIDE_LEFT => {
            let next = match current {
                Some(i) if key == GUIDE_RIGHT => (i + 1).min(programmes.len().saturating_sub(1)),
                Some(i) => i.saturating_sub(1),
                None => 0,
            };
            if let Some(programme) = programmes.get(next) {
                show_guide(window, row, programme);
            }
        }
        GUIDE_OPEN => {
            if let Some(programme) = current.and_then(|i| programmes.get(i)) {
                let start = programme.start_timestamp;
                state.set_guide_row(-1);
                open(window, channel, start);
            }
        }
        GUIDE_EXIT => state.set_guide_row(-1),
        _ => return false,
    }
    true
}

// Puts the guide cursor on a programme, panning 3 hours at a time until
// it's in the timeline's window.
fn show_guide(window: &AppWindow, row: i32, programme: &EPG) {
    let state = window.global::<HomeState>();
    state.set_guide_row(row);
    state.set_guide_start(relative(programme.start_timestamp));
    let now = now();
    let mut offset = with_epg(|e| e.offset);
    let original = offset;
    // The fetch covers a day either side, so this never needs many steps.
    for _ in 0..(2 * FETCH_AROUND / PAN_STEP + 1) {
        let start = now + offset - LEAD;
        if programme.end_timestamp <= start {
            offset -= PAN_STEP;
        } else if programme.start_timestamp >= start + DURATION {
            offset += PAN_STEP;
        } else {
            break;
        }
    }
    if offset != original {
        set_offset(window, offset);
    }
}

// Moves "now" on, and redraws the header's date and ruler.
fn tick(window: &AppWindow) {
    let state = window.global::<HomeState>();
    let offset = with_epg(|e| e.offset);
    let now = now();
    let panned = now + offset;
    state.set_epg_now(relative(now));
    state.set_epg_date(
        Local
            .timestamp_opt(panned, 0)
            .single()
            .map(|t| t.format("%a, %-d %b, %-I:%M %p").to_string())
            .unwrap_or_default()
            .into(),
    );
    let start = panned - LEAD;
    let first = (start + TICK - 1) / TICK * TICK;
    let ticks: Vec<EpgTick> = (0..)
        .map(|i| first + i * TICK)
        .take_while(|t| *t < start + DURATION)
        .map(|t| EpgTick {
            label: Local
                .timestamp_opt(t, 0)
                .single()
                .map(|t| t.format("%-I:%M").to_string())
                .unwrap_or_default()
                .into(),
            at: relative(t),
        })
        .collect();
    state.set_epg_ticks(ModelRc::new(VecModel::from(ticks)));
}

/// Fetches the programmes around the (panned) current time for these
/// channels, then shows them on their rows.
pub fn fetch(channels: Vec<Channel>) {
    let offset = with_epg(|e| e.offset);
    let centre = now() + offset;
    let channels: Vec<Channel> = channels
        .into_iter()
        .filter(|c| c.media_type == media_type::LIVESTREAM && c.id.is_some())
        .collect();
    if channels.is_empty() {
        return;
    }
    crate::spawn(
        crate::blocking(move || {
            Ok(channels
                .into_iter()
                .map(|c| {
                    let id = c.id.unwrap_or_default();
                    // A channel without EPG data just shows "No EPG data".
                    let programmes = api::get_epg(c, centre - FETCH_AROUND, centre + FETCH_AROUND)
                        .unwrap_or_default();
                    (id, programmes)
                })
                .collect::<Vec<_>>())
        }),
        |_, result: anyhow::Result<Vec<(i64, Vec<EPG>)>>| {
            let Ok(results) = result else { return };
            for (id, programmes) in results {
                let blocks: Vec<EpgBlock> = programmes
                    .iter()
                    .map(|p| EpgBlock {
                        title: p.title.as_str().into(),
                        start: relative(p.start_timestamp),
                        end: relative(p.end_timestamp),
                    })
                    .collect();
                with_epg(|e| e.programmes.insert(id, programmes));
                crate::home::set_epg(id, ModelRc::new(VecModel::from(blocks)));
            }
        },
    );
}

fn open(window: &AppWindow, channel: Channel, start: i64) {
    let state = window.global::<EpgState>();
    state.set_channel_name(channel.name.as_str().into());
    state.set_title("".into());
    state.set_description("".into());
    state.set_open(true);
    let fetched = channel.clone();
    crate::spawn(
        crate::blocking(move || {
            Ok((
                sql::get_epg_ids()?,
                api::get_epg_schedule(fetched).unwrap_or_default(),
            ))
        }),
        move |window, result| {
            let (reminders, schedule) = match result {
                Ok(loaded) => loaded,
                Err(e) => {
                    crate::show_error(window, &e);
                    return;
                }
            };
            let index = schedule
                .iter()
                .position(|p| p.start_timestamp == start)
                .unwrap_or(0);
            with_epg(|e| {
                e.reminders = reminders.into_iter().collect();
                e.dialog = Some(Dialog {
                    channel,
                    schedule,
                    index,
                    checked: HashSet::new(),
                    playing: false,
                });
            });
            show(window);
        },
    );
}

fn step(window: &AppWindow, by: i64) {
    let moved = with_epg(|e| {
        let dialog = e.dialog.as_mut()?;
        let index = dialog.index as i64 + by;
        if index < 0 || index >= dialog.schedule.len() as i64 {
            return None;
        }
        dialog.index = index as usize;
        Some(())
    });
    if moved.is_some() {
        show(window);
    }
}

fn show(window: &AppWindow) {
    let state = window.global::<EpgState>();
    let shown = with_epg(|e| {
        let dialog = e.dialog.as_ref()?;
        let p = dialog.schedule.get(dialog.index)?;
        let now = now();
        state.set_date(
            Local
                .timestamp_opt(p.start_timestamp, 0)
                .single()
                .map(|t| t.format("%B %-d").to_string())
                .unwrap_or_default()
                .into(),
        );
        state.set_title(p.title.as_str().into());
        state.set_time(format!("{} - {}", p.start_time, p.end_time).into());
        state.set_description(p.description.as_str().into());
        state.set_now_playing(p.start_timestamp <= now && now < p.end_timestamp);
        state.set_has_archive(p.has_archive && p.timeshift_url.is_some());
        state.set_reminder(e.reminders.contains(&p.epg_id));
        state.set_can_prev(dialog.index > 0);
        state.set_can_next(dialog.index + 1 < dialog.schedule.len());
        Some(())
    });
    if shown.is_none() {
        state.set_title("No programme information".into());
        state.set_can_prev(false);
        state.set_can_next(false);
        state.set_has_archive(false);
        return;
    }
    refresh_download(window);
    check_catch_up();
}

// A past programme on a catch-up channel can be played from the
// provider's archive; the URL is built once per programme shown.
fn check_catch_up() {
    let request = with_epg(|e| {
        let dialog = e.dialog.as_mut()?;
        let index = dialog.index;
        let p = dialog.schedule.get(index)?;
        if p.has_archive
            || dialog.channel.tv_archive != Some(true)
            || p.start_timestamp > now()
            || !dialog.checked.insert(index)
        {
            return None;
        }
        Some((
            dialog.channel.clone(),
            index,
            p.start_timestamp,
            p.end_timestamp,
        ))
    });
    let Some((channel, index, start, end)) = request else {
        return;
    };
    crate::spawn(
        xtream::get_timeshift_url_for_epg(channel, start, end),
        move |window, result| {
            // No archive for this programme: nothing changes.
            let Ok(url) = result else { return };
            with_epg(|e| {
                if let Some(p) = e.dialog.as_mut().and_then(|d| d.schedule.get_mut(index)) {
                    p.timeshift_url = Some(url);
                    p.has_archive = true;
                }
            });
            show(window);
        },
    );
}

// The channel the player or downloader is given for a catch-up programme.
fn catch_up_channel(channel: &Channel, programme: &EPG, id: i64) -> Option<Channel> {
    Some(Channel {
        id: Some(id),
        name: programme.title.clone(),
        url: Some(programme.timeshift_url.clone()?),
        group: None,
        image: None,
        media_type: media_type::MOVIE,
        source_id: channel.source_id,
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
        rating: None,
    })
}

fn play_catch_up() {
    let request = with_epg(|e| {
        let dialog = e.dialog.as_mut()?;
        if dialog.playing {
            return None;
        }
        let p = dialog.schedule.get(dialog.index)?;
        let channel = catch_up_channel(&dialog.channel, p, -1)?;
        dialog.playing = true;
        Some(channel)
    });
    let Some(channel) = request else { return };
    crate::spawn(api::play(channel, false, None, &STATE), |window, result| {
        with_epg(|e| {
            if let Some(dialog) = e.dialog.as_mut() {
                dialog.playing = false;
            }
        });
        if let Err(e) = result {
            crate::show_error(window, &e);
        }
    });
}

// (download id, channel, suggested file name) for the programme shown.
fn current_download() -> Option<(String, Channel, String)> {
    with_epg(|e| {
        let dialog = e.dialog.as_ref()?;
        let p = dialog.schedule.get(dialog.index)?;
        let channel_id = dialog.channel.id?;
        let channel = catch_up_channel(&dialog.channel, p, channel_id)?;
        let date = Local::now().format("%Y-%m-%d-%H-%M-%S");
        let file_name =
            crate::file_name::for_stream(&format!("{}_{date}", p.title), channel.url.as_deref()?);
        Some((format!("{channel_id}-{}", p.epg_id), channel, file_name))
    })
}

fn download_catch_up() {
    let Some((id, channel, file_name)) = current_download() else {
        return;
    };
    let ask = crate::downloads::ask_where_to_save();
    crate::spawn(
        async move {
            let path = if ask {
                let picked = rfd::AsyncFileDialog::new()
                    .set_title("Select where to save catchback")
                    .set_file_name(file_name)
                    .save_file()
                    .await;
                let Some(picked) = picked else { return Ok(()) };
                Some(picked.path().to_string_lossy().into_owned())
            } else {
                None
            };
            crate::downloads::download(id, channel, path).await;
            Ok(())
        },
        |window, result| {
            if let Err(e) = result {
                crate::show_error(window, &e);
            }
        },
    );
}

/// Shows the catch-up download's progress in the dialog, if it's open.
pub fn refresh_download(window: &AppWindow) {
    let state = window.global::<EpgState>();
    if !state.get_open() {
        return;
    }
    let progress = current_download().and_then(|(id, _, _)| {
        crate::downloads::progress_by_id()
            .into_iter()
            .find(|(d, _)| *d == id)
            .map(|(_, p)| p)
    });
    state.set_downloading(progress.is_some());
    state.set_progress((progress.unwrap_or(0.0) / 100.0) as f32);
}

fn toggle_reminder() {
    let request = with_epg(|e| {
        let dialog = e.dialog.as_ref()?;
        let p = dialog.schedule.get(dialog.index)?;
        Some((
            e.reminders.contains(&p.epg_id),
            EPGNotify {
                epg_id: p.epg_id.clone(),
                title: p.title.clone(),
                start_timestamp: p.start_timestamp,
                channel_name: dialog.channel.name.clone(),
            },
        ))
    });
    let Some((set, notify)) = request else { return };
    crate::spawn(
        async move {
            if set {
                epg::remove_epg(&STATE, crate::events(), notify.epg_id).await?;
            } else {
                epg::add_epg(&STATE, crate::events(), notify).await?;
            }
            crate::blocking(sql::get_epg_ids).await
        },
        move |window, result| match result {
            Ok(ids) => {
                with_epg(|e| e.reminders = ids.into_iter().collect());
                crate::show_toast(
                    window,
                    if set {
                        "Reminder removed"
                    } else {
                        "You'll be reminded when it starts"
                    },
                );
                show(window);
            }
            Err(e) => crate::show_error(window, &e),
        },
    );
}
