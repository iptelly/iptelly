use crate::settings::get_default_record_path;
use crate::types::{AppState, ChannelHttpHeaders, Source};
use crate::utils::{find_macos_bin, get_bin};
use crate::{external_player, log, sql};
use crate::{media_type, settings::get_settings, types::Channel};
use anyhow::{Context, Result};
use chrono::Local;

use std::sync::LazyLock;
use std::{env::consts::OS, path::Path};
use tauri::State;
use tokio::sync::Mutex;

const ARG_SAVE_POSITION_ON_QUIT: &str = "--save-position-on-quit";
const ARG_CACHE: &str = "--cache=";
const ARG_NO: &str = "no";
const ARG_RECORD: &str = "--stream-record=";
const ARG_TITLE: &str = "--title=";
const ARG_MSG_LEVEL: &str = "--msg-level=all=error";
const ARG_YTDLP_PATH: &str = "--script-opts=ytdl_hook-ytdl_path=";
const ARG_VOLUME: &str = "--volume=";
const ARG_HTTP_HEADERS: &str = "--http-header-fields=";
const ARG_USER_AGENT: &str = "--user-agent=";
const ARG_IGNORE_SSL: &str = "--ytdl-raw-options=no-check-certificates=True";
const ARG_PREFETCH_PLAYLIST: &str = "--prefetch-playlist=yes";
const ARG_LOOP_PLAYLIST: &str = "--loop-playlist=inf";
const ARG_HWDEC: &str = "--hwdec=auto";
const ARG_GPU_NEXT: &str = "--vo=gpu-next";
const ARG_GPU_PROFILE_HIGH_QUALITY: &str = "--profile=high-quality";
const ARG_NO_RESUME_PLAYBACK: &str = "--no-resume-playback";
const ARG_STREAM_LAVF_LOCAL_ADDR: &str = "--stream-lavf-o=local_addr=";
const ARG_STREAM_LAVF_INTERFACE: &str = ",interface=";
const MPV_BIN_NAME: &str = "mpv";
const YTDLP_BIN_NAME: &str = "yt-dlp";
const HTTP_ORIGIN: &str = "origin:";
const HTTP_REFERRER: &str = "referer:";
static MPV_PATH: LazyLock<String> = LazyLock::new(|| get_bin(MPV_BIN_NAME));
static YTDLP_PATH: LazyLock<String> = LazyLock::new(|| find_macos_bin(YTDLP_BIN_NAME));

