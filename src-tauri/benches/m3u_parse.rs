// Benchmarks the per-channel m3u parser against the real 2471-channel
// Samsung TV Plus fixture (see tests/fixtures/ and m3u_parser_test.rs) -
// a representative large, real-world playlist rather than a synthetic one,
// so timings reflect the actual regex-heavy hot path (NAME_REGEX,
// NAME_REGEX_ALT, ID_REGEX, LOGO_REGEX, GROUP_REGEX all run per channel).
//
// Run with: cargo bench --no-default-features

use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use iptelly_lib::m3u::get_channel_from_lines;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/samsung_tvplus_playlist.m3u8"
);

// Not checked into git (~90MB, see src-tauri/.gitignore) - generated once on
// first bench run and cached here for subsequent runs. 500k channels is far
// beyond anything a real playlist fixture reaches, so this is what actually
// shows how the parser scales rather than how fast it is on one real file.
const LARGE_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/synthetic_large_playlist.m3u8"
);
const LARGE_CHANNEL_COUNT: usize = 500_000;
const LARGE_CATEGORY_COUNT: usize = 20_000;
// 5% of channels omit tvg-name entirely (real playlists sometimes do, e.g.
// the Samsung fixture has zero tvg-name attributes at all), forcing
// NAME_REGEX_ALT's comma-separated fallback instead of the plain NAME_REGEX
// match for those - without this the benchmark would only ever exercise
// the cheaper of the two paths.

fn load_pairs(path: &str) -> Vec<(String, String)> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read fixture playlist at {path}: {e}"));
    let mut lines = text.lines();
    let mut pairs = Vec::new();
    while let Some(line) = lines.next() {
        if line.starts_with("#EXTINF") {
            if let Some(url) = lines.next() {
                pairs.push((line.to_string(), url.to_string()));
            }
        }
    }
    pairs
}

fn ensure_large_synthetic_playlist() {
    if std::path::Path::new(LARGE_FIXTURE).exists() {
        return;
    }
    eprintln!(
        "Generating {LARGE_CHANNEL_COUNT} synthetic channels across {LARGE_CATEGORY_COUNT} \
         categories at {LARGE_FIXTURE} (one-time, cached for future runs)..."
    );
    let mut out = String::with_capacity(LARGE_CHANNEL_COUNT * 200);
    out.push_str("#EXTM3U\n");
    for i in 0..LARGE_CHANNEL_COUNT {
        let category = i % LARGE_CATEGORY_COUNT;
        // 1 in 20 (5%) omit tvg-name, same idea as the 50/50 split but
        // matching real playlists more closely, where most channels do
        // carry a proper tvg-name and the fallback path is the exception.
        let tvg_name = if i % 20 == 0 {
            String::new()
        } else {
            format!(" tvg-name=\"Channel {i:06}\"")
        };
        out.push_str(&format!(
            "#EXTINF:-1 tvg-id=\"CH{i:06}\"{tvg_name} tvg-logo=\"https://img.example.com/logos/{i:06}.png\" \
             group-title=\"Category {category:05}\",Channel {i:06}\n"
        ));
        out.push_str(&format!(
            "https://stream.example.com/channel/{i:06}/index.m3u8\n"
        ));
    }
    std::fs::write(LARGE_FIXTURE, out).expect("failed to write synthetic playlist fixture");
}

fn bench_parse_full_playlist(c: &mut Criterion) {
    let pairs = load_pairs(FIXTURE);
    c.bench_function("parse all 2471 channels (Samsung TV Plus fixture)", |b| {
        b.iter(|| {
            for (first, second) in &pairs {
                let channel = get_channel_from_lines(
                    black_box(first.clone()),
                    black_box(second.clone()),
                    1,
                    Some(false),
                );
                black_box(channel).ok();
            }
        })
    });
}

fn bench_parse_single_channel(c: &mut Criterion) {
    let pairs = load_pairs(FIXTURE);
    let (first, second) = pairs[0].clone();
    c.bench_function("parse a single channel", |b| {
        b.iter(|| {
            black_box(get_channel_from_lines(
                black_box(first.clone()),
                black_box(second.clone()),
                1,
                Some(false),
            ))
        })
    });
}

fn bench_parse_large_synthetic_playlist(c: &mut Criterion) {
    ensure_large_synthetic_playlist();
    let pairs = load_pairs(LARGE_FIXTURE);

    // Each iteration parses all 500k channels (several seconds) - the
    // default sample_size (100) and measurement_time (5s) assume
    // sub-millisecond iterations, so both need raising or criterion either
    // takes ~15 minutes or warns about not reaching its target time.
    let mut group = c.benchmark_group("large synthetic playlist");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(90));
    group.bench_function(
        format!("parse {LARGE_CHANNEL_COUNT} channels across {LARGE_CATEGORY_COUNT} categories (synthetic)"),
        |b| {
            b.iter(|| {
                for (first, second) in &pairs {
                    let channel = get_channel_from_lines(
                        black_box(first.clone()),
                        black_box(second.clone()),
                        1,
                        Some(false),
                    );
                    black_box(channel).ok();
                }
            })
        },
    );
    group.finish();
}

// Isolates read_m3u8's per-line directive check (not exercised by the other
// benchmarks here, which all call get_channel_from_lines directly) - answers
// whether avoiding the to_uppercase() allocation there actually helped.
fn bench_line_prefix_check(c: &mut Criterion) {
    let text = std::fs::read_to_string(FIXTURE).expect("fixture playlist should exist");
    let lines: Vec<&str> = text.lines().collect();

    let mut group = c.benchmark_group("line prefix check (read_m3u8's per-line classification)");
    group.bench_function("old: to_uppercase() + starts_with", |b| {
        b.iter(|| {
            let mut extinf = 0usize;
            let mut vlcopt = 0usize;
            for line in &lines {
                let upper = black_box(*line).to_uppercase();
                if upper.starts_with("#EXTINF") {
                    extinf += 1;
                } else if upper.starts_with("#EXTVLCOPT") {
                    vlcopt += 1;
                }
            }
            black_box((extinf, vlcopt))
        })
    });
    group.bench_function("new: eq_ignore_ascii_case on a prefix slice", |b| {
        b.iter(|| {
            let mut extinf = 0usize;
            let mut vlcopt = 0usize;
            for line in &lines {
                let line = black_box(*line);
                if line
                    .get(.."#EXTINF".len())
                    .is_some_and(|s| s.eq_ignore_ascii_case("#EXTINF"))
                {
                    extinf += 1;
                } else if line
                    .get(.."#EXTVLCOPT".len())
                    .is_some_and(|s| s.eq_ignore_ascii_case("#EXTVLCOPT"))
                {
                    vlcopt += 1;
                }
            }
            black_box((extinf, vlcopt))
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_parse_full_playlist,
    bench_parse_single_channel,
    bench_parse_large_synthetic_playlist,
    bench_line_prefix_check
);
criterion_main!(benches);
