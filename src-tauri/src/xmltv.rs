use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDateTime, Utc};
use flate2::read::GzDecoder;
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::{
    log, sql,
    types::Source,
    utils::get_user_agent_from_source,
};

// Default when a source hasn't set its own epg_retention_days - how far
// back past programmes are kept, so they stay available to browse/catch
// up on (see epg_dispatch's read side, which uses the same setting).
pub const DEFAULT_EPG_RETENTION_DAYS: i64 = 7;
const EPG_LOOKAHEAD_SECONDS: i64 = 7 * 24 * 60 * 60;

// pub(crate) so sql::insert_epg_programmes_batch can build INSERT params
// directly from these without an intermediate tuple conversion.
pub(crate) struct ParsedProgramme {
    pub(crate) tvg_id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) start_timestamp: i64,
    pub(crate) end_timestamp: i64,
}

enum TextTarget {
    None,
    Title,
    Desc,
}

pub async fn refresh_epg(source: Source) -> Result<()> {
    let url = source.epg_url.clone().context("no epg url")?;
    refresh_epg_from_url(source, url).await
}

pub async fn refresh_epg_from_url(source: Source, url: String) -> Result<()> {
    let refresh_start = std::time::Instant::now();
    let source_id = source.id.context("no source id")?;
    let user_agent = get_user_agent_from_source(&source)?;
    let client = crate::utils::new_http_client(&user_agent)?;
    let fetch_start = std::time::Instant::now();
    let mut response = client.get(&url).send().await?;
    if !response.status().is_success() {
        bail!("Failed to fetch EPG, status: {}", response.status());
    }
    let tmp_path = get_tmp_path()?;
    let mut bytes_written: u64 = 0;
    {
        let mut file = std::fs::File::create(&tmp_path)?;
        while let Some(chunk) = response.chunk().await? {
            bytes_written += chunk.len() as u64;
            file.write_all(&chunk)?;
        }
    }
    log::log(format!(
        "[perf] {}: EPG download ({} bytes) took {:?}",
        source.name,
        bytes_written,
        fetch_start.elapsed()
    ));
    let parse_start = std::time::Instant::now();
    let known_tvg_ids: HashSet<String> = sql::get_tvg_ids_for_source(source_id)?
        .iter()
        .map(|id| crate::utils::normalize_tvg_id(id))
        .collect();
    let retention_days = source
        .epg_retention_days
        .map(|d| d as i64)
        .unwrap_or(DEFAULT_EPG_RETENTION_DAYS);
    let programmes = parse_xmltv(&tmp_path, &known_tvg_ids, retention_days * 24 * 60 * 60)?;
    log::log(format!(
        "[perf] {}: EPG parse ({} programmes) took {:?}",
        source.name,
        programmes.len(),
        parse_start.elapsed()
    ));
    log::log(format!(
        "Parsed {} EPG programmes for source {}",
        programmes.len(),
        source.name
    ));
    let db_start = std::time::Instant::now();
    let cached_at = Utc::now().timestamp();
    let mut sql_conn = sql::get_conn()?;
    let tx = sql_conn.transaction()?;
    sql::delete_epg_programmes_by_source(&tx, source_id)?;
    sql::insert_epg_programmes_batch(&tx, source_id, &programmes, cached_at)?;
    sql::analyze(&tx)?;
    tx.commit()?;
    log::log(format!(
        "[perf] {}: EPG delete+insert+analyze+commit took {:?}",
        source.name,
        db_start.elapsed()
    ));
    log::log(format!(
        "[perf] {}: total refresh_epg_from_url took {:?}",
        source.name,
        refresh_start.elapsed()
    ));
    Ok(())
}

// Refreshing already re-applies the current retention setting (it wipes and
// re-parses with today's lookback), so this only matters when you lower
// retention_days after already having more history stored than that, and
// want it trimmed immediately rather than waiting for the next refresh.
pub fn prune_old_epg(source: Source) -> Result<()> {
    let source_id = source.id.context("no source id")?;
    let retention_days = source
        .epg_retention_days
        .map(|d| d as i64)
        .unwrap_or(DEFAULT_EPG_RETENTION_DAYS);
    let cutoff = Utc::now().timestamp() - retention_days * 24 * 60 * 60;
    sql::prune_old_epg(source_id, cutoff)
}

fn get_tmp_path() -> Result<String> {
    let mut path = directories::ProjectDirs::from("dev", "iptelly", "iptelly")
        .context("Could not determine the app cache directory")?
        .cache_dir()
        .to_owned();
    if !path.exists() {
        std::fs::create_dir_all(&path).context("Failed to create EPG cache directory")?;
    }
    path.push("get_epg.dat");
    Ok(path.to_string_lossy().to_string())
}

