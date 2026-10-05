// Proves Settings > Export data / Import data (app_data.rs) round-trips
// everything it claims to: a source's own config, its favourited/hidden/
// last-watched channels and hidden categories, general settings, the adult
// PIN (as its hash, never needing the plaintext again) and download
// history. The test captures a "before" snapshot, exports it, mutates
// every one of those away, imports the export back, and checks everything
// came back - rather than just checking the export file's shape.

use iptelly_core::{
    app_data, m3u, media_type, settings, source_type,
    sql::{self},
    types::{ChannelPreserve, DownloadHistoryItem, Source},
    xtream,
};

const CHANNEL_COUNT: usize = 6;
const CATEGORY_COUNT: usize = 2;

fn test_db_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "open_tv_app_data_test_{}.sqlite",
        std::process::id()
    ))
}

fn export_file_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("open_tv_app_data_test_{}.otva", std::process::id()))
}

fn generate_categories_json() -> String {
    let cats: Vec<serde_json::Value> = (0..CATEGORY_COUNT)
        .map(|c| {
            serde_json::json!({
                "category_id": c.to_string(),
                "category_name": format!("Category {c}"),
            })
        })
        .collect();
    serde_json::to_string(&cats).unwrap()
}

fn generate_streams_json() -> String {
    let streams: Vec<serde_json::Value> = (0..CHANNEL_COUNT)
        .map(|i| {
            let category = i % CATEGORY_COUNT;
            serde_json::json!({
                "stream_id": i,
                "name": format!("Channel {i}"),
                "category_id": category.to_string(),
            })
        })
        .collect();
    serde_json::to_string(&streams).unwrap()
}

fn fixture_source() -> Source {
    Source {
        id: None,
        name: "app_data_test fixture".to_string(),
        url: Some("http://panel.example/".to_string()),
        url_origin: Some("http://panel.example".to_string()),
        username: Some("user".to_string()),
        password: Some("pass".to_string()),
        source_type: source_type::XTREAM,
        use_tvg_id: None,
        enabled: true,
        user_agent: None,
        max_streams: None,
        stream_user_agent: None,
        last_updated: None,
        epg_url: None,
        timezone: None,
        epg_retention_days: None,
        output_format: None,
        include_live: None,
        include_vod: None,
    }
}

fn blank_settings() -> iptelly_core::types::Settings {
    iptelly_core::types::Settings {
        recording_path: None,
        mpv_params: None,
        use_stream_caching: None,
        default_view: None,
        volume: None,
        refresh_on_start: None,
        restream_port: None,
        enable_tray_icon: None,
        zoom: None,
        default_sort: None,
        enable_hwdec: None,
        always_ask_save: None,
        enable_gpu: None,
        theme: None,
        player: None,
        vlc_params: None,
        network_interface: None,
        lightweight_mode: None,
    }
}

