// Proves the xtream parser against a synthetic-but-realistic panel response
// (raw JSON text, same shape get_xtream_http_data would deserialize from a
// real HTTP response) rather than hand-built Rust structs: every channel
// generated ends up in the database with the right name/group/stream_id,
// and every category becomes exactly one row in `groups`, deduplicated
// across the channels that share it.
//
// Deliberately small (routine `cargo test` territory) - see
// benches/xtream_parse.rs for the same pipeline at the 500k-channel/
// 20k-category scale this was modeled after.

use open_tv_lib::{media_type, source_type, sql, types::Source, xtream};

const CHANNEL_COUNT: usize = 3_000;
const CATEGORY_COUNT: usize = 150;

fn test_db_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("open_tv_xtream_parser_test_{}.sqlite", std::process::id()))
}

fn generate_categories_json() -> String {
    let cats: Vec<serde_json::Value> = (0..CATEGORY_COUNT)
        .map(|c| {
            serde_json::json!({
                "category_id": c.to_string(),
                "category_name": format!("Category {c:05}"),
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
                "name": format!("Channel {i:06}"),
                "category_id": category.to_string(),
                "stream_icon": format!("https://img.example.com/logos/{i:06}.png"),
                "epg_channel_id": format!("epg.{i:06}"),
                "tv_archive": 0,
                "is_adult": 0,
            })
        })
        .collect();
    serde_json::to_string(&streams).unwrap()
}

fn fixture_source() -> Source {
    Source {
        id: None,
        name: "xtream_parser_test fixture".to_string(),
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
    }
}

#[test]
fn every_synthetic_channel_and_category_ends_up_in_the_database() {
    let db_path = test_db_path();
    let _ = std::fs::remove_file(&db_path);
    // Safety: this test's process never touches OPEN_TV_DB_PATH again after
    // this point, and sql::CONN isn't initialized until the first sql::
    // call below, so there's no race with another read of the env var.
    unsafe {
        std::env::set_var("OPEN_TV_DB_PATH", &db_path);
    }
    open_tv_lib::sql::create_or_initialize_db().expect("failed to initialize test database");

    let streams_json = generate_streams_json();
    let categories_json = generate_categories_json();
    let mut source = fixture_source();

    {
        let mut conn = sql::get_conn().unwrap();
        let tx = conn.transaction().unwrap();
        source.id = Some(sql::create_or_find_source_by_name(&tx, &source).unwrap());
        xtream::process_xtream_json(
            &tx,
            &streams_json,
            &categories_json,
            &source,
            media_type::LIVESTREAM,
        )
        .expect("processing the synthetic streams should succeed");
        tx.commit().unwrap();
    }

    let conn = sql::get_conn().unwrap();
    let channel_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM channels", [], |r| r.get(0))
        .unwrap();
    assert_eq!(channel_count as usize, CHANNEL_COUNT);

    let group_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM groups", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        group_count as usize, CATEGORY_COUNT,
        "every category should become exactly one row, deduplicated across its channels"
    );

    // Spot-check specific channels for correct field mapping, not just counts.
    for i in [0usize, 1, CHANNEL_COUNT / 2, CHANNEL_COUNT - 1] {
        let (name, group, tvg_id): (String, String, String) = conn
            .query_row(
                "SELECT c.name, g.name, c.tvg_id FROM channels c \
                 JOIN groups g ON g.id = c.group_id WHERE c.stream_id = ?",
                [i as i64],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap_or_else(|e| panic!("channel with stream_id {i} not found: {e}"));
        assert_eq!(name, format!("Channel {i:06}"));
        assert_eq!(group, format!("Category {:05}", i % CATEGORY_COUNT));
        assert_eq!(tvg_id, format!("epg.{i:06}"));
    }

    let _ = std::fs::remove_file(&db_path);
}
