use crate::log;
use crate::media_type;
use crate::source_type;
use crate::sql;
use crate::sql::insert_season;
use crate::types::Channel;
use crate::types::ChannelPreserve;
use crate::types::MediaInfo;
use crate::types::Season;
use crate::types::Source;
use crate::types::XtreamAccount;
use crate::types::XtreamStatus;
use crate::utils::get_user_agent_from_source;
use anyhow::anyhow;
use anyhow::{Context, Result};
use chrono::TimeZone;
use chrono::Utc;
use futures::future::join_all;
use rusqlite::Transaction;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::str::FromStr;
use tokio::join;
use url::Url;

const GET_LIVE_STREAMS: &str = "get_live_streams";
const GET_VODS: &str = "get_vod_streams";
const GET_SERIES: &str = "get_series";
const GET_SERIES_INFO: &str = "get_series_info";
const GET_SERIES_CATEGORIES: &str = "get_series_categories";
const GET_LIVE_STREAM_CATEGORIES: &str = "get_live_categories";
const GET_VOD_CATEGORIES: &str = "get_vod_categories";
const LIVE_STREAM_EXTENSION: &str = "ts";
const NO_SEASON_NUMBER: i64 = -9999;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct XtreamStream {
    #[serde(default)]
    stream_id: serde_json::Value,
    name: Option<String>,
    #[serde(default)]
    category_id: serde_json::Value,
    stream_icon: Option<String>,
    #[serde(default)]
    series_id: serde_json::Value,
    cover: Option<String>,
    container_extension: Option<String>,
    #[serde(default)]
    tv_archive: serde_json::Value,
    #[serde(default)]
    epg_channel_id: serde_json::Value,
    // Not part of the official Xtream Codes spec, but many panels include
    // it per live/VOD/series list item ("0"/"1", 0/1, or a real bool
    // depending on the panel) - #[serde(default)] so it's simply absent
    // (Value::Null) on panels that don't send it at all.
    #[serde(default)]
    is_adult: serde_json::Value,
    // Movies and series only, out of 10 - "6.5", 6.5, "" or absent.
    #[serde(default)]
    rating: serde_json::Value,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct XtreamSeries {
    #[serde(default, deserialize_with = "deserialize_seasons")]
    seasons: Vec<XtreamSeason>,
    #[serde(default, deserialize_with = "deserialize_episodes")]
    episodes: HashMap<String, Vec<XtreamEpisode>>,
}

// Some Xtream panels return an empty array `[]` instead of an empty object
// `{}` (and vice versa) for these fields when a series has no
// seasons/episodes yet - a long-standing quirk of PHP's json_encode() on an
// empty associative array, which can't be told apart from an empty list.
// Treat "wrong but empty-shaped" as empty instead of failing to parse the
// whole series (see the "invalid type: sequence, expected a map" report).
fn deserialize_seasons<'de, D>(deserializer: D) -> std::result::Result<Vec<XtreamSeason>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Array(_) => {
            serde_json::from_value(value).map_err(serde::de::Error::custom)
        }
        _ => Ok(Vec::new()),
    }
}

