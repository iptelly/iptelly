// A throwaway database for lib unit tests that need sql:: - see
// sql::db_path_override. Every unit test runs in the same process, so they
// all share sql::CONN and this one file; lock() serialises the tests that
// use it, since settings are global to the whole database.

use std::sync::{Mutex, MutexGuard, Once, PoisonError};

use rusqlite::params;

use crate::{
    sql,
    types::{Channel, ChannelHttpHeaders, CustomChannel, Season, Source},
};

static INIT: Once = Once::new();
static LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn path() -> String {
    std::env::temp_dir()
        .join(format!("iptelly_unit_test_{}.sqlite", std::process::id()))
        .to_string_lossy()
        .to_string()
}

// Hold the returned guard for the whole test. Settings are cleared each
// time, so every test starts from the defaults; other tables are left
// alone, so tests should create their own uniquely named rows.
pub(crate) fn lock() -> MutexGuard<'static, ()> {
    // A failed test panics while holding the lock - that shouldn't fail
    // every test after it too.
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    INIT.call_once(|| {
        let _ = std::fs::remove_file(path());
        sql::create_or_initialize_db().expect("failed to initialize the unit test database");
    });
    sql::get_conn()
        .unwrap()
        .execute("DELETE FROM settings", [])
        .unwrap();
    guard
}

pub(crate) fn set_setting(key: &str, value: &str) {
    sql::update_settings([(key.to_string(), Some(value.to_string()))].into()).unwrap();
}

pub(crate) fn add_source(name: &str) -> Source {
    let mut source = sql::get_custom_source(name.to_string());
    source.id = Some(sql::do_tx(|tx| sql::create_or_find_source_by_name(tx, &source)).unwrap());
    source
}

pub(crate) fn add_season(source: &Source, series_id: u64) -> i64 {
    sql::do_tx(|tx| {
        sql::insert_season(
            tx,
            Season {
                id: None,
                name: "Season 1".to_string(),
                season_number: 1,
                image: None,
                series_id,
                source_id: source.id.unwrap(),
            },
        )
    })
    .unwrap()
}

// A channel that isn't in the database yet - pass it to add_channel, or use
// it as-is where only its fields matter.
pub(crate) fn channel(name: &str, url: &str, media_type: u8) -> Channel {
    Channel {
        id: None,
        name: name.to_string(),
        url: Some(url.to_string()),
        group: None,
        image: None,
        media_type,
        source_id: None,
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

// Inserts the channel (and its headers, if any) into source and returns it
// with its new id filled in.
pub(crate) fn add_channel(
    source: &Source,
    mut channel: Channel,
    headers: Option<ChannelHttpHeaders>,
) -> Channel {
    let source_id = source.id.unwrap();
    channel.source_id = Some(source_id);
    let id = sql::do_tx(|tx| {
        sql::add_custom_channel(
            tx,
            CustomChannel {
                data: channel.clone(),
                headers,
            },
        )?;
        Ok(tx.query_row(
            "SELECT id FROM channels WHERE name = ? AND source_id = ?",
            params![channel.name, source_id],
            |row| row.get(0),
        )?)
    })
    .unwrap();
    channel.id = Some(id);
    channel
}
