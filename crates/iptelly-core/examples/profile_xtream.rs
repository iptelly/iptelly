// A standalone, single-shot driver for profiling (flamegraphs etc.) - unlike
// benches/xtream_parse.rs, which repeats this pipeline many times under
// criterion's harness (ideal for timing, noisy for profiling since a
// sampler then blends warmup with many separate iterations), this runs it
// exactly once so a profiler's output reflects one clean call.
//
// Run with: cargo flamegraph --release --no-default-features --example profile_xtream

use iptelly_core::{media_type, source_type, sql, types::Source, xtream};

const CHANNEL_COUNT: usize = 500_000;
const CATEGORY_COUNT: usize = 20_000;

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
        name: "profile_xtream fixture".to_string(),
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

fn main() {
    let db_path = std::env::temp_dir().join("open_tv_xtream_profile.sqlite");
    let _ = std::fs::remove_file(&db_path);
    // Safety: nothing else in this process touches OPEN_TV_DB_PATH, and
    // this runs before the first sql:: call, which is what actually
    // initializes the connection pool against it.
    unsafe {
        std::env::set_var("OPEN_TV_DB_PATH", &db_path);
    }
    sql::create_or_initialize_db().expect("failed to initialize profiling database");

    let streams_json = generate_streams_json();
    let categories_json = generate_categories_json();

    // A single ~3s call only gives a sampling profiler a few thousand
    // samples to work with - repeating it (wiping the tables between runs,
    // same reasoning as benches/xtream_parse.rs's iter_batched setup) gives
    // a cleaner, better-resolved flamegraph without changing what's on the
    // hot path.
    const REPEATS: u32 = 5;
    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(1000)
        .build()
        .expect("failed to start profiler");

    let start = std::time::Instant::now();
    for _ in 0..REPEATS {
        let mut conn = sql::get_conn().unwrap();
        conn.execute_batch("DELETE FROM channels; DELETE FROM groups;")
            .unwrap();
        let tx = conn.transaction().unwrap();
        let mut source = fixture_source();
        source.id = Some(sql::create_or_find_source_by_name(&tx, &source).unwrap());
        xtream::process_xtream_json(
            &tx,
            &streams_json,
            &categories_json,
            &source,
            media_type::LIVESTREAM,
        )
        .unwrap();
        tx.commit().unwrap();
    }
    println!(
        "process_xtream_json (parse + load) took {:?}/iteration over {REPEATS} iterations",
        start.elapsed() / REPEATS
    );

    let flamegraph_path =
        std::env::var("FLAMEGRAPH_OUTPUT").unwrap_or_else(|_| "xtream_flamegraph.svg".to_string());
    match guard.report().build() {
        Ok(report) => {
            let file =
                std::fs::File::create(&flamegraph_path).expect("failed to create flamegraph file");
            report.flamegraph(file).expect("failed to write flamegraph");
            println!("Wrote flamegraph to {flamegraph_path}");
        }
        Err(e) => eprintln!("Failed to build profiler report: {e:?}"),
    }

    let _ = std::fs::remove_file(&db_path);
}