#[test]
fn export_then_import_restores_everything_it_backed_up() {
    let db_path = test_db_path();
    let export_path = export_file_path();
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&export_path);
    // Safety: this test's process never touches OPEN_TV_DB_PATH again after
    // this point, and sql::CONN isn't initialized until the first sql::
    // call below, so there's no race with another read of the env var.
    unsafe {
        std::env::set_var("OPEN_TV_DB_PATH", &db_path);
    }
    sql::create_or_initialize_db().expect("failed to initialize test database");

    let mut source = fixture_source();
    {
        let mut conn = sql::get_conn().unwrap();
        let tx = conn.transaction().unwrap();
        source.id = Some(sql::create_or_find_source_by_name(&tx, &source).unwrap());
        xtream::process_xtream_json(
            &tx,
            &generate_streams_json(),
            &generate_categories_json(),
            &source,
            media_type::LIVESTREAM,
        )
        .unwrap();
        tx.commit().unwrap();
    }
    let source_id = source.id.unwrap();

    let (fav_id, hidden_channel_id, watched_id, hidden_group_id): (i64, i64, i64, i64) = {
        let conn = sql::get_conn().unwrap();
        let get_channel_id = |name: &str| -> i64 {
            conn.query_row("SELECT id FROM channels WHERE name = ?", [name], |r| {
                r.get(0)
            })
            .unwrap()
        };
        let group_id: i64 = conn
            .query_row(
                "SELECT id FROM groups WHERE name = ?",
                ["Category 1"],
                |r| r.get(0),
            )
            .unwrap();
        (
            get_channel_id("Channel 0"),
            get_channel_id("Channel 1"),
            get_channel_id("Channel 2"),
            group_id,
        )
    };

    sql::favorite_channel(fav_id, true).unwrap();
    sql::hide_channel(hidden_channel_id, true).unwrap();
    sql::add_last_watched(watched_id).unwrap();
    sql::hide_group(hidden_group_id, true).unwrap();

    let mut backed_up_settings = blank_settings();
    backed_up_settings.theme = Some("classic".to_string());
    backed_up_settings.volume = Some(42);
    settings::update_settings(backed_up_settings.clone()).unwrap();
    settings::set_adult_pin(Some("1234".to_string())).unwrap();
    let pin_hash_before = settings::get_adult_pin_hash().unwrap();
    assert!(pin_hash_before.is_some());

    let download = DownloadHistoryItem {
        id: "app_data_test-dl".to_string(),
        channel_id: Some(fav_id),
        source_id: Some(source_id),
        name: "Some Movie".to_string(),
        path: "/tmp/some_movie.mp4".to_string(),
        status: "completed".to_string(),
        downloaded_bytes: 123_456,
        total_bytes: Some(123_456),
        created_at: 1_700_000_000,
        updated_at: 1_700_000_001,
    };
    sql::upsert_download_row(&download).unwrap();

    app_data::export_app_data(export_path.to_string_lossy().to_string())
        .expect("export should succeed");

    // Mutate every piece of state the export captured, away from what was
    // backed up, so a successful import is the only way the assertions
    // below can pass.
    sql::favorite_channel(fav_id, false).unwrap();
    sql::hide_channel(hidden_channel_id, false).unwrap();
    sql::remove_last_watched(watched_id).unwrap();
    sql::hide_group(hidden_group_id, false).unwrap();
    let mut other_settings = blank_settings();
    other_settings.theme = Some("modern".to_string());
    other_settings.volume = Some(1);
    settings::update_settings(other_settings).unwrap();
    settings::set_adult_pin(None).unwrap();
    assert!(!settings::has_adult_pin().unwrap());
    sql::delete_download_row(&download.id).unwrap();
    assert!(sql::get_download_row(&download.id).unwrap().is_none());

    app_data::import_app_data(export_path.to_string_lossy().to_string())
        .expect("import should succeed");

    let restored_channel = sql::get_channel_by_id(fav_id).unwrap();
    assert!(restored_channel.favorite, "favourite should be restored");
    let restored_hidden = sql::get_channel_by_id(hidden_channel_id).unwrap();
    assert_eq!(
        restored_hidden.hidden,
        Some(true),
        "hidden channel should be restored"
    );
    let conn = sql::get_conn().unwrap();
    let last_watched: Option<i64> = conn
        .query_row(
            "SELECT last_watched FROM channels WHERE id = ?",
            [watched_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(last_watched.is_some(), "history entry should be restored");

    let group_hidden: bool = conn
        .query_row(
            "SELECT hidden FROM groups WHERE id = ?",
            [hidden_group_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(group_hidden, "hidden category should be restored");
    drop(conn);

    let restored_settings = settings::get_settings().unwrap();
    assert_eq!(restored_settings.theme, Some("classic".to_string()));
    assert_eq!(restored_settings.volume, Some(42));

    assert!(settings::has_adult_pin().unwrap());
    assert_eq!(settings::get_adult_pin_hash().unwrap(), pin_hash_before);

    let restored_download = sql::get_download_row(&download.id)
        .unwrap()
        .expect("download history row should be restored");
    assert_eq!(restored_download.name, download.name);
    assert_eq!(restored_download.path, download.path);
    assert_eq!(restored_download.status, download.status);
    assert_eq!(
        restored_download.downloaded_bytes,
        download.downloaded_bytes
    );

    // Reproduces the real bug a user hit: importing a backup onto a source
    // whose channels don't exist yet (e.g. right after a fresh install, or
    // a brand new source the backup describes) can't restore_preserve
    // anything immediately - there's nothing to match by name yet. Proves
    // the pending_preserve catch-up actually fixes it: stage a favourite/
    // hidden entry for a source with zero channels, then run the exact
    // code path a real "Refresh all" click runs (m3u::read_m3u8 with
    // wipe=true) and confirm it lands once the channel actually exists.
    let mut fresh_source = fixture_source();
    fresh_source.name = "pending_preserve_test fixture".to_string();
    fresh_source.source_type = source_type::M3U;
    let fresh_source_id = sql::do_tx(|tx| sql::import_source_full(tx, &fresh_source)).unwrap();
    fresh_source.id = Some(fresh_source_id);

    sql::do_tx(|tx| {
        sql::set_pending_preserve(
            tx,
            fresh_source_id,
            &[ChannelPreserve {
                name: "Fresh Channel".to_string(),
                favorite: true,
                last_watched: None,
                hidden: Some(true),
                is_group: false,
                headers: None,
            }],
        )
    })
    .unwrap();

    let m3u_path =
        std::env::temp_dir().join(format!("open_tv_app_data_test_{}.m3u8", std::process::id()));
    std::fs::write(
        &m3u_path,
        "#EXTM3U\n#EXTINF:-1,Fresh Channel\nhttp://example.com/fresh.ts\n",
    )
    .unwrap();
    fresh_source.url = Some(m3u_path.to_string_lossy().to_string());

    m3u::read_m3u8(fresh_source, true).expect("refresh should succeed");

    let conn = sql::get_conn().unwrap();
    let (restored_favorite, restored_hidden): (bool, Option<bool>) = conn
        .query_row(
            "SELECT favorite, hidden FROM channels WHERE name = 'Fresh Channel' AND source_id = ?",
            [fresh_source_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(
        restored_favorite,
        "favourite staged before the source had any channels should apply on its first refresh"
    );
    assert_eq!(restored_hidden, Some(true));

    let still_pending: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pending_preserve WHERE source_id = ?",
            [fresh_source_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        still_pending, 0,
        "staged data should be consumed, not left to reapply forever"
    );
    drop(conn);

    let _ = std::fs::remove_file(&m3u_path);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&export_path);
}