fn deserialize_episodes<'de, D>(
    deserializer: D,
) -> std::result::Result<HashMap<String, Vec<XtreamEpisode>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Object(_) => {
            serde_json::from_value(value).map_err(serde::de::Error::custom)
        }
        _ => Ok(HashMap::new()),
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct XtreamSeason {
    #[serde(default)]
    season_number: serde_json::Value,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    cover: Option<String>,
    #[serde(default)]
    cover_tmdb: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct XtreamEpisode {
    id: serde_json::Value,
    title: String,
    container_extension: String,
    #[serde(default)]
    episode_num: serde_json::Value,
    #[serde(default)]
    season: serde_json::Value,
    #[serde(default)]
    info: serde_json::Value,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct XtreamEpisodeInfo {
    movie_image: Option<String>,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct XtreamCategory {
    #[serde(default)]
    category_id: serde_json::Value,
    category_name: String,
}

/// The Xtream API address for a server address as people usually type it:
/// "host:port" becomes "http://host:port/player_api.php". Panels answer
/// 401 at the bare server address. An address with a path is left alone.
pub fn api_url(server: &str) -> String {
    let server = server.trim();
    let url = if server.contains("://") {
        server.to_string()
    } else {
        format!("http://{server}")
    };
    match Url::parse(&url) {
        Ok(mut parsed) if parsed.path().is_empty() || parsed.path() == "/" => {
            parsed.set_path("/player_api.php");
            parsed.to_string()
        }
        _ => url,
    }
}

fn build_xtream_url(source: &mut Source) -> Result<Url> {
    let mut url = Url::parse(&source.url.clone().context("Missing URL")?)?;
    source.url_origin = Some(
        Url::from_str(&source.url.clone().unwrap())?
            .origin()
            .ascii_serialization(),
    );
    url.query_pairs_mut()
        .append_pair(
            "username",
            &source.username.clone().context("Missing username")?,
        )
        .append_pair(
            "password",
            &source.password.clone().context("Missing password")?,
        );
    Ok(url)
}

// Xtream Codes panels also expose a bulk XMLTV dump for the whole
// playlist at xmltv.php - a single reliable request instead of the
// flaky per-channel get_simple_data_table calls in get_epg() below.
// Reuses build_xtream_url for its username/password query pairs (and
// url_origin side effect), just swapping the path.
pub async fn refresh_xtream_epg(mut source: Source) -> Result<()> {
    let mut url = build_xtream_url(&mut source)?;
    url.set_path("/xmltv.php");
    crate::xmltv::refresh_epg_from_url(source, url.to_string()).await
}

pub async fn get_xtream(mut source: Source, wipe: bool) -> Result<()> {
    let refresh_start = std::time::Instant::now();
    let url = build_xtream_url(&mut source)?;
    let user_agent = get_user_agent_from_source(&source)?;
    // A source can leave out its TV channels, or its movies and series:
    // those lists aren't downloaded, so it ends up with none of them.
    let live_on = source.include_live != Some(false);
    let vod_on = source.include_vod != Some(false);
    let fetch_start = std::time::Instant::now();
    let (live, live_cats, vods, vods_cats, series, series_cats) = join!(
        get_xtream_list::<XtreamStream>(live_on, url.clone(), GET_LIVE_STREAMS, &user_agent),
        get_xtream_list::<XtreamCategory>(
            live_on,
            url.clone(),
            GET_LIVE_STREAM_CATEGORIES,
            &user_agent
        ),
        get_xtream_list::<XtreamStream>(vod_on, url.clone(), GET_VODS, &user_agent),
        get_xtream_list::<XtreamCategory>(vod_on, url.clone(), GET_VOD_CATEGORIES, &user_agent),
        get_xtream_list::<XtreamStream>(vod_on, url.clone(), GET_SERIES, &user_agent),
        get_xtream_list::<XtreamCategory>(vod_on, url.clone(), GET_SERIES_CATEGORIES, &user_agent),
    );
    // Timing is phase-level rather than per-request because the 6 calls
    // above run concurrently via join! - wall-clock time for this block is
    // ~the slowest single request, not their sum, so timing each one
    // separately would be misleading about where end-to-end time actually
    // goes. Logged (not just measured) so a real refresh through the app
    // shows the breakdown directly in the dev log, without attaching a
    // profiler.
    log::log(format!(
        "[perf] {}: network fetch (6 concurrent requests) took {:?}",
        source.name,
        fetch_start.elapsed()
    ));
    let db_start = std::time::Instant::now();
    let mut sql = sql::get_conn()?;
    let tx = sql.transaction()?;
    let mut channel_preserve: Vec<ChannelPreserve> = Vec::new();
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
        sql::wipe(&tx, source.id.context("Source should have id")?)?;
    } else {
        source.id = Some(sql::create_or_find_source_by_name(&tx, &source)?);
    }
    log::log(format!(
        "[perf] {}: wipe/setup took {:?}",
        source.name,
        db_start.elapsed()
    ));
    let mut fail_count = 0;
    let mut failures: Vec<String> = Vec::new();
    let process_start = std::time::Instant::now();
    live.and_then(|live| process_xtream(&tx, live, live_cats?, &source, media_type::LIVESTREAM))
        .unwrap_or_else(|e| {
            let e = e.context("Failed to process live streams");
            log::log(format!("{:?}", e));
            failures.push(format!("{:?}", e));
            fail_count += 1;
        });
    log::log(format!(
        "[perf] {}: process live took {:?}",
        source.name,
        process_start.elapsed()
    ));
    let vods_start = std::time::Instant::now();
    vods.and_then(|vods: Vec<XtreamStream>| {
        process_xtream(&tx, vods, vods_cats?, &source, media_type::MOVIE)
    })
    .unwrap_or_else(|e| {
        let e = e.context("Failed to process VODs");
        log::log(format!("{:?}", e));
        failures.push(format!("{:?}", e));
        fail_count += 1;
    });
    log::log(format!(
        "[perf] {}: process vods took {:?}",
        source.name,
        vods_start.elapsed()
    ));
    let series_start = std::time::Instant::now();
    series
        .and_then(|series: Vec<XtreamStream>| {
            process_xtream(&tx, series, series_cats?, &source, media_type::SERIE)
        })
        .unwrap_or_else(|e| {
            let e = e.context("Failed to process series");
            log::log(format!("{:?}", e));
            failures.push(format!("{:?}", e));
            fail_count += 1;
        });
    log::log(format!(
        "[perf] {}: process series took {:?}",
        source.name,
        series_start.elapsed()
    ));
    if fail_count > 2 {
        match tx.rollback() {
            Ok(_) => {}
            Err(e) => log::log(format!("Failed to rollback tx: {:?}", e)),
        }
        return Err(anyhow::anyhow!(
            "Xtream refresh failed for '{}':\n{}",
            source.name,
            failures.join("\n")
        ));
    }
    let finalize_start = std::time::Instant::now();
    if wipe {
        sql::restore_preserve(&tx, source.id.context("no source id")?, channel_preserve)?;
    }
    sql::analyze(&tx)?;
    tx.commit()?;
    log::log(format!(
        "[perf] {}: preserve/analyze/commit took {:?}",
        source.name,
        finalize_start.elapsed()
    ));
    log::log(format!(
        "[perf] {}: total get_xtream took {:?}",
        source.name,
        refresh_start.elapsed()
    ));
    Ok(())
}

// A list from the panel, or an empty one without asking when it's left out.
async fn get_xtream_list<T>(
    include: bool,
    url: Url,
    action: &str,
    user_agent: &String,
) -> Result<Vec<T>>
where
    T: serde::de::DeserializeOwned,
{
    if !include {
        return Ok(Vec::new());
    }
    get_xtream_http_data(url, action, user_agent).await
}

async fn get_xtream_http_data<T>(mut url: Url, action: &str, user_agent: &String) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let client = crate::utils::new_http_client(user_agent)?;
    url.query_pairs_mut().append_pair("action", action);
    let response = client.get(url).send().await?;
    parse_xtream_json(response, &format!("action '{action}'")).await
}

// A panel returning a non-2xx status, an empty body, or an HTML error page
// (rather than JSON) all used to surface as the same unhelpful
// `expected value at line 1 column 1` from .json() - this instead reports
// which of those actually happened, with a preview of what came back.
async fn parse_xtream_json<T>(response: reqwest::Response, context_label: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!(
            "Xtream API returned HTTP {} for {context_label}. Body preview: {}",
            status.as_u16(),
            truncate_for_preview(&text)
        ));
    }
    if text.is_empty() {
        return Err(anyhow!(
            "Xtream API returned an empty body for {context_label}."
        ));
    }
    serde_json::from_str(&text).map_err(|e| {
        anyhow!(
            "Failed to parse JSON from Xtream API for {context_label}: {e}. Body preview: {}",
            truncate_for_preview(&text)
        )
    })
}

