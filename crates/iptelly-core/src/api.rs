// Operations that combine several modules or touch AppState. Anything that's a single call into one
// module (most of sql::, settings::, share::, ...) is called directly instead.

use anyhow::{Context, Result};
use tokio::sync::Mutex;

use crate::{
    mpv, settings, sql,
    types::{AppState, Channel, CustomChannel, EPG, Filters, Group},
    utils, vlc, xmltv,
};

pub async fn play(
    channel: Channel,
    record: bool,
    record_path: Option<String>,
    state: &Mutex<AppState>,
) -> Result<()> {
    let player = settings::get_settings()?
        .player
        .unwrap_or_else(|| "mpv".to_string());
    // Recording always goes through mpv regardless of the chosen player -
    // it's the only one with a working --stream-record equivalent.
    if record || player != "vlc" {
        mpv::play(channel, record, record_path, state).await
    } else {
        vlc::play(channel, state).await
    }
}

pub async fn search(filters: Filters, state: &Mutex<AppState>) -> Result<Vec<Channel>> {
    let hide_adult = settings::has_adult_pin()? && !state.lock().await.adult_unlocked;
    sql::search(filters, hide_adult)
}

pub async fn verify_adult_pin(pin: &str, state: &Mutex<AppState>) -> Result<bool> {
    let correct = settings::verify_adult_pin(pin)?;
    if correct {
        state.lock().await.adult_unlocked = true;
    }
    Ok(correct)
}

pub async fn lock_adult_content(state: &Mutex<AppState>) {
    state.lock().await.adult_unlocked = false;
}

pub fn add_custom_channel(channel: CustomChannel) -> Result<()> {
    sql::do_tx(|tx| sql::add_custom_channel(tx, channel))
}

pub fn add_custom_source(name: String) -> Result<()> {
    sql::do_tx(|tx| sql::create_or_find_source_by_name(tx, &mut sql::get_custom_source(name)))?;
    Ok(())
}

pub fn add_custom_group(group: Group) -> Result<()> {
    sql::do_tx(|tx| {
        sql::add_custom_group(tx, group)?;
        Ok(())
    })
}

pub fn clear_cancelled_downloads() -> Result<()> {
    // 'paused' included since the Cancelled view now shows those too (see
    // reconcile_interrupted_downloads) - "Clear all" there should clear
    // everything actually visible in that list.
    sql::clear_download_history_by_status(&["cancelled", "failed", "paused"])
}

// Timeline display only ever needs a window of a day or two around wherever
// the timeline is currently panned to (FETCH_AROUND in the app's epg.rs) -
// not the whole retention range, which is only needed by the EPG dialog's
// prev/next paging (get_epg_schedule below). Fetching the full range for every
// channel tile as it scrolls into view was the main cost behind slow
// scrolling through the channel grid.
pub fn get_epg(channel: Channel, start_timestamp: i64, end_timestamp: i64) -> Result<Vec<EPG>> {
    let (source_id, normalized) = epg_key(&channel)?;
    let (min_ts, max_ts) = epg_read_bounds(source_id)?;
    let from_ts = start_timestamp.max(min_ts);
    let to_ts = end_timestamp.min(max_ts);
    sql::get_epg_for_channel(source_id, &normalized, from_ts, to_ts)
}

// Full retention-window fetch, for the EPG modal's prev/next paging through
// a single channel's whole kept schedule - only called once, when the
// modal actually opens, rather than for every visible timeline row.
pub fn get_epg_schedule(channel: Channel) -> Result<Vec<EPG>> {
    let (source_id, normalized) = epg_key(&channel)?;
    let (from_ts, to_ts) = epg_read_bounds(source_id)?;
    sql::get_epg_for_channel(source_id, &normalized, from_ts, to_ts)
}

const EPG_READ_LOOKAHEAD_SECONDS: i64 = 7 * 24 * 60 * 60;

// How far back/forward reads are allowed to reach - has to match how far
// back was actually kept (see xmltv::refresh_epg_from_url/prune_old_epg)
// - otherwise panning the timeline back further than retention would just
// show nothing even for programmes that are genuinely still stored, and a
// too-wide request would just waste work re-reading data that was pruned.
fn epg_read_bounds(source_id: i64) -> Result<(i64, i64)> {
    let now = chrono::Utc::now().timestamp();
    let source = sql::get_source_from_id(source_id)?;
    let retention_days = source
        .epg_retention_days
        .map(|d| d as i64)
        .unwrap_or(xmltv::DEFAULT_EPG_RETENTION_DAYS);
    let lookback = retention_days * 24 * 60 * 60;
    Ok((now - lookback, now + EPG_READ_LOOKAHEAD_SECONDS))
}

// Custom-guide and bulk-fetched Xtream EPG (see
// xtream::refresh_xtream_epg) both land in the same epg_programmes table,
// keyed the same way (normalized tvg_id), so there's only one read path
// regardless of source type - no per-channel live fetch here at all (see
// xtream::get_timeshift_url_for_epg for how catch-up URLs get built on
// demand instead).
fn epg_key(channel: &Channel) -> Result<(i64, String)> {
    let source_id = channel.source_id.context("no source id")?;
    let tvg_id = channel
        .tvg_id
        .as_ref()
        .context("No EPG data for this channel")?;
    Ok((source_id, utils::normalize_tvg_id(tvg_id)))
}
