// The details panel at the top of the Movies and Series views, like
// TiviMate's: the highlighted movie's or series' poster, rating, year,
// length, genre, cast, director and plot over its backdrop. The details come
// from the provider's Xtream info page, so other sources only have a title,
// poster and rating.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use iptelly_core::types::{Channel, MediaInfo};
use iptelly_core::{media_type, xtream};
use slint::{ComponentHandle, Image, Timer, TimerMode};

use crate::{AppWindow, HomeState, MediaDetails};

// Moving through the grid only fetches once the highlight rests.
const DELAY: Duration = Duration::from_millis(300);
const POSTER_SIZE: u32 = 480;
const BACKDROP_SIZE: u32 = 1280;
// Past this many fetched details the cache is simply emptied.
const CACHE_LIMIT: usize = 500;

#[derive(Default)]
struct Details {
    timer: Timer,
    // The channel id the panel is showing.
    shown: Option<i64>,
    info: HashMap<i64, MediaInfo>,
}

thread_local! {
    static DETAILS: RefCell<Details> = RefCell::new(Details::default());
}

fn with_details<R>(f: impl FnOnce(&mut Details) -> R) -> R {
    DETAILS.with_borrow_mut(f)
}

/// Shows a movie or series in the panel, or empties it. Its details are
/// fetched from Xtream (`xtream`) once it's been highlighted for a moment.
pub fn show(window: &AppWindow, channel: Option<Channel>, xtream: bool) {
    let state = window.global::<HomeState>();
    let Some(channel) = channel else {
        with_details(|d| d.shown = None);
        state.set_details(MediaDetails::default());
        return;
    };
    let Some(id) = channel.id else { return };
    if with_details(|d| d.shown == Some(id)) {
        return;
    }
    let cached = with_details(|d| {
        d.shown = Some(id);
        d.info.get(&id).cloned()
    });
    state.set_details(details_for(&channel, cached.as_ref()));

    with_details(|d| {
        d.timer.start(TimerMode::SingleShot, DELAY, move || {
            load_images(&channel, cached.as_ref(), Kind::Poster);
            match &cached {
                Some(info) => load_images(&channel, Some(info), Kind::Backdrop),
                None if xtream => fetch(channel.clone()),
                None => {}
            }
        })
    });
}

/// Forgets the fetched details, for after a playlist update.
pub fn clear_cache() {
    with_details(|d| d.info.clear());
}

fn fetch(channel: Channel) {
    let Some(id) = channel.id else { return };
    crate::spawn(
        xtream::get_media_info(channel.clone()),
        move |window, result| {
            // A provider without the info page just leaves the basics.
            let Ok(info) = result else { return };
            with_details(|d| {
                if d.info.len() >= CACHE_LIMIT {
                    d.info.clear();
                }
                d.info.insert(id, info.clone());
            });
            if !is_shown(id) {
                return;
            }
            let state = window.global::<HomeState>();
            let mut details = details_for(&channel, Some(&info));
            // Keeps the poster that may have arrived already.
            let current = state.get_details();
            details.poster = current.poster;
            details.has_poster = current.has_poster;
            state.set_details(details);
            load_images(&channel, Some(&info), Kind::Backdrop);
        },
    );
}

#[derive(Clone, Copy)]
enum Kind {
    Poster,
    Backdrop,
}

fn load_images(channel: &Channel, info: Option<&MediaInfo>, kind: Kind) {
    let (url, size) = match kind {
        Kind::Poster => (channel.image.clone(), POSTER_SIZE),
        Kind::Backdrop => (info.and_then(|i| i.backdrop.clone()), BACKDROP_SIZE),
    };
    let (Some(url), Some(id)) = (url.filter(|u| !u.is_empty()), channel.id) else {
        return;
    };
    crate::spawn(
        crate::images::load_sized(url, size),
        move |window, result| {
            // A missing or broken image just leaves the panel without it.
            let Ok(buffer) = result else { return };
            if !is_shown(id) {
                return;
            }
            let state = window.global::<HomeState>();
            let mut details = state.get_details();
            let image = Image::from_rgba8(buffer);
            match kind {
                Kind::Poster => {
                    details.poster = image;
                    details.has_poster = true;
                }
                Kind::Backdrop => {
                    details.backdrop = image;
                    details.has_backdrop = true;
                }
            }
            state.set_details(details);
        },
    );
}