// Truncates on a char boundary rather than a raw byte index - the bodies
// this wraps (HTML error pages, panel-specific error messages) can contain
// multi-byte UTF-8 characters, and slicing by byte index panics if it lands
// mid-character.
fn truncate_for_preview(text: &str) -> String {
    const PREVIEW_LEN: usize = 200;
    text.chars().take(PREVIEW_LEN).collect()
}

// pub so the integration test and benchmark (both compiled as separate
// crates) can drive the real JSON-to-database pipeline directly, without
// needing XtreamStream/XtreamCategory/process_xtream themselves to be pub -
// takes raw JSON text, the same shape a real panel response would have.
pub fn process_xtream_json(
    tx: &Transaction,
    streams_json: &str,
    categories_json: &str,
    source: &Source,
    stream_type: u8,
) -> Result<()> {
    let streams: Vec<XtreamStream> = serde_json::from_str(streams_json)?;
    let cats: Vec<XtreamCategory> = serde_json::from_str(categories_json)?;
    process_xtream(tx, streams, cats, source, stream_type)
}

fn process_xtream(
    tx: &Transaction,
    streams: Vec<XtreamStream>,
    cats: Vec<XtreamCategory>,
    source: &Source,
    stream_type: u8,
) -> Result<()> {
    let cats: HashMap<String, String> = cats
        .into_iter()
        .filter_map(|f| {
            let category_id = get_serde_json_string(&f.category_id);
            category_id.map(|cid| (cid, f.category_name))
        })
        .collect();
    let mut groups: HashMap<String, i64> = HashMap::new();
    // Collected into a Vec and inserted in one batched pass below rather
    // than one execute() per channel - same rationale as
    // insert_epg_programmes_batch, and this is the phase that showed up as
    // "process live/vods/series" in refresh timing.
    let mut channels: Vec<Channel> = Vec::with_capacity(streams.len());
    for live in streams {
        let category_name = get_cat_name(&cats, get_serde_json_string(&live.category_id));
        convert_xtream_live_to_channel(live, &source, stream_type.clone(), category_name)
            .map(|mut channel| {
                sql::set_channel_group_id(
                    &mut groups,
                    &mut channel,
                    &tx,
                    source.id.as_ref().unwrap(),
                )
                .unwrap_or_else(|e| log::log(format!("{:?}", e)));
                channels.push(channel);
            })
            .unwrap_or_else(|e| log::log(format!("{:?}", e)));
    }
    sql::insert_channels_batch(&tx, &channels)?;
    Ok(())
}

fn get_cat_name(cats: &HashMap<String, String>, category_id: Option<String>) -> Option<String> {
    if category_id.is_none() {
        return None;
    }
    return cats.get(&category_id.unwrap()).map(|t| t.to_string());
}

fn convert_xtream_live_to_channel(
    stream: XtreamStream,
    source: &Source,
    stream_type: u8,
    category_name: Option<String>,
) -> Result<Channel> {
    let stream_id = get_serde_json_u64(&stream.stream_id);
    let tvg_id =
        get_serde_json_string(&stream.epg_channel_id).filter(|id| !id.is_empty() && id != "0");
    Ok(Channel {
        id: None,
        group: category_name.map(|x| x.trim().to_string()),
        image: stream
            .stream_icon
            .or(stream.cover)
            .map(|x| x.trim().to_string()),
        media_type: stream_type.clone(),
        name: stream.name.context("No name")?.trim().to_string(),
        source_id: source.id,
        url: if stream_type == media_type::SERIE {
            get_serde_json_string(&stream.series_id)
        } else {
            Some(get_url(
                stream_id.context("missing stream id")?.to_string(),
                source,
                stream_type,
                stream.container_extension,
            )?)
        },
        stream_id,
        favorite: false,
        group_id: None,
        series_id: None,
        tv_archive: get_serde_json_u64(&stream.tv_archive).map(|x| x == 1),
        tvg_id,
        season_id: None,
        episode_num: None,
        hidden: Some(false),
        is_adult: get_serde_json_bool(&stream.is_adult).unwrap_or(false),
        rating: get_rating(&stream.rating),
    })
}

// Panels send 0 or "" for films nobody has rated.
fn get_rating(value: &serde_json::Value) -> Option<f64> {
    let rating = match value {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }?;
    (rating > 0.0 && rating <= 10.0).then_some(rating)
}

fn get_url(
    stream_id: String,
    source: &Source,
    stream_type: u8,
    extension: Option<String>,
) -> Result<String> {
    // Live channels play in the source's output format, if it has one.
    let extension = match &source.output_format {
        Some(format) if stream_type == media_type::LIVESTREAM => Some(format.clone()),
        _ => extension,
    };
    Ok(format!(
        "{}/{}/{}/{}/{}.{}",
        source.url_origin.clone().unwrap(),
        get_media_type_string(stream_type)?,
        source.username.clone().unwrap(),
        source.password.clone().unwrap(),
        stream_id,
        extension.unwrap_or(LIVE_STREAM_EXTENSION.to_string())
    ))
}

fn get_media_type_string(stream_type: u8) -> Result<String> {
    match stream_type {
        media_type::LIVESTREAM => Ok("live".to_string()),
        media_type::MOVIE => Ok("movie".to_string()),
        media_type::SERIE => Ok("series".to_string()),
        _ => Err(anyhow!("Invalid stream_type")),
    }
}

