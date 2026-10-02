// Benchmarks the xtream codes pipeline end to end: deserializing a raw JSON
// panel response (same shape get_xtream_http_data would hand to
// process_xtream, see tests/xtream_parser_test.rs for the correctness
// proof at a smaller scale), converting it into Channel rows, and loading
// those into a real (but throwaway) SQLite database - not just the parsing
// step on its own, since "load into a fake db" is itself part of what's
// being measured here.
//
// Run with: cargo bench --no-default-features --bench xtream_parse

use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use iptelly_lib::{media_type, source_type, sql, types::Source, xtream};

const CHANNEL_COUNT: usize = 500_000;
// The real database (601,032 channels) has only 3,489 categories across its
// 4 enabled sources - 4,000 here is close to that real-world ratio, rather
// than the much more fragmented 20,000 originally guessed at.
const CATEGORY_COUNT: usize = 4_000;

fn bench_db_path() -> std::path::PathBuf {
    std::env::temp_dir().join("open_tv_xtream_bench.sqlite")
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
        name: "xtream_bench fixture".to_string(),
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

fn bench_process_xtream_end_to_end(c: &mut Criterion) {
    let db_path = bench_db_path();
    let _ = std::fs::remove_file(&db_path);
    // Safety: nothing else in this process touches OPEN_TV_DB_PATH, and
    // this runs before the first sql:: call, which is what actually
    // initializes the connection pool against it.
    unsafe {
        std::env::set_var("OPEN_TV_DB_PATH", &db_path);
    }
    sql::create_or_initialize_db().expect("failed to initialize benchmark database");

    let streams_json = generate_streams_json();
    let categories_json = generate_categories_json();

    // Each iteration parses + loads 500k channels (likely several seconds,
    // real disk-backed SQLite writes included) - the default sample_size
    // (100) and measurement_time (5s) assume sub-millisecond iterations.
    let mut group = c.benchmark_group("xtream end-to-end");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(180));
    group.bench_function(
        format!(
            "parse + load {CHANNEL_COUNT} channels across {CATEGORY_COUNT} categories into sqlite"
        ),
        |b| {
            b.iter_batched(
                || {
                    // Untimed setup: start every iteration from an empty
                    // channels/groups table, so each sample measures a
                    // fresh load rather than an ON CONFLICT ... DO UPDATE
                    // against whatever the previous iteration already
                    // inserted - a fresh-table insert and a mass-update are
                    // different operations with different costs, and the
                    // realistic scenario (a first-time refresh) is the
                    // former.
                    let conn = sql::get_conn().unwrap();
                    conn.execute_batch("DELETE FROM channels; DELETE FROM groups;")
                        .unwrap();
                },
                |()| {
                    let mut conn = sql::get_conn().unwrap();
                    let tx = conn.transaction().unwrap();
                    let mut source = fixture_source();
                    source.id = Some(sql::create_or_find_source_by_name(&tx, &source).unwrap());
                    xtream::process_xtream_json(
                        &tx,
                        black_box(&streams_json),
                        black_box(&categories_json),
                        black_box(&source),
                        media_type::LIVESTREAM,
                    )
                    .unwrap();
                    tx.commit().unwrap();
                },
                criterion::BatchSize::PerIteration,
            )
        },
    );
    group.finish();
}

criterion_group!(benches, bench_process_xtream_end_to_end);
criterion_main!(benches);