fn is_shown(id: i64) -> bool {
    with_details(|d| d.shown == Some(id))
}

/// The panel's text for a movie or series, without its images.
fn details_for(channel: &Channel, info: Option<&MediaInfo>) -> MediaDetails {
    let text = |value: Option<&String>| value.cloned().unwrap_or_default().into();
    let rating = info.and_then(|i| i.rating).or(channel.rating);
    MediaDetails {
        title: channel.name.as_str().into(),
        rating: rating.map(format_rating).unwrap_or_default().into(),
        line: info.map(details_line).unwrap_or_default().into(),
        cast: text(info.and_then(|i| i.cast.as_ref())),
        director: text(info.and_then(|i| i.director.as_ref())),
        plot: text(info.and_then(|i| i.plot.as_ref())),
        playable: channel.media_type == media_type::MOVIE,
        ..Default::default()
    }
}

pub fn format_rating(rating: f64) -> String {
    format!("{rating:.1}")
}

/// "2025 • 1h 45m • Thriller", leaving out what the provider didn't give.
fn details_line(info: &MediaInfo) -> String {
    [
        info.year.clone(),
        info.duration_secs.map(format_duration),
        info.genre.clone(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" • ")
}

/// "1h 45m", "45m" or "2h".
fn format_duration(seconds: u64) -> String {
    let minutes = (seconds + 30) / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::window;
    use i_slint_backend_testing::ElementHandle;

    fn movie(rating: Option<f64>) -> Channel {
        Channel {
            id: Some(7),
            name: "The Fires".to_string(),
            url: Some("http://example.com/7.mp4".to_string()),
            group: None,
            image: None,
            media_type: media_type::MOVIE,
            source_id: Some(1),
            series_id: None,
            group_id: None,
            favorite: false,
            stream_id: Some(7),
            tv_archive: None,
            season_id: None,
            episode_num: None,
            hidden: None,
            tvg_id: None,
            is_adult: false,
            rating,
        }
    }

    #[test]
    fn durations() {
        assert_eq!(format_duration(6300), "1h 45m");
        assert_eq!(format_duration(2700), "45m");
        assert_eq!(format_duration(7200), "2h");
    }

    #[test]
    fn the_details_leave_out_what_the_provider_left_out() {
        let info = MediaInfo {
            plot: Some("A volcanologist faces two disasters.".to_string()),
            cast: Some("Vigdís Hrefna Pálsdóttir".to_string()),
            year: Some("2025".to_string()),
            duration_secs: Some(6300),
            genre: Some("Thriller".to_string()),
            ..Default::default()
        };
        let details = details_for(&movie(Some(6.5)), Some(&info));
        assert_eq!(details.title, "The Fires");
        assert_eq!(details.rating, "6.5");
        assert_eq!(details.line, "2025 • 1h 45m • Thriller");
        assert_eq!(details.cast, "Vigdís Hrefna Pálsdóttir");
        assert_eq!(details.director, "");
        assert_eq!(details.plot, "A volcanologist faces two disasters.");
    }

    #[test]
    fn without_an_info_page_it_has_the_title_and_rating() {
        let details = details_for(&movie(Some(8.0)), None);
        assert_eq!(details.title, "The Fires");
        assert_eq!(details.rating, "8.0");
        assert_eq!(details.line, "");
    }

    #[test]
    fn the_panel_shows_the_details() {
        let window = window();
        window.set_page(crate::Page::Home);
        let state = window.global::<HomeState>();
        state.set_rail(crate::Rail::Movies);
        state.set_show_details(true);
        show(&window, Some(movie(Some(6.5))), false);
        assert_eq!(state.get_details().title, "The Fires");
        let texts: Vec<String> = ElementHandle::find_by_element_type_name(&window, "Text")
            .filter_map(|t| t.accessible_label())
            .map(|t| t.to_string())
            .collect();
        assert!(texts.iter().any(|t| t == "The Fires"), "{texts:?}");
        assert!(texts.iter().any(|t| t == "6.5"), "{texts:?}");

        show(&window, None, false);
        assert_eq!(state.get_details().title, "");
    }
}