pub async fn get_episodes(channel: Channel) -> Result<()> {
    let series_id = channel.url.context("no url")?.parse()?;
    if sql::series_has_episodes(series_id, channel.source_id.context("no source id")?)
        .unwrap_or_else(|e| {
            log::log(format!("{:?}", e));
            return false;
        })
    {
        return Ok(());
    }
    let mut source = sql::get_source_from_id(channel.source_id.context("no source id")?)?;
    let mut url = build_xtream_url(&mut source)?;
    let user_agent = get_user_agent_from_source(&source)?;
    url.query_pairs_mut()
        .append_pair("series_id", &series_id.to_string());
    let mut series =
        get_xtream_http_data::<XtreamSeries>(url, GET_SERIES_INFO, &user_agent).await?;
    let mut episodes: Vec<XtreamEpisode> = series
        .episodes
        .into_values()
        .flat_map(|episode| episode)
        .collect();
    series
        .seasons
        .sort_by_key(|f| get_serde_json_i64(&f.season_number));
    let seasons: HashMap<i64, XtreamSeason> = series
        .seasons
        .iter()
        .filter_map(|x| get_serde_json_i64(&x.season_number).map(|y| (y, x.clone())))
        .collect();
    episodes.sort_by(|a, b| {
        get_serde_json_u64(&a.season)
            .cmp(&get_serde_json_u64(&b.season))
            .then_with(|| {
                get_serde_json_u64(&a.episode_num).cmp(&get_serde_json_u64(&b.episode_num))
            })
    });
    insert_episodes(&source, seasons, episodes, series_id, channel.image)?;
    Ok(())
}

// Ensures the series' episodes are populated (get_episodes is idempotent -
// skips the network call if already fetched) then returns all of them
// across every season in one flat list, for "Download Series".
pub async fn get_series_episodes_for_download(
    channel: Channel,
) -> Result<Vec<crate::types::SeriesEpisode>> {
    let series_id: u64 = channel
        .url
        .clone()
        .context("no url")?
        .parse()
        .context("invalid series id")?;
    let source_id = channel.source_id.context("no source id")?;
    get_episodes(channel).await?;
    sql::get_series_episodes(source_id, series_id)
}

// No get_episodes() call needed here (unlike the series-level function
// above) - a season pseudo-channel only exists on screen once its parent
// series has already been clicked into, which already fetched/populated
// this data.
pub fn get_season_episodes_for_download(
    season_channel: Channel,
) -> Result<crate::types::SeasonDownloadInfo> {
    let source_id = season_channel.source_id.context("no source id")?;
    let series_id = season_channel.series_id.context("no series id")?;
    let series_name = sql::get_series_name(source_id, series_id)?.context("series not found")?;
    let episodes = sql::get_season_episodes(season_channel.id.context("no season id")?)?;
    Ok(crate::types::SeasonDownloadInfo {
        series_name,
        episodes,
    })
}

fn insert_episodes(
    source: &Source,
    seasons: HashMap<i64, XtreamSeason>,
    episodes: Vec<XtreamEpisode>,
    series_id: u64,
    default_season_image: Option<String>,
) -> Result<()> {
    let mut seasons_db: HashMap<i64, i64> = HashMap::new();
    sql::do_tx(|tx| {
        for episode in episodes {
            match insert_episode(
                episode.clone(),
                source,
                tx,
                &mut seasons_db,
                &seasons,
                series_id,
                default_season_image.clone(),
            )
            .with_context(|| format!("Failed to insert episode {:?}", episode))
            {
                Ok(_) => (),
                Err(e) => {
                    log::log(format!("{:?}", e));
                    continue;
                }
            }
        }
        Ok(())
    })
}

fn insert_episode(
    episode: XtreamEpisode,
    source: &Source,
    tx: &Transaction,
    seasons_db: &mut HashMap<i64, i64>,
    seasons: &HashMap<i64, XtreamSeason>,
    series_id: u64,
    default_season_image: Option<String>,
) -> Result<()> {
    let season_number = get_serde_json_i64(&episode.season).unwrap_or(NO_SEASON_NUMBER);
    let season_id = seasons_db.get(&season_number);
    let season_id: i64 = match season_id {
        Some(s) => s.clone(),
        None => {
            let season = seasons
                .get(&season_number)
                .and_then(|f| {
                    xtream_season_to_season(f.clone(), source.id.unwrap(), series_id)
                        .with_context(|| "Failed to convert XtreamSeason to Season")
                        .inspect_err(|e| log::log(format!("{}", e)))
                        .ok()
                })
                .unwrap_or_else(|| {
                    create_makeshift_season(
                        season_number,
                        series_id,
                        source.id.unwrap(),
                        default_season_image,
                    )
                });
            let id = insert_season(tx, season)?;
            seasons_db.insert(season_number, id);
            id
        }
    };
    let episode = episode_to_channel(episode, &source, series_id, season_id)?;
    sql::insert_channel(&tx, episode)?;
    Ok(())
}

fn create_makeshift_season(
    number: i64,
    series_id: u64,
    source_id: i64,
    image: Option<String>,
) -> Season {
    Season {
        name: match number == NO_SEASON_NUMBER {
            true => "Uncategorized".to_string(),
            false => format!("Season {number}"),
        },
        series_id,
        season_number: number,
        source_id,
        image,
        ..Default::default()
    }
}

fn get_serde_json_string(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(|cid| cid.to_string())
        .or_else(|| value.as_u64().map(|cid| cid.to_string()))
        .map(|cid| cid.trim().to_string())
}

fn get_serde_json_u64(value: &serde_json::Value) -> Option<u64> {
    value
        .as_str()
        .and_then(|val| val.trim().parse::<u64>().ok())
        .or_else(|| value.as_u64())
}

