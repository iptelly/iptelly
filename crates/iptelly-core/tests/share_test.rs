// Sharing custom channels, groups and playlists (share.rs): everything
// exported should import back unchanged - into another source for channels
// and groups, or as a new source for a whole playlist - against a
// throwaway database. Tests share it, so each one uses its own uniquely
// named sources.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, Once, PoisonError};

use iptelly_core::{
    media_type, share, sql,
    types::{Channel, ChannelHttpHeaders, CustomChannel, ExportedGroup, Group, Source},
};
use rusqlite::params;

static INIT: Once = Once::new();
static LOCK: Mutex<()> = Mutex::new(());

fn setup() -> MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    INIT.call_once(|| {
        let db_path =
            std::env::temp_dir().join(format!("iptelly_share_test_{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&db_path);
        // Safety: this runs once, before the first sql:: call, while every
        // other test is blocked on LOCK - nothing else reads the
        // environment concurrently.
        unsafe {
            std::env::set_var("OPEN_TV_DB_PATH", &db_path);
        }
        sql::create_or_initialize_db().expect("failed to initialize test database");
    });
    guard
}

fn export_path(name: &str, extension: &str) -> String {
    let path: PathBuf = std::env::temp_dir().join(format!(
        "iptelly_share_test_{}_{name}.{extension}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    path.to_string_lossy().to_string()
}

fn add_source(name: &str) -> Source {
    let mut source = sql::get_custom_source(name.to_string());
    source.id = Some(sql::do_tx(|tx| sql::create_or_find_source_by_name(tx, &source)).unwrap());
    source
}

fn find_source(name: &str) -> Option<Source> {
    sql::get_sources()
        .unwrap()
        .into_iter()
        .find(|s| s.name == name)
}

fn channel(name: &str, url: &str) -> Channel {
    Channel {
        id: None,
        name: name.to_string(),
        url: Some(url.to_string()),
        group: None,
        image: Some(format!("{url}.png")),
        media_type: media_type::LIVESTREAM,
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

fn headers() -> ChannelHttpHeaders {
    ChannelHttpHeaders {
        id: None,
        channel_id: None,
        referrer: Some("https://referrer.example".to_string()),
        user_agent: Some("Shared UA".to_string()),
        http_origin: Some("https://origin.example".to_string()),
        ignore_ssl: Some(true),
    }
}

// Adds the channel to the source (and group, if given) and returns it with
// its database id, the way the UI passes it to share_custom_channel.
fn add_channel(
    source: &Source,
    group_id: Option<i64>,
    mut channel: Channel,
    headers: Option<ChannelHttpHeaders>,
) -> Channel {
    let source_id = source.id.unwrap();
    channel.source_id = Some(source_id);
    channel.group_id = group_id;
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

fn add_group(source: &Source, name: &str) -> i64 {
    sql::do_tx(|tx| {
        sql::add_custom_group(
            tx,
            Group {
                id: None,
                name: name.to_string(),
                image: Some("https://example.com/group.png".to_string()),
                source_id: source.id,
                hidden: None,
                media_type: None,
            },
        )
    })
    .unwrap()
}

// The group as the UI passes it to share_custom_group - a Channel
// row standing in for the group.
fn group_as_channel(source: &Source, group_id: i64, name: &str) -> Channel {
    Channel {
        id: Some(group_id),
        source_id: source.id,
        image: Some("https://example.com/group.png".to_string()),
        media_type: media_type::GROUP,
        ..channel(name, "")
    }
}

fn ungrouped_channels(source: &Source) -> Vec<CustomChannel> {
    let mut channels = sql::get_custom_channels(None, source.id.unwrap()).unwrap();
    channels.sort_by(|a, b| a.data.name.cmp(&b.data.name));
    channels
}

fn groups(source: &Source) -> Vec<ExportedGroup> {
    let mut groups = sql::get_custom_groups(source.id.unwrap()).unwrap();
    groups.sort_by(|a, b| a.group.name.cmp(&b.group.name));
    for group in &mut groups {
        group.channels.sort_by(|a, b| a.data.name.cmp(&b.data.name));
    }
    groups
}

#[test]
fn a_shared_channel_imports_with_its_headers() {
    let _db = setup();
    let from = add_source("channel from");
    let to = add_source("channel to");
    let news = add_channel(
        &from,
        None,
        channel("News", "http://example.com/news.m3u8"),
        Some(headers()),
    );
    let path = export_path("channel", "otv");

    share::share_custom_channel(news, path.clone()).unwrap();
    share::import(path, to.id, None).unwrap();

    assert_eq!(ungrouped_channels(&to), ungrouped_channels(&from));
    let imported = &ungrouped_channels(&to)[0];
    assert_eq!(imported.data.name, "News");
    assert_eq!(
        imported.headers.as_ref().unwrap().user_agent.as_deref(),
        Some("Shared UA")
    );
}

#[test]
fn a_channel_cannot_be_imported_twice_unless_renamed() {
    let _db = setup();
    let from = add_source("duplicate channel from");
    let to = add_source("duplicate channel to");
    let news = add_channel(
        &from,
        None,
        channel("News", "http://example.com/news.m3u8"),
        None,
    );
    let path = export_path("duplicate_channel", "otv");
    share::share_custom_channel(news, path.clone()).unwrap();

    share::import(path.clone(), to.id, None).unwrap();
    let error = share::import(path.clone(), to.id, None).unwrap_err();
    assert_eq!(error.to_string(), "Duplicate exists");

    share::import(path, to.id, Some("News (copy)".to_string())).unwrap();
    let names: Vec<String> = ungrouped_channels(&to)
        .into_iter()
        .map(|c| c.data.name)
        .collect();
    assert_eq!(names, ["News", "News (copy)"]);
}

#[test]
fn a_shared_group_imports_with_all_its_channels() {
    let _db = setup();
    let from = add_source("group from");
    let to = add_source("group to");
    let sports = add_group(&from, "Sports");
    add_channel(
        &from,
        Some(sports),
        channel("Football", "http://example.com/football.m3u8"),
        Some(headers()),
    );
    add_channel(
        &from,
        Some(sports),
        channel("Tennis", "http://example.com/tennis.m3u8"),
        None,
    );
    let path = export_path("group", "otvg");

    share::share_custom_group(group_as_channel(&from, sports, "Sports"), path.clone()).unwrap();
    share::import(path, to.id, None).unwrap();

    let imported = groups(&to);
    assert_eq!(imported, groups(&from));
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].channels.len(), 2);
    assert!(ungrouped_channels(&to).is_empty());
}

#[test]
fn a_group_cannot_be_imported_twice_unless_renamed() {
    let _db = setup();
    let from = add_source("duplicate group from");
    let to = add_source("duplicate group to");
    let sports = add_group(&from, "Sports");
    add_channel(
        &from,
        Some(sports),
        channel("Football", "http://example.com/football.m3u8"),
        None,
    );
    let path = export_path("duplicate_group", "otvg");
    share::share_custom_group(group_as_channel(&from, sports, "Sports"), path.clone()).unwrap();

    share::import(path.clone(), to.id, None).unwrap();
    let error = share::import(path.clone(), to.id, None).unwrap_err();
    assert_eq!(error.to_string(), "Duplicate exists");

    share::import(path, to.id, Some("More Sports".to_string())).unwrap();
    let names: Vec<String> = groups(&to).into_iter().map(|g| g.group.name).collect();
    assert_eq!(names, ["More Sports", "Sports"]);
}

#[test]
fn a_shared_playlist_imports_as_a_new_source() {
    let _db = setup();
    let original = add_source("playlist original");
    let sports = add_group(&original, "Sports");
    add_channel(
        &original,
        Some(sports),
        channel("Football", "http://example.com/football.m3u8"),
        Some(headers()),
    );
    add_channel(
        &original,
        None,
        channel("News", "http://example.com/news.m3u8"),
        None,
    );
    let path = export_path("playlist", "otvp");

    share::share_custom_source(original.clone(), path.clone()).unwrap();
    share::import(path, None, Some("playlist copy".to_string())).unwrap();

    let copy = find_source("playlist copy").expect("the playlist was not imported");
    assert_ne!(copy.id, original.id);
    assert_eq!(groups(&copy), groups(&original));
    assert_eq!(ungrouped_channels(&copy), ungrouped_channels(&original));
}

#[test]
fn a_playlist_cannot_be_imported_over_an_existing_source() {
    let _db = setup();
    let original = add_source("playlist duplicate");
    add_channel(
        &original,
        None,
        channel("News", "http://example.com/news.m3u8"),
        None,
    );
    let path = export_path("duplicate_playlist", "otvp");
    share::share_custom_source(original, path.clone()).unwrap();

    let error = share::import(path, None, None).unwrap_err();
    assert_eq!(error.to_string(), "Duplicate exists");
}

#[test]
fn an_exported_playlist_does_not_carry_its_source_id() {
    let _db = setup();
    let original = add_source("playlist without id");
    let path = export_path("playlist_without_id", "otvp");
    share::share_custom_source(original, path.clone()).unwrap();

    let exported: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(exported["source"]["name"], "playlist without id");
    assert!(exported["source"].get("id").is_none());
}

#[test]
fn rejects_unknown_files_and_missing_sources() {
    let _db = setup();
    let source = add_source("bad imports");
    let unknown = export_path("unknown", "txt");
    std::fs::write(&unknown, "{}").unwrap();
    assert!(share::import(unknown, source.id, None).is_err());

    let news = add_channel(
        &source,
        None,
        channel("News", "http://example.com/news.m3u8"),
        None,
    );
    let path = export_path("no_source", "otv");
    share::share_custom_channel(news, path.clone()).unwrap();
    assert!(share::import(path, None, None).is_err());

    let corrupt = export_path("corrupt", "otv");
    std::fs::write(&corrupt, "not json").unwrap();
    assert!(share::import(corrupt, source.id, None).is_err());
}
