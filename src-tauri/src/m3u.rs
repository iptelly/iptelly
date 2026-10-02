use std::io::Write;
use std::sync::LazyLock;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
};

use anyhow::{Context, Result, bail};
use regex::{Captures, Regex};
use rusqlite::Transaction;
use types::{Channel, Source};

use crate::types::ChannelPreserve;
use crate::{
    log, media_type, source_type,
    sql::{self, set_channel_group_id},
    types::{self, ChannelHttpHeaders},
    utils::get_user_agent_from_source,
};

static NAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"tvg-name="(?P<name>[^"]*)""#).unwrap());
static ID_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"tvg-id="(?P<id>[^"]*)""#).unwrap());
static LOGO_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"tvg-logo="(?P<logo>[^"]*)""#).unwrap());
static GROUP_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"group-title="(?P<group>[^"]*)""#).unwrap());

static HTTP_ORIGIN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"http-origin=(?P<origin>.+)"#).unwrap());
static HTTP_REFERRER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"http-referrer=(?P<referrer>.+)"#).unwrap());
static HTTP_USER_AGENT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"http-user-agent=(?P<user_agent>.+)"#).unwrap());

struct M3UProcessing {
    channel_line: Option<String>,
    channel_headers: Option<ChannelHttpHeaders>,
    channel_headers_set: bool,
    last_non_empty_line: Option<String>,
    groups: HashMap<String, i64>,
    source_id: i64,
    use_tvg_id: Option<bool>,
    line_count: usize,
}

pub fn read_m3u8(mut source: Source, wipe: bool) -> Result<()> {
    let path = match source.source_type {
        source_type::M3U_LINK => get_tmp_path()?,
        _ => source.url.clone().context("no file path found")?,
    };
    let file = File::open(path).context("Failed to open m3u8 file")?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines().enumerate();
    let mut sql = sql::get_conn()?;
    let mut channel_preserve: Vec<ChannelPreserve> = Vec::new();
    let tx = sql.transaction()?;
    if wipe {
        channel_preserve =
            sql::get_preserve(&tx, source.id.context("no source id")?).unwrap_or_default();
        // Picks up anything a whole-app data import staged for this source
        // before its channels existed yet to restore_preserve against (see
        // pending_preserve's migration comment in sql.rs) - a no-op for
        // every ordinary refresh, which never stages anything here.
        channel_preserve.extend(sql::consume_pending_preserve(
            &tx,
            source.id.context("no source id")?,
        )?);
        sql::wipe(&tx, source.id.context("no source id")?)?;
    } else {
        source.id = Some(sql::create_or_find_source_by_name(&tx, &source)?);
    }
    let mut processing = M3UProcessing {
        channel_headers: None,
        channel_headers_set: false,
        channel_line: None,
        groups: HashMap::new(),
        last_non_empty_line: None,
        source_id: source.id.context("no source id")?,
        use_tvg_id: source.use_tvg_id,
        line_count: 0,
    };
    while let Some((c1, l1)) = lines.next() {
        processing.line_count = c1;
        let l1 = match l1.with_context(|| format!("Failed to process line {c1}")) {
            Ok(r) => r,
            Err(e) => {
                log::log(format!("{:?}", e));
                continue;
            }
        };
        if starts_with_ignore_case(&l1, "#EXTINF") {
            try_commit_channel(&mut processing, &tx);
            processing.channel_line = Some(l1);
            processing.channel_headers_set = false;
        } else if starts_with_ignore_case(&l1, "#EXTVLCOPT") {
            if processing.channel_headers.is_none() {
                processing.channel_headers = Some(ChannelHttpHeaders {
                    ..Default::default()
                });
            }
            if set_http_headers(
                &l1,
                processing.channel_headers.as_mut().context("no headers")?,
            ) {
                processing.channel_headers_set = true;
            }
        } else if !l1.trim().is_empty() {
            processing.last_non_empty_line = Some(l1);
        }
    }
    try_commit_channel(&mut processing, &tx);
    if wipe {
        sql::restore_preserve(&tx, source.id.context("no source id")?, channel_preserve)?;
    }
    sql::analyze(&tx)?;
    tx.commit()?;
    Ok(())
}

fn try_commit_channel(processing: &mut M3UProcessing, tx: &Transaction) {
    if let Some(channel) = processing.channel_line.take() {
        if !processing.channel_headers_set {
            processing.channel_headers = None;
        }
        commit_channel(
            channel,
            processing.last_non_empty_line.take(),
            &mut processing.groups,
            processing.channel_headers.take(),
            processing.source_id,
            processing.use_tvg_id,
            &tx,
        )
        .with_context(|| {
            format!(
                "Failed to process channel ending at line {}",
                processing.line_count
            )
        })
        .unwrap_or_else(|e| {
            log::log(format!("{:?}", e));
        });
    }
}

fn commit_channel(
    channel_line: String,
    last_line: Option<String>,
    groups: &mut HashMap<String, i64>,
    headers: Option<ChannelHttpHeaders>,
    source_id: i64,
    use_tvg_id: Option<bool>,
    tx: &Transaction,
) -> Result<()> {
    let mut channel = get_channel_from_lines(
        channel_line,
        last_line.context("missing last line")?,
        source_id,
        use_tvg_id,
    )?;
    set_channel_group_id(groups, &mut channel, tx, &source_id).unwrap_or_else(|e| {
        log::log(format!(
            "Failed to set group id for channel: {}, Error: {:?}",
            channel.name, e
        ))
    });
    sql::insert_channel(tx, channel)?;
    if let Some(mut headers) = headers {
        headers.channel_id = Some(tx.last_insert_rowid());
        sql::insert_channel_headers(tx, headers)?;
    }
    Ok(())
}

pub async fn get_m3u8_from_link(source: Source, wipe: bool) -> Result<()> {
    let user_agent = get_user_agent_from_source(&source)?;
    let client = crate::utils::new_http_client(&user_agent)?;
    let url = source.url.clone().context("Invalid source")?;
    let mut response = client.get(&url).send().await?;
    if !response.status().is_success() {
        log::log(format!(
            "Failed to get m3u8 from link, status: {}",
            response.status()
        ));
        bail!(
            "Failed to get m3u8 from link, status: {}",
            response.status()
        );
    }
    let mut file = std::fs::File::create(get_tmp_path()?)?;
    while let Some(chunk) = response.chunk().await? {
        file.write(&chunk)?;
    }
    read_m3u8(source, wipe)
}

fn get_tmp_path() -> Result<String> {
    let mut path = directories::ProjectDirs::from("dev", "iptelly", "iptelly")
        .context("Could not determine the app cache directory")?
        .cache_dir()
        .to_owned();
    if !path.exists() {
        std::fs::create_dir_all(&path).context("Failed to create m3u cache directory")?;
    }
    path.push("get.m3u");
    Ok(path.to_string_lossy().to_string())
}

// Avoids allocating an uppercased copy of the *entire* line (read_m3u8's
// main loop runs this on every line of the file, URLs included, not just
// the directive lines it's actually checking for) just to do a
// case-insensitive prefix check.
fn starts_with_ignore_case(line: &str, prefix: &str) -> bool {
    line.get(..prefix.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
}

// Trims before allocating, rather than after - some callers used to repeat
// their own `.trim().to_string()` on top of this, re-copying a string that
// was just copied here. Returns the already-trimmed value, None if blank.
fn non_empty_trimmed(m: regex::Match) -> Option<String> {
    let value = m.as_str().trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn extract_non_empty_capture(caps: Captures) -> Option<String> {
    caps.get(1).and_then(non_empty_trimmed)
}

fn set_http_headers(line: &str, headers: &mut ChannelHttpHeaders) -> bool {
    if let Some(origin) = HTTP_ORIGIN_REGEX
        .captures(&line)
        .and_then(extract_non_empty_capture)
    {
        headers.http_origin = Some(origin);
        return true;
    } else if let Some(referrer) = HTTP_REFERRER_REGEX
        .captures(&line)
        .and_then(extract_non_empty_capture)
    {
        headers.referrer = Some(referrer);
        return true;
    } else if let Some(user_agent) = HTTP_USER_AGENT_REGEX
        .captures(&line)
        .and_then(extract_non_empty_capture)
    {
        headers.user_agent = Some(user_agent);
        return true;
    }
    return false;
}

// pub so the integration test (proving every channel in a real playlist
// ends up in the DB) and the parser benchmark can both call it directly -
// neither lives inside this crate, so anything short of pub is invisible
// to them.
pub fn get_channel_from_lines(
    first: String,
    mut second: String,
    source_id: i64,
    use_tvg_id: Option<bool>,
) -> Result<Channel> {
    second = second.trim().to_string();
    if second.is_empty() {
        bail!("second line is empty");
    }
    let tvg_id = ID_REGEX.captures(&first).and_then(extract_non_empty_capture);
    let name = NAME_REGEX
        .captures(&first)
        .and_then(extract_non_empty_capture)
        .or_else(|| {
            // The title is everything after the *last* comma on the line,
            // not the first - several providers put an unescaped comma
            // inside an earlier attribute (e.g. group-title="Sports,
            // General"), and splitting there would wrongly cut the name
            // right after it instead of at the real title separator. A
            // plain rfind is also meaningfully cheaper than a regex scan
            // for that same last comma would be.
            let name_alt = || {
                first.rfind(',').and_then(|comma| {
                    let value = first[comma + 1..].trim();
                    (!value.is_empty()).then(|| value.to_string())
                })
            };
            if let Some(true) = use_tvg_id {
                return tvg_id.clone().or_else(name_alt);
            } else {
                return name_alt().or_else(|| tvg_id.clone());
            }
        })
        .context("Couldn't find name from Name or ID")?;
    let group = GROUP_REGEX
        .captures(&first)
        .and_then(extract_non_empty_capture);
    let image = LOGO_REGEX
        .captures(&first)
        .and_then(extract_non_empty_capture);
    let channel = Channel {
        id: None,
        name,
        group,
        image,
        url: Some(second.clone()),
        media_type: get_media_type(second),
        source_id: Some(source_id),
        series_id: None,
        group_id: None,
        favorite: false,
        stream_id: None,
        tv_archive: None,
        tvg_id,
        season_id: None,
        episode_num: None,
        hidden: Some(false),
        is_adult: false,
    };
    Ok(channel)
}

fn get_media_type(url: String) -> u8 {
    let media_type = if url.ends_with(".mp4") || url.ends_with(".mkv") {
        media_type::MOVIE
    } else {
        media_type::LIVESTREAM
    };
    return media_type;
}

#[cfg(test)]
mod test_m3u {
    use super::*;

    #[test]
    fn test_get_channel_from_lines() {
        get_channel_from_lines(r#"#EXTINF:-1 tvg-id="Amazing Channel" tvg-name="Amazing Channel" tvg-logo="http://myurl.local/logos/amazing/amazing-1.png" group-title="The Best Channels"#.to_string()
       , r#"http://myurl.local/1234/1234/1234"#.to_string(), 0,Some(true)).unwrap();
        get_channel_from_lines(r#"#EXTINF:-1 tvg-id="Amazing Channel" tvg-name="" tvg-logo="http://myurl.local/logos/amazing/amazing-1.png" group-title="The Best Channels"#.to_string()
       , r#"http://myurl.local/1234/1234/1234"#.to_string(), 0, Some(true)).unwrap();
        assert!(get_channel_from_lines(r#"#EXTINF:-1 tvg-id="" tvg-name="" tvg-logo="http://myurl.local/logos/amazing/amazing-1.png" group-title="The Best Channels"#.to_string()
       , r#"http://myurl.local/1234/1234/1234"#.to_string(), 0, Some(true)).is_err());
        assert!(get_channel_from_lines(r#"#EXTINF:-1 tvg-id=" " tvg-name="" tvg-logo="http://myurl.local/logos/amazing/amazing-1.png" group-title="The Best Channels"#.to_string()
       , r#"http://myurl.local/1234/1234/1234"#.to_string(), 0, Some(true)).is_err());
        assert!(get_channel_from_lines(r#"#EXTINF:-1 tvg-id="Id Of Channel" tvg-name="Name Of Channel" tvg-logo="http://myurl.local/amazing/stuff.png" group-title="|EU| FRANCE HEVC",Alt Name Of Channel"#.to_string(), "http://myurl.local/1111/1111.ts".to_string(), 0, Some(true)).unwrap().name == "Name Of Channel");
        assert!(get_channel_from_lines(r#"#EXTINF:-1 tvg-id="Id Of Channel" tvg-name="" tvg-logo="http://myurl.local/amazing/stuff.png" group-title="|EU| FRANCE HEVC",Alt Name Of Channel"#.to_string(), "http://myurl.local/1111/1111.ts".to_string(), 0, Some(true)).unwrap().name == "Id Of Channel");
        assert!(get_channel_from_lines(r#"#EXTINF:-1 tvg-id="Id Of Channel" tvg-name="" tvg-logo="http://myurl.local/amazing/stuff.png" group-title="|EU| FRANCE HEVC",Alt Name Of Channel"#.to_string(), "http://myurl.local/1111/1111.ts".to_string(), 0, Some(false)).unwrap().name == "Alt Name Of Channel");
    }

    #[test]
    fn test_get_channel_from_lines_use_tvg_id_none_defaults_like_false() {
        // use_tvg_id: None should fall back to preferring the comma-
        // separated display name over tvg-id, same as Some(false) - it's
        // only Some(true) that flips the priority.
        let channel = get_channel_from_lines(
            r#"#EXTINF:-1 tvg-id="Id Of Channel" tvg-name="" tvg-logo="" group-title="News",Alt Name Of Channel"#.to_string(),
            "http://myurl.local/1111/1111.ts".to_string(),
            0,
            None,
        )
        .unwrap();
        assert_eq!(channel.name, "Alt Name Of Channel");
    }

    #[test]
    fn test_get_channel_from_lines_populates_all_fields() {
        let channel = get_channel_from_lines(
            r#"#EXTINF:-1 tvg-id="chan.id" tvg-name="Channel Name" tvg-logo="http://myurl.local/logo.png" group-title="Sports""#
                .to_string(),
            "http://myurl.local/1234/1234/1234.ts".to_string(),
            42,
            Some(true),
        )
        .unwrap();
        assert_eq!(channel.name, "Channel Name");
        assert_eq!(channel.tvg_id, Some("chan.id".to_string()));
        assert_eq!(channel.group, Some("Sports".to_string()));
        assert_eq!(channel.image, Some("http://myurl.local/logo.png".to_string()));
        assert_eq!(
            channel.url,
            Some("http://myurl.local/1234/1234/1234.ts".to_string())
        );
        assert_eq!(channel.source_id, Some(42));
        assert_eq!(channel.media_type, media_type::LIVESTREAM);
    }

    #[test]
    fn test_name_regex_alt_ignores_comma_inside_an_earlier_attribute() {
        // group-title contains an unescaped comma - the real title
        // separator is still the *last* comma on the line, not the first.
        let channel = get_channel_from_lines(
            r#"#EXTINF:-1 tvg-id="" tvg-name="" group-title="Sports, General",Real Channel Name"#
                .to_string(),
            "http://myurl.local/1111/1111.ts".to_string(),
            0,
            Some(false),
        )
        .unwrap();
        assert_eq!(channel.name, "Real Channel Name");
    }

    #[test]
    fn test_get_media_type() {
        assert_eq!(get_media_type("http://x/movie.mp4".to_string()), media_type::MOVIE);
        assert_eq!(get_media_type("http://x/movie.mkv".to_string()), media_type::MOVIE);
        assert_eq!(
            get_media_type("http://x/stream.ts".to_string()),
            media_type::LIVESTREAM
        );
        assert_eq!(get_media_type("http://x/stream".to_string()), media_type::LIVESTREAM);
    }

    #[test]
    fn test_set_http_headers() {
        let mut headers = ChannelHttpHeaders::default();
        assert!(set_http_headers(
            "#EXTVLCOPT:http-origin=http://example.com",
            &mut headers
        ));
        assert_eq!(headers.http_origin, Some("http://example.com".to_string()));

        let mut headers = ChannelHttpHeaders::default();
        assert!(set_http_headers(
            "#EXTVLCOPT:http-referrer=http://example.com",
            &mut headers
        ));
        assert_eq!(headers.referrer, Some("http://example.com".to_string()));

        let mut headers = ChannelHttpHeaders::default();
        assert!(set_http_headers(
            "#EXTVLCOPT:http-user-agent=SomeAgent/1.0",
            &mut headers
        ));
        assert_eq!(headers.user_agent, Some("SomeAgent/1.0".to_string()));

        let mut headers = ChannelHttpHeaders::default();
        assert!(!set_http_headers("#EXTVLCOPT:some-other-option=value", &mut headers));
        assert_eq!(headers, ChannelHttpHeaders::default());
    }

    #[test]
    fn test_extract_non_empty_capture_treats_whitespace_as_empty() {
        let re = Regex::new(r#"x="(?P<v>[^"]*)""#).unwrap();
        let caps = re.captures(r#"x="   ""#).unwrap();
        assert_eq!(extract_non_empty_capture(caps), None);

        let caps = re.captures(r#"x="real-value""#).unwrap();
        assert_eq!(extract_non_empty_capture(caps), Some("real-value".to_string()));
    }

    #[test]
    fn test_get_channel_from_lines_attribute_order_and_duplicates() {
        // Attribute order on the line shouldn't matter, and a duplicated
        // attribute should resolve to its *first* occurrence.
        let channel = get_channel_from_lines(
            r#"#EXTINF:-1 group-title="News" tvg-logo="http://x/logo.png" tvg-name="Channel Name" tvg-id="first" tvg-id="second""#
                .to_string(),
            "http://x/stream.ts".to_string(),
            0,
            Some(true),
        )
        .unwrap();
        assert_eq!(channel.tvg_id, Some("first".to_string()));
        assert_eq!(channel.name, "Channel Name");
        assert_eq!(channel.group, Some("News".to_string()));
        assert_eq!(channel.image, Some("http://x/logo.png".to_string()));
    }
}