fn get_serde_json_i64(value: &serde_json::Value) -> Option<i64> {
    value
        .as_str()
        .and_then(|val| val.trim().parse::<i64>().ok())
        .or_else(|| value.as_i64())
}

// Panels are inconsistent about whether flags like is_adult are sent as a
// real JSON bool, a "0"/"1" string, or a 0/1 number.
fn get_serde_json_bool(value: &serde_json::Value) -> Option<bool> {
    value
        .as_bool()
        .or_else(|| get_serde_json_u64(value).map(|v| v == 1))
}

fn xtream_season_to_season(season: XtreamSeason, source_id: i64, series_id: u64) -> Result<Season> {
    let season_number = get_serde_json_i64(&season.season_number).context("no season number")?;
    Ok(Season {
        season_number,
        series_id,
        source_id,
        image: season.cover_tmdb.or(season.cover).or(season.overview),
        name: format!("Season {season_number}"),
        ..Default::default()
    })
}

fn episode_to_channel(
    episode: XtreamEpisode,
    source: &Source,
    series_id: u64,
    season_id: i64,
) -> Result<Channel> {
    Ok(Channel {
        id: None,
        group: None,
        image: serde_json::from_value::<XtreamEpisodeInfo>(episode.info)
            .map(|e| e.movie_image)
            .unwrap_or_default(),
        media_type: media_type::MOVIE,
        name: episode.title.trim().to_string(),
        source_id: source.id,
        url: Some(get_url(
            get_serde_json_string(&episode.id).context("no id")?,
            &source,
            media_type::SERIE,
            Some(episode.container_extension),
        )?),
        series_id: Some(series_id),
        episode_num: get_serde_json_i64(&episode.episode_num),
        season_id: Some(season_id),
        stream_id: None,
        group_id: None,
        favorite: false,
        tv_archive: None,
        tvg_id: None,
        hidden: Some(false),
        // Xtream's per-episode info blob doesn't carry its own is_adult -
        // only the parent series' list entry does (already stored on that
        // series' own Channel row).
        is_adult: false,
        rating: None,
    })
}

// Xtream's own per-channel EPG endpoint (get_simple_data_table) is what
// this used to rely on to discover catch-up availability - but it's proven
// unreliable for this specific purpose: providers commonly only return a
// short rolling window (today, or a day or two), so a programme several
// days back (now that EPG retention/panning makes that reachable) simply
// isn't in the response, no matter how the result is filtered or matched.
// Timeshift playback itself only ever needed the programme's start/end and
// the channel's stream id - all of which are already known from whichever
// EPG source populated the clicked entry (bulk XMLTV or otherwise) - so
// build the URL directly from that instead of depending on this API at all.
pub async fn get_timeshift_url_for_epg(
    channel: Channel,
    start_timestamp: i64,
    end_timestamp: i64,
) -> Result<String> {
    let mut source = sql::get_source_from_id(channel.source_id.context("no source id")?)?;
    // source.url_origin is never persisted to the DB - it's only ever set
    // as a side effect of build_xtream_url(), so it has to be called here
    // explicitly or get_timeshift_url_base() below fails with "no origin".
    build_xtream_url(&mut source)?;
    let stream_id = channel.stream_id.context("No stream id")?.to_string();
    // timeshift.php wants the start time expressed in the panel's own local
    // time, not UTC and not the viewer's local time. Confirmed by testing
    // against a news channel's on-screen clock: both raw-UTC and
    // BST-converted (UTC+1) requests landed exactly 1 hour earlier on
    // screen than whatever was sent, while this account's reported
    // Europe/Amsterdam timezone (CEST = UTC+2 in September) landed exactly
    // on time. Rather than hardcoding +2h - which would break once
    // Amsterdam falls back to CET for winter, or for any other provider
    // hosted elsewhere - fetch the panel's own reported timezone and use it
    // to compute the correct wall-clock offset for this specific instant.
    let tz = get_source_timezone(&mut source).await?;
    let start_local = Utc
        .timestamp_opt(start_timestamp, 0)
        .single()
        .context("invalid start timestamp")?
        .with_timezone(&tz)
        .format("%Y-%m-%d:%H-%M")
        .to_string();
    let duration_minutes = (end_timestamp - start_timestamp).max(0) / 60;
    // The standard Xtream Codes catch-up endpoint is a path-based URL, not
    // the streaming/timeshift.php query-string form (which returned a clean
    // 404 - not an auth or param error - strongly suggesting that endpoint
    // just doesn't exist on this panel). Catch-up content is served as
    // segmented HLS rather than a direct transport stream on many panels,
    // even when live channels are .ts, so this deliberately doesn't reuse
    // LIVE_STREAM_EXTENSION.
    let url = format!(
        "{}/timeshift/{}/{}/{}/{}/{}.m3u8",
        source.url_origin.as_ref().context("no origin")?,
        source.username.as_ref().context("no username")?,
        source.password.as_ref().context("no password")?,
        duration_minutes,
        start_local,
        stream_id,
    );
    Ok(url)
}

async fn get_status(source: &mut Source) -> Result<(i64, XtreamStatus)> {
    let url = build_xtream_url(source)?;
    let user_agent = get_user_agent_from_source(&source)?;
    let client = crate::utils::new_http_client(&user_agent)?;
    let response = client.get(url).send().await?;
    let data: XtreamStatus = parse_xtream_json(response, "get_status").await?;
    Ok((source.id.context("no id")?, data))
}