pub async fn play(
    channel: Channel,
    record: bool,
    record_path: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<()> {
    eprintln!(
        "{} playing",
        channel.url.as_ref().context("no channel url")?
    );
    let source = channel
        .source_id
        .and_then(|id| {
            sql::get_source_from_id(id)
                .with_context(|| format!("failed to fetch source with id {}", id))
                .ok()
        })
        .or(None);
    let args = get_play_args(&channel, record, record_path, &source)?;
    external_player::run(&MPV_PATH, args, &channel, &source, &state).await
}

pub async fn cancel_play(
    source_id: i64,
    key: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<()> {
    log::log(format!("Cancelling play for channel: {}", key));
    let token = crate::utils::remove_from_play_stop(state, &source_id, &key).await?;
    let token = token.context("no channel found")?;
    token.cancel();
    Ok(())
}

fn get_play_args(
    channel: &Channel,
    record: bool,
    record_path: Option<String>,
    source: &Option<Source>,
) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let settings = get_settings()?;
    let headers = sql::get_channel_headers_by_id(channel.id.context("no channel id?")?)?;
    if channel.episode_num.is_some() {
        args.push(ARG_NO_RESUME_PLAYBACK.to_string());
    }
    if channel.media_type != media_type::LIVESTREAM {
        args.push(ARG_SAVE_POSITION_ON_QUIT.to_string());
    }
    if settings.use_stream_caching == Some(false) {
        let stream_caching_arg = format!("{ARG_CACHE}{ARG_NO}",);
        args.push(stream_caching_arg);
    }
    // Catch-up/timeshift streams commonly don't start exactly on a clean
    // keyframe boundary (the server seeks into an already-recorded
    // segment), which hardware decoding tends to choke on - confirmed via
    // testing: the same URL played fine with --hwdec=no but produced
    // "non-existing PPS referenced" / decoder failures with --hwdec=auto.
    // Regular live/VOD playback is unaffected and keeps using hwdec.
    let is_timeshift = channel
        .url
        .as_deref()
        .is_some_and(|url| url.contains("/timeshift/"));
    if settings.enable_hwdec.unwrap_or(true) && !is_timeshift {
        args.push(ARG_HWDEC.to_string());
    }
    if settings.enable_gpu.unwrap_or(false) {
        args.push(ARG_GPU_NEXT.to_string());
        args.push(ARG_GPU_PROFILE_HIGH_QUALITY.to_string());
    }
    if let Some(address) = settings.network_interface.as_ref() {
        args.push(format!(
            "{ARG_STREAM_LAVF_LOCAL_ADDR}{address}{ARG_STREAM_LAVF_INTERFACE}{address}"
        ));
    }
    if record {
        let path = if let Some(p) = record_path {
            p
        } else if let Some(p) = settings.recording_path.map(get_path) {
            p
        } else {
            get_path(get_default_record_path()?)
        };
        args.push(format!("{ARG_RECORD}{path}"));
    }
    if OS == "macos" && *MPV_PATH != MPV_BIN_NAME {
        args.push(format!("{}{}", ARG_YTDLP_PATH, *YTDLP_PATH));
    }
    args.push(format!("{}{}", ARG_TITLE, channel.name));
    args.push(ARG_MSG_LEVEL.to_string());
    if channel.media_type == media_type::LIVESTREAM {
        args.push(ARG_PREFETCH_PLAYLIST.to_string());
        args.push(ARG_LOOP_PLAYLIST.to_string());
    }
    if let Some(volume) = settings.volume {
        args.push(format!("{ARG_VOLUME}{volume}"));
    }
    if headers.is_some() || source.is_some() {
        set_headers(headers, &mut args, source);
    }
    if let Some(mpv_params) = settings.mpv_params {
        #[cfg(not(target_os = "windows"))]
        let mut params = shell_words::split(&mpv_params)?;
        #[cfg(target_os = "windows")]
        let mut params = winsplit::split(&mpv_params);
        args.append(&mut params);
    }
    // Everything after this point is treated as a filename/URL, never an
    // option - protects against a malicious playlist/provider crafting a
    // channel URL that looks like an mpv flag (e.g. --script=... or
    // --input-ipc-server=...), which mpv would otherwise happily parse as
    // one regardless of its position in argv.
    args.push("--".to_string());
    args.push(channel.url.clone().context("no url")?);
    if channel.episode_num.is_some() {
        for url in sql::find_all_episodes_after(channel)? {
            args.push(url);
        }
    }
    Ok(args)
}

fn set_headers(
    headers: Option<ChannelHttpHeaders>,
    args: &mut Vec<String>,
    source: &Option<Source>,
) {
    let headers = headers.unwrap_or_default();
    let mut headers_vec: Vec<String> = Vec::with_capacity(2);
    if let Some(origin) = headers.http_origin {
        headers_vec.push(format!("{HTTP_ORIGIN}{origin}"));
    }
    if let Some(referrer) = headers.referrer {
        headers_vec.push(format!("{HTTP_REFERRER}{referrer}"));
    }
    if let Some(user_agent) = headers
        .user_agent
        .or_else(|| source.as_ref().and_then(|f| f.stream_user_agent.clone()))
    {
        args.push(format!("{ARG_USER_AGENT}{user_agent}"));
    }
    if let Some(ignore_ssl) = headers.ignore_ssl {
        if ignore_ssl == true {
            args.push(ARG_IGNORE_SSL.to_string());
        }
    }
    if headers_vec.len() > 0 {
        let headers = headers_vec.join(",");
        args.push(format!("{ARG_HTTP_HEADERS}{headers}"));
    }
}

fn get_path(path_str: String) -> String {
    let path = Path::new(&path_str);
    let path = path.join(get_file_name());
    return path.to_string_lossy().to_string();
}

fn get_file_name() -> String {
    let current_time = Local::now();
    let formatted_time = current_time.format("%Y-%m-%d-%H-%M-%S").to_string();
    format!("{formatted_time}.mp4")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{settings, test_db};

    fn headers() -> ChannelHttpHeaders {
        ChannelHttpHeaders::default()
    }

    fn source_with_user_agent(user_agent: &str) -> Option<Source> {
        let mut source = sql::get_custom_source("unused".to_string());
        source.stream_user_agent = Some(user_agent.to_string());
        Some(source)
    }

    fn header_args(headers: Option<ChannelHttpHeaders>, source: &Option<Source>) -> Vec<String> {
        let mut args = Vec::new();
        set_headers(headers, &mut args, source);
        args
    }

    // Everything before the "--" separator - the options mpv will parse.
    fn options(args: &[String]) -> &[String] {
        let end = args
            .iter()
            .position(|a| a == "--")
            .expect("no -- separator");
        &args[..end]
    }

    fn play_args(channel: &Channel) -> Vec<String> {
        get_play_args(channel, false, None, &None).unwrap()
    }

    #[test]
    fn set_headers_adds_nothing_without_headers_or_a_source() {
        assert!(header_args(None, &None).is_empty());
        assert!(header_args(Some(headers()), &None).is_empty());
    }

    #[test]
    fn set_headers_combines_origin_and_referrer_into_one_argument() {
        let args = header_args(
            Some(ChannelHttpHeaders {
                http_origin: Some("https://origin.example".to_string()),
                referrer: Some("https://referrer.example".to_string()),
                ..headers()
            }),
            &None,
        );
        assert_eq!(
            args,
            ["--http-header-fields=origin:https://origin.example,referer:https://referrer.example"]
        );
    }

    #[test]
    fn set_headers_prefers_the_channel_user_agent_over_the_source() {
        let args = header_args(
            Some(ChannelHttpHeaders {
                user_agent: Some("Channel UA".to_string()),
                ..headers()
            }),
            &source_with_user_agent("Source UA"),
        );
        assert_eq!(args, ["--user-agent=Channel UA"]);
    }

    #[test]
    fn set_headers_falls_back_to_the_source_user_agent() {
        let args = header_args(None, &source_with_user_agent("Source UA"));
        assert_eq!(args, ["--user-agent=Source UA"]);
    }

    #[test]
    fn set_headers_only_skips_certificate_checks_when_asked_to() {
        let ignore = |value| {
            header_args(
                Some(ChannelHttpHeaders {
                    ignore_ssl: Some(value),
                    ..headers()
                }),
                &None,
            )
        };
        assert_eq!(ignore(true), [ARG_IGNORE_SSL]);
        assert!(ignore(false).is_empty());
    }

    #[test]
    fn the_url_always_comes_after_the_option_separator() {
        let _db = test_db::lock();
        test_db::set_setting(settings::MPV_PARAMS, "--mute=yes");
        let source = test_db::add_source("mpv url separator");
        // A malicious playlist entry crafted to look like an mpv option.
        let channel = test_db::add_channel(
            &source,
            test_db::channel("Evil", "--script=/tmp/evil.lua", media_type::LIVESTREAM),
            None,
        );
        let args = play_args(&channel);
        assert_eq!(&args[args.len() - 2..], ["--", "--script=/tmp/evil.lua"]);
        assert!(!options(&args).contains(&"--script=/tmp/evil.lua".to_string()));
        assert!(options(&args).contains(&"--mute=yes".to_string()));
    }

    #[test]
    fn livestreams_loop_and_prefetch_with_default_settings() {
        let _db = test_db::lock();
        let source = test_db::add_source("mpv livestream defaults");
        let channel = test_db::add_channel(
            &source,
            test_db::channel(
                "News",
                "http://example.com/live/1.ts",
                media_type::LIVESTREAM,
            ),
            None,
        );
        assert_eq!(
            play_args(&channel),
            [
                ARG_HWDEC,
                "--title=News",
                ARG_MSG_LEVEL,
                ARG_PREFETCH_PLAYLIST,
                ARG_LOOP_PLAYLIST,
                "--",
                "http://example.com/live/1.ts",
            ]
        );
    }

    #[test]
    fn movies_save_their_position_and_do_not_loop() {
        let _db = test_db::lock();
        let source = test_db::add_source("mpv movie");
        let channel = test_db::add_channel(
            &source,
            test_db::channel("Film", "http://example.com/movie/1.mkv", media_type::MOVIE),
            None,
        );
        let args = play_args(&channel);
        assert!(args.contains(&ARG_SAVE_POSITION_ON_QUIT.to_string()));
        assert!(!args.contains(&ARG_LOOP_PLAYLIST.to_string()));
        assert!(!args.contains(&ARG_PREFETCH_PLAYLIST.to_string()));
    }

    #[test]
    fn timeshift_streams_never_use_hardware_decoding() {
        let _db = test_db::lock();
        test_db::set_setting(settings::ENABLE_HWDEC, "true");
        let source = test_db::add_source("mpv timeshift");
        let channel = test_db::add_channel(
            &source,
            test_db::channel(
                "Catch-up",
                "http://example.com/timeshift/u/p/60/2026-01-01:12-00/1.ts",
                media_type::LIVESTREAM,
            ),
            None,
        );
        assert!(!play_args(&channel).contains(&ARG_HWDEC.to_string()));
    }

    #[test]
    fn applies_playback_settings() {
        let _db = test_db::lock();
        test_db::set_setting(settings::ENABLE_HWDEC, "false");
        test_db::set_setting(settings::USE_STREAM_CACHING, "false");
        test_db::set_setting(settings::ENABLE_GPU, "true");
        test_db::set_setting(settings::VOLUME, "50");
        test_db::set_setting(settings::NETWORK_INTERFACE, "10.0.0.2");
        let source = test_db::add_source("mpv settings");
        let channel = test_db::add_channel(
            &source,
            test_db::channel(
                "News",
                "http://example.com/live/1.ts",
                media_type::LIVESTREAM,
            ),
            None,
        );
        let args = play_args(&channel);
        let options = options(&args);
        assert!(!options.contains(&ARG_HWDEC.to_string()));
        for expected in [
            "--cache=no",
            ARG_GPU_NEXT,
            ARG_GPU_PROFILE_HIGH_QUALITY,
            "--volume=50",
            "--stream-lavf-o=local_addr=10.0.0.2,interface=10.0.0.2",
        ] {
            assert!(
                options.contains(&expected.to_string()),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn splits_custom_mpv_params_like_a_shell() {
        let _db = test_db::lock();
        test_db::set_setting(
            settings::MPV_PARAMS,
            r#"--sub-font="DejaVu Sans" --mute=yes"#,
        );
        let source = test_db::add_source("mpv params");
        let channel = test_db::add_channel(
            &source,
            test_db::channel(
                "News",
                "http://example.com/live/1.ts",
                media_type::LIVESTREAM,
            ),
            None,
        );
        let args = play_args(&channel);
        let options = options(&args);
        assert_eq!(
            &options[options.len() - 2..],
            ["--sub-font=DejaVu Sans", "--mute=yes"]
        );
    }

    #[test]
    fn rejects_unparseable_mpv_params() {
        let _db = test_db::lock();
        test_db::set_setting(settings::MPV_PARAMS, r#"--sub-font="unclosed"#);
        let source = test_db::add_source("mpv bad params");
        let channel = test_db::add_channel(
            &source,
            test_db::channel(
                "News",
                "http://example.com/live/1.ts",
                media_type::LIVESTREAM,
            ),
            None,
        );
        assert!(get_play_args(&channel, false, None, &None).is_err());
    }

    #[test]
    fn records_to_the_given_path() {
        let _db = test_db::lock();
        let source = test_db::add_source("mpv record path");
        let channel = test_db::add_channel(
            &source,
            test_db::channel(
                "News",
                "http://example.com/live/1.ts",
                media_type::LIVESTREAM,
            ),
            None,
        );
        let args = get_play_args(&channel, true, Some("/tmp/news.mp4".to_string()), &None).unwrap();
        assert!(options(&args).contains(&"--stream-record=/tmp/news.mp4".to_string()));
    }

    #[test]
    fn records_into_the_recording_path_setting() {
        let _db = test_db::lock();
        test_db::set_setting(settings::RECORDING_PATH, "/tmp/recordings");
        let source = test_db::add_source("mpv recording setting");
        let channel = test_db::add_channel(
            &source,
            test_db::channel(
                "News",
                "http://example.com/live/1.ts",
                media_type::LIVESTREAM,
            ),
            None,
        );
        let args = get_play_args(&channel, true, None, &None).unwrap();
        let record = options(&args)
            .iter()
            .find_map(|a| a.strip_prefix(ARG_RECORD))
            .expect("no record argument");
        let record = Path::new(record);
        assert_eq!(record.parent(), Some(Path::new("/tmp/recordings")));
        assert_eq!(record.extension().unwrap(), "mp4");
    }

    #[test]
    fn uses_the_channel_headers_from_the_database() {
        let _db = test_db::lock();
        let source = test_db::add_source("mpv db headers");
        let channel = test_db::add_channel(
            &source,
            test_db::channel(
                "News",
                "http://example.com/live/1.ts",
                media_type::LIVESTREAM,
            ),
            Some(ChannelHttpHeaders {
                referrer: Some("https://referrer.example".to_string()),
                user_agent: Some("Channel UA".to_string()),
                ..headers()
            }),
        );
        let args = play_args(&channel);
        let options = options(&args);
        assert!(options.contains(&"--user-agent=Channel UA".to_string()));
        assert!(
            options.contains(&"--http-header-fields=referer:https://referrer.example".to_string())
        );
    }

    #[test]
    fn queues_the_rest_of_the_season_after_an_episode() {
        let _db = test_db::lock();
        let source = test_db::add_source("mpv episodes");
        let season_id = test_db::add_season(&source, 9001);
        let episode = |num: i64| {
            let mut channel = test_db::channel(
                &format!("Episode {num}"),
                &format!("http://example.com/series/{num}.mkv"),
                media_type::SERIE,
            );
            channel.season_id = Some(season_id);
            channel.episode_num = Some(num);
            test_db::add_channel(&source, channel, None)
        };
        let _first = episode(1);
        let second = episode(2);
        let _third = episode(3);
        let _fourth = episode(4);

        let args = play_args(&second);
        assert!(options(&args).contains(&ARG_NO_RESUME_PLAYBACK.to_string()));
        let after_separator = &args[options(&args).len() + 1..];
        assert_eq!(
            after_separator,
            [
                "http://example.com/series/2.mkv",
                "http://example.com/series/3.mkv",
                "http://example.com/series/4.mkv",
            ]
        );
    }

    #[test]
    fn needs_a_channel_id_and_url() {
        let _db = test_db::lock();
        let channel = test_db::channel(
            "News",
            "http://example.com/live/1.ts",
            media_type::LIVESTREAM,
        );
        assert!(get_play_args(&channel, false, None, &None).is_err());

        let source = test_db::add_source("mpv no url");
        let mut channel = test_db::add_channel(&source, channel, None);
        channel.url = None;
        assert!(get_play_args(&channel, false, None, &None).is_err());
    }
}