fn open_reader(path: &str) -> Result<Box<dyn BufRead>> {
    let mut header = [0u8; 2];
    let is_gzip = {
        let mut peek = std::fs::File::open(path)?;
        peek.read_exact(&mut header).is_ok() && header == [0x1f, 0x8b]
    };
    let file = std::fs::File::open(path)?;
    if is_gzip {
        Ok(Box::new(BufReader::new(GzDecoder::new(file))))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

fn parse_xmltv(
    path: &str,
    known_tvg_ids: &HashSet<String>,
    lookback_seconds: i64,
) -> Result<Vec<ParsedProgramme>> {
    let now = Utc::now().timestamp();
    let from_ts = now - lookback_seconds;
    let to_ts = now + EPG_LOOKAHEAD_SECONDS;
    parse_programmes(open_reader(path)?, known_tvg_ids, from_ts, to_ts)
}

// Split from parse_xmltv so tests can feed in-memory XML and a fixed time
// window, rather than depending on a file and the current time.
fn parse_programmes(
    reader: impl BufRead,
    known_tvg_ids: &HashSet<String>,
    from_ts: i64,
    to_ts: i64,
) -> Result<Vec<ParsedProgramme>> {
    // Not trim_text(true): quick-xml reports entity references (&amp; etc.)
    // as their own GeneralRef events, splitting the text around them, so
    // trimming each piece would eat the spaces either side of the entity
    // ("Fish &amp; Chips" -> "Fish&Chips"). Title/desc are trimmed as a
    // whole when the programme ends instead.
    let mut xml_reader = Reader::from_reader(reader);

    let mut programmes = Vec::new();
    let mut buf = Vec::new();
    let mut current: Option<(String, i64, i64)> = None;
    let mut current_title = String::new();
    let mut current_desc = String::new();
    let mut text_target = TextTarget::None;

    loop {
        let event = xml_reader.read_event_into(&mut buf)?;
        match event {
            Event::Eof => break,
            Event::Start(e) if e.name().as_ref() == "programme" => {
                current = read_programme_attrs(&e, known_tvg_ids, from_ts, to_ts);
                current_title.clear();
                current_desc.clear();
            }
            Event::Start(e) if e.name().as_ref() == "title" => {
                text_target = TextTarget::Title;
            }
            Event::Start(e) if e.name().as_ref() == "desc" => {
                text_target = TextTarget::Desc;
            }
            Event::End(e) if e.name().as_ref() == "title" || e.name().as_ref() == "desc" => {
                text_target = TextTarget::None;
            }
            Event::Text(t) if current.is_some() => {
                let decoded = quick_xml::escape::unescape(&t).unwrap_or_default();
                match text_target {
                    TextTarget::Title => current_title.push_str(&decoded),
                    TextTarget::Desc => current_desc.push_str(&decoded),
                    TextTarget::None => {}
                }
            }
            Event::GeneralRef(r) if current.is_some() => {
                let target = match text_target {
                    TextTarget::Title => Some(&mut current_title),
                    TextTarget::Desc => Some(&mut current_desc),
                    TextTarget::None => None,
                };
                if let Some(target) = target {
                    push_entity(target, &r);
                }
            }
            Event::End(e) if e.name().as_ref() == "programme" => {
                if let Some((tvg_id, start_timestamp, end_timestamp)) = current.take() {
                    // take_trimmed instead of clone() - the accumulated
                    // title/desc are moved out when already trimmed (the
                    // usual case), leaving empty Strings behind, which the
                    // next <programme>'s clear() calls above would do
                    // anyway, avoiding a copy per programme kept.
                    programmes.push(ParsedProgramme {
                        tvg_id,
                        title: take_trimmed(&mut current_title),
                        description: take_trimmed(&mut current_desc),
                        start_timestamp,
                        end_timestamp,
                    });
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(programmes)
}

fn push_entity(target: &mut String, entity: &quick_xml::events::BytesRef) {
    if let Ok(Some(ch)) = entity.resolve_char_ref() {
        target.push(ch);
    } else if let Some(resolved) = quick_xml::escape::resolve_xml_entity(entity) {
        target.push_str(resolved);
    } else {
        // Not a predefined XML entity (or an invalid character reference) -
        // keep it as written rather than silently dropping it.
        target.push('&');
        target.push_str(entity);
        target.push(';');
    }
}

fn take_trimmed(text: &mut String) -> String {
    let trimmed = text.trim();
    if trimmed.len() == text.len() {
        std::mem::take(text)
    } else {
        trimmed.to_string()
    }
}

fn read_programme_attrs(
    e: &quick_xml::events::BytesStart,
    known_tvg_ids: &HashSet<String>,
    from_ts: i64,
    to_ts: i64,
) -> Option<(String, i64, i64)> {
    let mut channel_id = None;
    // Deliberately kept as raw strings rather than parsed to timestamps
    // here - see the known_tvg_ids check below for why.
    let mut start_raw: Option<String> = None;
    let mut stop_raw: Option<String> = None;
    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            "channel" => channel_id = Some(crate::utils::normalize_tvg_id(&attr.value)),
            "start" => start_raw = Some(attr.value.into_owned()),
            "stop" => stop_raw = Some(attr.value.into_owned()),
            _ => {}
        }
    }
    let channel_id = channel_id?;
    // Most providers' xmltv.php covers their entire channel catalogue,
    // while known_tvg_ids only covers the (usually much smaller) set of
    // channels this playlist actually imported - checking membership here,
    // before parsing either timestamp, skips the comparatively expensive
    // chrono format-string parsing entirely for programmes belonging to
    // channels this playlist doesn't even have.
    if !known_tvg_ids.contains(&channel_id) {
        return None;
    }
    let start = parse_xmltv_time(&start_raw?)?;
    let stop = parse_xmltv_time(&stop_raw?)?;
    if stop < from_ts || start > to_ts {
        return None;
    }
    Some((channel_id, start, stop))
}

// XMLTV makes the timezone offset optional, defaulting to UTC when it's
// left off, and some guides do leave it off.
fn parse_xmltv_time(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    DateTime::parse_from_str(raw, "%Y%m%d%H%M%S %z")
        .map(|d| d.timestamp())
        .or_else(|_| {
            NaiveDateTime::parse_from_str(raw, "%Y%m%d%H%M%S").map(|d| d.and_utc().timestamp())
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-01-01 12:00:00 UTC
    const NOON: i64 = 1_767_268_800;
    const HOUR: i64 = 3600;

    fn ids(ids: &[&str]) -> HashSet<String> {
        ids.iter()
            .map(|id| crate::utils::normalize_tvg_id(id))
            .collect()
    }

    fn parse(xml: &str, known: &[&str]) -> Vec<ParsedProgramme> {
        parse_programmes(
            xml.as_bytes(),
            &ids(known),
            NOON - 24 * HOUR,
            NOON + 24 * HOUR,
        )
        .unwrap()
    }

    fn programme(channel: &str, start: &str, stop: &str, inner: &str) -> String {
        format!(
            r#"<programme channel="{channel}" start="{start}" stop="{stop}">{inner}</programme>"#
        )
    }

    fn tv(programmes: &[String]) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><tv>{}</tv>"#,
            programmes.concat()
        )
    }

    #[test]
    fn parses_times_with_a_utc_offset() {
        assert_eq!(parse_xmltv_time("20260101120000 +0000"), Some(NOON));
        assert_eq!(parse_xmltv_time("20260101130000 +0100"), Some(NOON));
        assert_eq!(parse_xmltv_time("20260101070000 -0500"), Some(NOON));
        assert_eq!(parse_xmltv_time("  20260101120000 +0000  "), Some(NOON));
    }

    #[test]
    fn parses_times_without_a_space_before_the_offset() {
        assert_eq!(parse_xmltv_time("20260101130000+0100"), Some(NOON));
    }

    #[test]
    fn treats_times_without_an_offset_as_utc() {
        assert_eq!(parse_xmltv_time("20260101120000"), Some(NOON));
        assert_eq!(parse_xmltv_time(" 20260101120000 "), Some(NOON));
    }

    #[test]
    fn keeps_programmes_whose_times_have_no_offset() {
        let xml = tv(&[programme(
            "c",
            "20260101120000",
            "20260101130000",
            "<title>Now</title>",
        )]);
        let programmes = parse(&xml, &["c"]);
        assert_eq!(programmes.len(), 1);
        assert_eq!(programmes[0].start_timestamp, NOON);
        assert_eq!(programmes[0].end_timestamp, NOON + HOUR);
    }

    #[test]
    fn rejects_invalid_times() {
        assert_eq!(parse_xmltv_time(""), None);
        assert_eq!(parse_xmltv_time("not a time"), None);
        assert_eq!(parse_xmltv_time("20261301120000 +0000"), None);
    }

    #[test]
    fn parses_a_programme() {
        let xml = tv(&[programme(
            "bbc1.uk",
            "20260101120000 +0000",
            "20260101130000 +0000",
            "<title>News</title><desc>The headlines</desc>",
        )]);
        let programmes = parse(&xml, &["bbc1.uk"]);
        assert_eq!(programmes.len(), 1);
        let p = &programmes[0];
        assert_eq!(p.tvg_id, "bbc1.uk");
        assert_eq!(p.title, "News");
        assert_eq!(p.description, "The headlines");
        assert_eq!(p.start_timestamp, NOON);
        assert_eq!(p.end_timestamp, NOON + HOUR);
    }

    #[test]
    fn resolves_entities_and_keeps_the_spaces_around_them() {
        let xml = tv(&[programme(
            "bbc1.uk",
            "20260101120000 +0000",
            "20260101130000 +0000",
            "<title>Fish &amp; Chips</title><desc>&lt;Live&gt; &quot;now&quot; &apos;ok&apos;</desc>",
        )]);
        let p = &parse(&xml, &["bbc1.uk"])[0];
        assert_eq!(p.title, "Fish & Chips");
        assert_eq!(p.description, "<Live> \"now\" 'ok'");
    }

    #[test]
    fn resolves_character_references() {
        let xml = tv(&[programme(
            "bbc1.uk",
            "20260101120000 +0000",
            "20260101130000 +0000",
            "<title>Caf&#233; &#x2014; Rock&#x27;n&#x27;Roll</title>",
        )]);
        assert_eq!(parse(&xml, &["bbc1.uk"])[0].title, "Café — Rock'n'Roll");
    }

    #[test]
    fn keeps_unknown_entities_as_written() {
        let xml = tv(&[programme(
            "bbc1.uk",
            "20260101120000 +0000",
            "20260101130000 +0000",
            "<title>A&nbsp;B</title>",
        )]);
        assert_eq!(parse(&xml, &["bbc1.uk"])[0].title, "A&nbsp;B");
    }

    #[test]
    fn trims_whitespace_around_title_and_desc() {
        let xml = tv(&[programme(
            "bbc1.uk",
            "20260101120000 +0000",
            "20260101130000 +0000",
            "<title>\n      News &amp; Weather\n    </title>\n    <desc>  Line one\nline two  </desc>",
        )]);
        let p = &parse(&xml, &["bbc1.uk"])[0];
        assert_eq!(p.title, "News & Weather");
        assert_eq!(p.description, "Line one\nline two");
    }

    #[test]
    fn ignores_text_outside_title_and_desc() {
        let xml = tv(&[programme(
            "bbc1.uk",
            "20260101120000 +0000",
            "20260101130000 +0000",
            "<title>News</title><sub-title>Episode 1</sub-title><category>Factual</category>",
        )]);
        let p = &parse(&xml, &["bbc1.uk"])[0];
        assert_eq!(p.title, "News");
        assert_eq!(p.description, "");
    }

    #[test]
    fn does_not_carry_text_over_between_programmes() {
        let xml = tv(&[
            programme(
                "bbc1.uk",
                "20260101120000 +0000",
                "20260101130000 +0000",
                "<title>News</title><desc>The headlines</desc>",
            ),
            programme(
                "bbc1.uk",
                "20260101130000 +0000",
                "20260101140000 +0000",
                "<title>Weather</title>",
            ),
        ]);
        let programmes = parse(&xml, &["bbc1.uk"]);
        assert_eq!(programmes.len(), 2);
        assert_eq!(programmes[1].title, "Weather");
        assert_eq!(programmes[1].description, "");
    }

    #[test]
    fn skips_channels_the_playlist_does_not_have() {
        let xml = tv(&[
            programme(
                "bbc1.uk",
                "20260101120000 +0000",
                "20260101130000 +0000",
                "<title>A</title>",
            ),
            programme(
                "itv1.uk",
                "20260101120000 +0000",
                "20260101130000 +0000",
                "<title>B</title>",
            ),
        ]);
        let programmes = parse(&xml, &["bbc1.uk"]);
        assert_eq!(programmes.len(), 1);
        assert_eq!(programmes[0].title, "A");
    }

    #[test]
    fn matches_channel_ids_after_normalising_both_sides() {
        let xml = tv(&[programme(
            "BBCParliament.UK",
            "20260101120000 +0000",
            "20260101130000 +0000",
            "<title>Debate</title>",
        )]);
        let programmes = parse(&xml, &["BBCParliament.uk@SD"]);
        assert_eq!(programmes.len(), 1);
        assert_eq!(programmes[0].tvg_id, "bbcparliament.uk");
    }

    #[test]
    fn keeps_only_programmes_overlapping_the_window() {
        let xml = tv(&[
            // Ended before the window.
            programme(
                "c",
                "20251230100000 +0000",
                "20251230110000 +0000",
                "<title>Old</title>",
            ),
            // Started before the window but still running into it.
            programme(
                "c",
                "20251231113000 +0000",
                "20251231123000 +0000",
                "<title>Edge</title>",
            ),
            programme(
                "c",
                "20260101120000 +0000",
                "20260101130000 +0000",
                "<title>Now</title>",
            ),
            // Starts after the window.
            programme(
                "c",
                "20260103120000 +0000",
                "20260103130000 +0000",
                "<title>Later</title>",
            ),
        ]);
        let titles: Vec<String> = parse(&xml, &["c"]).into_iter().map(|p| p.title).collect();
        assert_eq!(titles, ["Edge", "Now"]);
    }

    #[test]
    fn skips_programmes_with_missing_or_invalid_times() {
        let xml = tv(&[
            r#"<programme channel="c" start="20260101120000 +0000"><title>No stop</title></programme>"#
                .to_string(),
            r#"<programme channel="c" stop="20260101130000 +0000"><title>No start</title></programme>"#
                .to_string(),
            programme("c", "garbage", "20260101130000 +0000", "<title>Bad start</title>"),
            programme("c", "20260101120000 +0000", "20260101130000 +0000", "<title>Good</title>"),
        ]);
        let titles: Vec<String> = parse(&xml, &["c"]).into_iter().map(|p| p.title).collect();
        assert_eq!(titles, ["Good"]);
    }

    #[test]
    fn skips_programmes_without_a_channel() {
        let xml = tv(&[
            r#"<programme start="20260101120000 +0000" stop="20260101130000 +0000"><title>Orphan</title></programme>"#
                .to_string(),
        ]);
        assert!(parse(&xml, &["c"]).is_empty());
    }

    #[test]
    fn reads_plain_and_gzipped_files() {
        let xml = tv(&[programme(
            "c",
            "20260101120000 +0000",
            "20260101130000 +0000",
            "<title>Now</title>",
        )]);
        let dir = std::env::temp_dir();
        let plain = dir.join(format!("iptelly_xmltv_test_{}.xml", std::process::id()));
        let gzipped = dir.join(format!("iptelly_xmltv_test_{}.xml.gz", std::process::id()));
        std::fs::write(&plain, &xml).unwrap();
        let mut encoder = flate2::write::GzEncoder::new(
            std::fs::File::create(&gzipped).unwrap(),
            Default::default(),
        );
        encoder.write_all(xml.as_bytes()).unwrap();
        encoder.finish().unwrap();

        for path in [&plain, &gzipped] {
            let reader = open_reader(path.to_str().unwrap()).unwrap();
            let programmes =
                parse_programmes(reader, &ids(&["c"]), NOON - HOUR, NOON + HOUR).unwrap();
            assert_eq!(programmes.len(), 1, "{}", path.display());
            assert_eq!(programmes[0].title, "Now");
        }
        std::fs::remove_file(plain).unwrap();
        std::fs::remove_file(gzipped).unwrap();
    }

    #[test]
    fn parses_the_samsung_tv_plus_fixture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/samsung_tvplus_epg.xml.gz"
        );
        let programmes = parse_programmes(
            open_reader(path).unwrap(),
            &ids(&["AT1100006NI"]),
            0,
            i64::MAX,
        )
        .unwrap();
        assert_eq!(programmes.len(), 17);
        let first = &programmes[0];
        assert_eq!(first.tvg_id, "at1100006ni");
        assert_eq!(first.title, "Lucky Dog");
        assert!(first.description.starts_with("Brandon zügelt"));
        assert_eq!(first.start_timestamp, 1_790_841_761);
        assert_eq!(first.end_timestamp, 1_790_843_130);
    }

    #[test]
    fn parses_entities_in_the_samsung_tv_plus_fixture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/samsung_tvplus_epg.xml.gz"
        );
        let programmes = parse_programmes(
            open_reader(path).unwrap(),
            &ids(&["ATAJ700002OG"]),
            0,
            i64::MAX,
        )
        .unwrap();
        assert!(programmes.iter().any(|p| p.title == "Macky & Shefat"));
    }
}