// Falls back to UTC rather than failing outright when the panel doesn't
// report a timezone (or reports one chrono_tz doesn't recognize) - without
// this, get_timeshift_url_for_epg would propagate the error and catch-up
// would just silently be unavailable for the whole source.
async fn get_source_timezone(source: &mut Source) -> Result<chrono_tz::Tz> {
    let (_, status) = get_status(source).await?;
    let tz_name = status.server_info.and_then(|info| info.timezone);
    let tz_name = match tz_name {
        Some(tz_name) => tz_name,
        None => {
            log::log(format!(
                "Source '{}' did not report a timezone, defaulting to UTC for timeshift URLs",
                source.name
            ));
            return Ok(chrono_tz::Tz::UTC);
        }
    };
    chrono_tz::Tz::from_str(&tz_name).or_else(|e| {
        log::log(format!(
            "Source '{}' reported unrecognized timezone '{tz_name}' ({e}), defaulting to UTC for timeshift URLs",
            source.name
        ));
        Ok(chrono_tz::Tz::UTC)
    })
}

pub async fn get_all_expiries() -> Result<HashMap<i64, i64>> {
    let mut sources = sql::get_sources_by_type(source_type::XTREAM)?;
    let to_await = sources.iter_mut().map(|source| get_status(source));
    let results: Vec<std::result::Result<(i64, XtreamStatus), anyhow::Error>> =
        join_all(to_await).await;
    let statuses: HashMap<i64, i64> = results
        .into_iter()
        .flatten()
        .filter_map(|(id, status)| {
            let exp_date = get_serde_json_i64(&status.user_info.exp_date)?;
            Some((id, exp_date))
        })
        .collect();
    Ok(statuses)
}

pub async fn get_account(source_id: i64) -> Result<XtreamAccount> {
    let mut source = sql::get_source_from_id(source_id)?;
    let (_, status) = get_status(&mut source).await?;
    Ok(XtreamAccount {
        expires: get_serde_json_i64(&status.user_info.exp_date),
        max_connections: get_serde_json_u64(&status.user_info.max_connections)
            .and_then(|n| u32::try_from(n).ok()),
    })
}

// Surfaced in settings so it's clear which timezone convention the timeshift
// URL (see get_timeshift_url_for_epg) is computing against for this source.
pub async fn get_all_timezones() -> Result<HashMap<i64, String>> {
    let mut sources = sql::get_sources_by_type(source_type::XTREAM)?;
    let to_await = sources.iter_mut().map(|source| get_status(source));
    let results: Vec<std::result::Result<(i64, XtreamStatus), anyhow::Error>> =
        join_all(to_await).await;
    let timezones: HashMap<i64, String> = results
        .into_iter()
        .flatten()
        .filter_map(|(id, status)| Some((id, status.server_info?.timezone?)))
        .collect();
    Ok(timezones)
}

const GET_VOD_INFO: &str = "get_vod_info";

#[derive(Deserialize)]
struct XtreamInfoPage {
    #[serde(default)]
    info: serde_json::Value,
}

// A movie's or series' details from its info page. Movies are looked up by
// stream id, series by the series id kept in their url.
pub async fn get_media_info(channel: Channel) -> Result<MediaInfo> {
    let mut source = sql::get_source_from_id(channel.source_id.context("no source id")?)?;
    let mut url = build_xtream_url(&mut source)?;
    let user_agent = get_user_agent_from_source(&source)?;
    let action = if channel.media_type == media_type::SERIE {
        let series_id = channel.url.context("no series id")?;
        url.query_pairs_mut().append_pair("series_id", &series_id);
        GET_SERIES_INFO
    } else {
        let stream_id = channel.stream_id.context("no stream id")?;
        url.query_pairs_mut()
            .append_pair("vod_id", &stream_id.to_string());
        GET_VOD_INFO
    };
    let page = get_xtream_http_data::<XtreamInfoPage>(url, action, &user_agent).await?;
    Ok(media_info_from_json(&page.info))
}

// Panels differ in which keys they fill in, and send "" or [] for missing
// ones, so this takes the first key with a real value.
fn media_info_from_json(info: &serde_json::Value) -> MediaInfo {
    let text = |keys: &[&str]| {
        keys.iter().find_map(|key| {
            let value = match &info[key] {
                serde_json::Value::Array(items) => items.first()?.as_str()?.trim(),
                value => value.as_str()?.trim(),
            };
            (!value.is_empty()).then(|| value.to_string())
        })
    };
    let duration_secs = get_serde_json_u64(&info["duration_secs"])
        .or_else(|| text(&["duration_secs"])?.parse().ok())
        .or_else(|| {
            let minutes: u64 = text(&["episode_run_time"])?.parse().ok()?;
            Some(minutes * 60)
        })
        .filter(|secs| *secs > 0);
    MediaInfo {
        plot: text(&["plot", "description"]),
        cast: text(&["cast", "actors"]),
        director: text(&["director"]),
        genre: text(&["genre"]),
        year: text(&["releasedate", "releaseDate", "release_date"])
            .and_then(|date| date.get(..4).map(str::to_string)),
        duration_secs,
        rating: get_rating(&info["rating"]),
        backdrop: text(&["backdrop_path"]),
    }
}

#[cfg(test)]
mod test_xtream {
    use super::*;
    use serde_json::json;

