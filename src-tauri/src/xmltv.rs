use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
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
    let tmp_path = get_tmp_path();
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

fn get_tmp_path() -> String {
    let mut path = directories::ProjectDirs::from("dev", "fredol", "open-tv")
        .unwrap()
        .cache_dir()
        .to_owned();
    if !path.exists() {
        std::fs::create_dir_all(&path).unwrap();
    }
    path.push("get_epg.dat");
    path.to_string_lossy().to_string()
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

    let mut xml_reader = Reader::from_reader(open_reader(path)?);
    xml_reader.config_mut().trim_text(true);

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
            Event::End(e) if e.name().as_ref() == "programme" => {
                if let Some((tvg_id, start_timestamp, end_timestamp)) = current.take() {
                    // mem::take instead of clone() - the accumulated title/desc
                    // are moved out (leaving empty Strings behind, which the
                    // next <programme>'s clear() calls above would do anyway),
                    // avoiding a copy per programme kept.
                    programmes.push(ParsedProgramme {
                        tvg_id,
                        title: std::mem::take(&mut current_title),
                        description: std::mem::take(&mut current_desc),
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

fn parse_xmltv_time(raw: &str) -> Option<i64> {
    DateTime::parse_from_str(raw.trim(), "%Y%m%d%H%M%S %z")
        .ok()
        .map(|d| d.timestamp())
}