    fn test_source() -> Source {
        Source {
            id: Some(1),
            name: "Test Source".to_string(),
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

    #[test]
    fn test_get_serde_json_string() {
        assert_eq!(
            get_serde_json_string(&json!("hello")),
            Some("hello".to_string())
        );
        assert_eq!(
            get_serde_json_string(&json!(" padded ")),
            Some("padded".to_string())
        );
        // Panels are inconsistent about sending category/stream ids as a
        // real JSON number instead of a string - both must resolve to the
        // same thing.
        assert_eq!(get_serde_json_string(&json!(42)), Some("42".to_string()));
        assert_eq!(get_serde_json_string(&json!(null)), None);
        assert_eq!(get_serde_json_string(&json!(true)), None);
    }

    #[test]
    fn test_get_serde_json_u64() {
        assert_eq!(get_serde_json_u64(&json!(42)), Some(42));
        assert_eq!(get_serde_json_u64(&json!("42")), Some(42));
        assert_eq!(get_serde_json_u64(&json!(" 42 ")), Some(42));
        assert_eq!(get_serde_json_u64(&json!("not a number")), None);
        assert_eq!(get_serde_json_u64(&json!(null)), None);
    }

    #[test]
    fn test_get_serde_json_i64() {
        assert_eq!(get_serde_json_i64(&json!(-5)), Some(-5));
        assert_eq!(get_serde_json_i64(&json!("-5")), Some(-5));
        assert_eq!(get_serde_json_i64(&json!(null)), None);
    }

    #[test]
    fn test_get_serde_json_bool() {
        // Panels send is_adult (and similar flags) as a real bool, a
        // "0"/"1" string, or a 0/1 number, interchangeably.
        assert_eq!(get_serde_json_bool(&json!(true)), Some(true));
        assert_eq!(get_serde_json_bool(&json!(false)), Some(false));
        assert_eq!(get_serde_json_bool(&json!(1)), Some(true));
        assert_eq!(get_serde_json_bool(&json!(0)), Some(false));
        assert_eq!(get_serde_json_bool(&json!("1")), Some(true));
        assert_eq!(get_serde_json_bool(&json!("0")), Some(false));
        assert_eq!(get_serde_json_bool(&json!(null)), None);
    }

    #[test]
    fn test_deserialize_seasons_tolerates_wrong_empty_shape() {
        let normal: XtreamSeries =
            serde_json::from_value(json!({ "seasons": [{"season_number": 1}], "episodes": {} }))
                .unwrap();
        assert_eq!(normal.seasons.len(), 1);

        // Some panels send `{}` instead of `[]` for an empty seasons list -
        // a long-standing PHP json_encode() quirk (see the "invalid type:
        // sequence, expected a map" report) - must not fail the whole series.
        let wrong_shape: XtreamSeries =
            serde_json::from_value(json!({ "seasons": {}, "episodes": [] })).unwrap();
        assert_eq!(wrong_shape.seasons.len(), 0);
    }

    #[test]
    fn test_deserialize_episodes_tolerates_wrong_empty_shape() {
        let normal: XtreamSeries = serde_json::from_value(json!({
            "seasons": [],
            "episodes": {"1": [{"id": 1, "title": "Ep 1", "container_extension": "mp4"}]}
        }))
        .unwrap();
        assert_eq!(normal.episodes.get("1").unwrap().len(), 1);

        // The reverse of the seasons case - an empty array instead of `{}`.
        let wrong_shape: XtreamSeries =
            serde_json::from_value(json!({ "seasons": [], "episodes": [] })).unwrap();
        assert_eq!(wrong_shape.episodes.len(), 0);
    }

    fn fake_response(status: u16, body: &str) -> reqwest::Response {
        http::Response::builder()
            .status(status)
            .body(body.to_string())
            .unwrap()
            .into()
    }

    #[derive(serde::Deserialize, Debug)]
    struct DummyPayload {
        ok: bool,
    }

    #[tokio::test]
    async fn test_parse_xtream_json_success() {
        let data: DummyPayload = parse_xtream_json(fake_response(200, r#"{"ok": true}"#), "test")
            .await
            .unwrap();
        assert!(data.ok);
    }

    #[tokio::test]
    async fn test_parse_xtream_json_non_2xx_status() {
        let err = parse_xtream_json::<DummyPayload>(fake_response(502, "Bad Gateway"), "test")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("502"));
    }

    #[tokio::test]
    async fn test_parse_xtream_json_empty_body() {
        let err = parse_xtream_json::<DummyPayload>(fake_response(200, ""), "test")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("empty body"));
    }

    #[tokio::test]
    async fn test_parse_xtream_json_invalid_json_does_not_panic_on_multibyte_body() {
        // A non-ASCII error page (e.g. a provider's own HTML error in a
        // non-English language) must not panic truncate_for_preview, which
        // is exactly the kind of body this error path exists to describe.
        let body = "é".repeat(150);
        let err = parse_xtream_json::<DummyPayload>(fake_response(200, &body), "test")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Failed to parse JSON"));
    }

    #[test]
    fn test_get_media_type_string() {
        assert_eq!(
            get_media_type_string(media_type::LIVESTREAM).unwrap(),
            "live"
        );
        assert_eq!(get_media_type_string(media_type::MOVIE).unwrap(), "movie");
        assert_eq!(get_media_type_string(media_type::SERIE).unwrap(), "series");
        assert!(get_media_type_string(media_type::GROUP).is_err());
    }

    #[test]
    fn test_get_url_uses_the_output_format_for_live_channels() {
        let mut source = test_source();
        assert_eq!(
            get_url("1".into(), &source, media_type::LIVESTREAM, None).unwrap(),
            "http://panel.example/live/user/pass/1.ts"
        );
        source.output_format = Some("m3u8".into());
        assert_eq!(
            get_url("1".into(), &source, media_type::LIVESTREAM, None).unwrap(),
            "http://panel.example/live/user/pass/1.m3u8"
        );
        // Films keep their own container.
        assert_eq!(
            get_url("2".into(), &source, media_type::MOVIE, Some("mkv".into())).unwrap(),
            "http://panel.example/movie/user/pass/2.mkv"
        );
    }

    #[test]
    fn test_create_makeshift_season() {
        let season = create_makeshift_season(3, 10, 1, None);
        assert_eq!(season.name, "Season 3");
        assert_eq!(season.series_id, 10);
        assert_eq!(season.source_id, 1);

        let uncategorized = create_makeshift_season(NO_SEASON_NUMBER, 10, 1, None);
        assert_eq!(uncategorized.name, "Uncategorized");
    }

    #[test]
    fn test_xtream_season_to_season() {
        assert!(
            xtream_season_to_season(
                XtreamSeason {
                    season_number: json!(null),
                    overview: None,
                    cover: None,
                    cover_tmdb: None,
                },
                1,
                10,
            )
            .is_err(),
            "a season with no season_number at all should error, not panic"
        );

        let season = xtream_season_to_season(
            XtreamSeason {
                season_number: json!(2),
                overview: Some("overview".to_string()),
                cover: Some("cover".to_string()),
                cover_tmdb: Some("cover_tmdb".to_string()),
            },
            1,
            10,
        )
        .unwrap();
        assert_eq!(season.name, "Season 2");
        // cover_tmdb takes priority over cover, which takes priority over
        // overview.
        assert_eq!(season.image, Some("cover_tmdb".to_string()));
    }

    #[test]
    fn test_get_cat_name() {
        let mut cats = HashMap::new();
        cats.insert("5".to_string(), "Sports".to_string());
        assert_eq!(
            get_cat_name(&cats, Some("5".to_string())),
            Some("Sports".to_string())
        );
        assert_eq!(get_cat_name(&cats, Some("missing".to_string())), None);
        assert_eq!(get_cat_name(&cats, None), None);
    }

    #[test]
    fn test_convert_xtream_live_to_channel_filters_zero_and_empty_tvg_id() {
        let source = test_source();
        // Many panels send epg_channel_id: "0" (or "") to mean "no EPG
        // mapping" rather than omitting the field - both must become None,
        // not a literal tvg_id of "0" that'd never match any real EPG entry.
        let stream = XtreamStream {
            stream_id: json!(100),
            name: Some("Series Name".to_string()),
            category_id: json!("5"),
            stream_icon: None,
            series_id: json!(100),
            cover: None,
            container_extension: None,
            tv_archive: json!(null),
            epg_channel_id: json!("0"),
            is_adult: json!(null),
            rating: json!(null),
        };
        let channel = convert_xtream_live_to_channel(
            stream,
            &source,
            media_type::SERIE,
            Some("Drama".to_string()),
        )
        .unwrap();
        assert_eq!(channel.tvg_id, None);
        assert_eq!(channel.name, "Series Name");
        assert_eq!(channel.group, Some("Drama".to_string()));

        let stream_with_real_id = XtreamStream {
            stream_id: json!(101),
            name: Some("Other Series".to_string()),
            category_id: json!("5"),
            stream_icon: None,
            series_id: json!(101),
            cover: None,
            container_extension: None,
            tv_archive: json!(null),
            epg_channel_id: json!("real.epg.id"),
            is_adult: json!(null),
            rating: json!(null),
        };
        let channel =
            convert_xtream_live_to_channel(stream_with_real_id, &source, media_type::SERIE, None)
                .unwrap();
        assert_eq!(channel.tvg_id, Some("real.epg.id".to_string()));
    }

    #[test]
    fn a_bare_server_address_gets_the_api_path() {
        assert_eq!(
            api_url("http://panel.example:8080"),
            "http://panel.example:8080/player_api.php"
        );
        assert_eq!(
            api_url("panel.example:8080/ "),
            "http://panel.example:8080/player_api.php"
        );
        assert_eq!(
            api_url("https://panel.example/player_api.php"),
            "https://panel.example/player_api.php"
        );
        assert_eq!(
            api_url("http://panel.example/custom/path"),
            "http://panel.example/custom/path"
        );
    }

    #[test]
    fn reads_ratings_as_strings_or_numbers() {
        assert_eq!(get_rating(&json!("6.5")), Some(6.5));
        assert_eq!(get_rating(&json!(7.9)), Some(7.9));
        assert_eq!(get_rating(&json!(8)), Some(8.0));
        // Unrated films come as 0, "" or nothing.
        assert_eq!(get_rating(&json!("0")), None);
        assert_eq!(get_rating(&json!("")), None);
        assert_eq!(get_rating(&json!(null)), None);
    }

    #[test]
    fn reads_a_movie_info_page() {
        let info = media_info_from_json(&json!({
            "plot": "A volcanologist faces two disasters at once.",
            "description": "",
            "cast": "",
            "actors": "Vigdís Hrefna Pálsdóttir, Pilou Asbæk",
            "director": "Ugla Hauksdóttir",
            "genre": "Thriller",
            "releasedate": "2025-03-14",
            "duration_secs": 6300,
            "rating": "6.5",
            "backdrop_path": ["https://image.example/backdrop.jpg"],
        }));
        assert_eq!(
            info,
            MediaInfo {
                plot: Some("A volcanologist faces two disasters at once.".to_string()),
                cast: Some("Vigdís Hrefna Pálsdóttir, Pilou Asbæk".to_string()),
                director: Some("Ugla Hauksdóttir".to_string()),
                genre: Some("Thriller".to_string()),
                year: Some("2025".to_string()),
                duration_secs: Some(6300),
                rating: Some(6.5),
                backdrop: Some("https://image.example/backdrop.jpg".to_string()),
            }
        );
    }

    #[test]
    fn reads_a_series_info_page() {
        let info = media_info_from_json(&json!({
            "plot": "",
            "releaseDate": "2019",
            "episode_run_time": "45",
            "rating": 0,
            "backdrop_path": [],
        }));
        assert_eq!(info.plot, None);
        assert_eq!(info.year, Some("2019".to_string()));
        assert_eq!(info.duration_secs, Some(45 * 60));
        assert_eq!(info.rating, None);
        assert_eq!(info.backdrop, None);
    }

    #[test]
    fn an_empty_info_page_has_no_details() {
        // Panels send [] instead of {} when a film has no details.
        assert_eq!(media_info_from_json(&json!([])), MediaInfo::default());
    }
}
