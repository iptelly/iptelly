use crate::external_player;
use crate::types::{AppState, ChannelHttpHeaders, Source};
use crate::utils::get_bin;
use crate::{media_type, settings::get_settings, sql, types::Channel};
use anyhow::{Context, Result};

use std::sync::LazyLock;
use tokio::sync::Mutex;

const ARG_PLAY_AND_EXIT: &str = "--play-and-exit";
const ARG_TITLE: &str = "--meta-title=";
const ARG_GAIN: &str = "--gain=";
const ARG_HWDEC_ON: &str = "--avcodec-hw=any";
const ARG_HWDEC_OFF: &str = "--avcodec-hw=none";
const ARG_NETWORK_CACHING: &str = "--network-caching=";
const ARG_USER_AGENT: &str = "--http-user-agent=";
const ARG_REFERRER: &str = "--http-referrer=";
const VLC_BIN_NAME: &str = "vlc";
static VLC_PATH: LazyLock<String> = LazyLock::new(|| get_bin(VLC_BIN_NAME));

// VLC is never asked to record (see api::play - recording
// always routes to mpv, which is the only one of the three players with a
// working --stream-record equivalent wired up), so this is narrower than
// mpv::play: no record/record_path params.
pub async fn play(channel: Channel, state: &Mutex<AppState>) -> Result<()> {
    let source = channel
        .source_id
        .and_then(|id| {
            sql::get_source_from_id(id)
                .with_context(|| format!("failed to fetch source with id {}", id))
                .ok()
        })
        .or(None);
    let args = get_play_args(&channel, &source)?;
    external_player::run(&VLC_PATH, args, &channel, &source, state).await
}

fn get_play_args(channel: &Channel, source: &Option<Source>) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let settings = get_settings()?;
    let headers = sql::get_channel_headers_by_id(channel.id.context("no channel id?")?)?;
    args.push(ARG_PLAY_AND_EXIT.to_string());
    args.push(format!("{ARG_TITLE}{}", channel.name));
    if settings.use_stream_caching == Some(false) {
        args.push(format!("{ARG_NETWORK_CACHING}300"));
    }
    if settings.enable_hwdec.unwrap_or(true) {
        args.push(ARG_HWDEC_ON.to_string());
    } else {
        args.push(ARG_HWDEC_OFF.to_string());
    }
    if channel.media_type == media_type::LIVESTREAM {
        args.push("--loop".to_string());
    }
    if let Some(volume) = settings.volume {
        args.push(format!("{ARG_GAIN}{}", volume as f32 / 100.0));
    }
    set_headers(headers, &mut args, source);
    if let Some(vlc_params) = settings.vlc_params {
        #[cfg(not(target_os = "windows"))]
        let mut params = shell_words::split(&vlc_params)?;
        #[cfg(target_os = "windows")]
        let mut params = winsplit::split(&vlc_params);
        args.append(&mut params);
    }
    // Everything after this point is treated as a URL, never an option -
    // protects against a malicious playlist/provider crafting a channel
    // URL that looks like a VLC flag.
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
    if let Some(referrer) = headers.referrer {
        args.push(format!("{ARG_REFERRER}{referrer}"));
    }
    if let Some(user_agent) = headers
        .user_agent
        .or_else(|| source.as_ref().and_then(|f| f.stream_user_agent.clone()))
    {
        args.push(format!("{ARG_USER_AGENT}{user_agent}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{settings, test_db};

    fn header_args(headers: Option<ChannelHttpHeaders>, source: &Option<Source>) -> Vec<String> {
        let mut args = Vec::new();
        set_headers(headers, &mut args, source);
        args
    }

    fn source_with_user_agent(user_agent: &str) -> Option<Source> {
        let mut source = sql::get_custom_source("unused".to_string());
        source.stream_user_agent = Some(user_agent.to_string());
        Some(source)
    }

    // Everything before the "--" separator - the options VLC will parse.
    fn options(args: &[String]) -> &[String] {
        let end = args
            .iter()
            .position(|a| a == "--")
            .expect("no -- separator");
        &args[..end]
    }

    fn livestream(source_name: &str) -> Channel {
        let source = test_db::add_source(source_name);
        test_db::add_channel(
            &source,
            test_db::channel(
                "News",
                "http://example.com/live/1.ts",
                media_type::LIVESTREAM,
            ),
            None,
        )
    }

    #[test]
    fn set_headers_adds_nothing_without_headers_or_a_source() {
        assert!(header_args(None, &None).is_empty());
    }

    #[test]
    fn set_headers_passes_the_referrer_and_channel_user_agent() {
        let args = header_args(
            Some(ChannelHttpHeaders {
                referrer: Some("https://referrer.example".to_string()),
                user_agent: Some("Channel UA".to_string()),
                ..Default::default()
            }),
            &source_with_user_agent("Source UA"),
        );
        assert_eq!(
            args,
            [
                "--http-referrer=https://referrer.example",
                "--http-user-agent=Channel UA",
            ]
        );
    }

    #[test]
    fn set_headers_falls_back_to_the_source_user_agent() {
        let args = header_args(None, &source_with_user_agent("Source UA"));
        assert_eq!(args, ["--http-user-agent=Source UA"]);
    }

    #[test]
    fn the_url_always_comes_after_the_option_separator() {
        let _db = test_db::lock();
        test_db::set_setting(settings::VLC_PARAMS, "--no-audio");
        let source = test_db::add_source("vlc url separator");
        // A malicious playlist entry crafted to look like a VLC option.
        let channel = test_db::add_channel(
            &source,
            test_db::channel("Evil", "--extraintf=lua", media_type::LIVESTREAM),
            None,
        );
        let args = get_play_args(&channel, &None).unwrap();
        assert_eq!(&args[args.len() - 2..], ["--", "--extraintf=lua"]);
        assert!(options(&args).contains(&"--no-audio".to_string()));
    }

    #[test]
    fn livestreams_loop_with_default_settings() {
        let _db = test_db::lock();
        let channel = livestream("vlc livestream defaults");
        assert_eq!(
            get_play_args(&channel, &None).unwrap(),
            [
                ARG_PLAY_AND_EXIT,
                "--meta-title=News",
                ARG_HWDEC_ON,
                "--loop",
                "--",
                "http://example.com/live/1.ts",
            ]
        );
    }

    #[test]
    fn movies_do_not_loop() {
        let _db = test_db::lock();
        let source = test_db::add_source("vlc movie");
        let channel = test_db::add_channel(
            &source,
            test_db::channel("Film", "http://example.com/movie/1.mkv", media_type::MOVIE),
            None,
        );
        assert!(
            !get_play_args(&channel, &None)
                .unwrap()
                .contains(&"--loop".to_string())
        );
    }

    #[test]
    fn applies_playback_settings() {
        let _db = test_db::lock();
        test_db::set_setting(settings::ENABLE_HWDEC, "false");
        test_db::set_setting(settings::USE_STREAM_CACHING, "false");
        test_db::set_setting(settings::VOLUME, "50");
        let channel = livestream("vlc settings");
        let args = get_play_args(&channel, &None).unwrap();
        let options = options(&args);
        assert!(!options.contains(&ARG_HWDEC_ON.to_string()));
        for expected in [ARG_HWDEC_OFF, "--network-caching=300", "--gain=0.5"] {
            assert!(
                options.contains(&expected.to_string()),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn splits_custom_vlc_params_like_a_shell() {
        let _db = test_db::lock();
        test_db::set_setting(
            settings::VLC_PARAMS,
            r#"--sub-file="/tmp/my subs.srt" --no-audio"#,
        );
        let channel = livestream("vlc params");
        let args = get_play_args(&channel, &None).unwrap();
        let options = options(&args);
        assert_eq!(
            &options[options.len() - 2..],
            ["--sub-file=/tmp/my subs.srt", "--no-audio"]
        );
    }

    #[test]
    fn queues_the_rest_of_the_season_after_an_episode() {
        let _db = test_db::lock();
        let source = test_db::add_source("vlc episodes");
        let season_id = test_db::add_season(&source, 9002);
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

        let args = get_play_args(&second, &None).unwrap();
        let after_separator = &args[options(&args).len() + 1..];
        assert_eq!(
            after_separator,
            [
                "http://example.com/series/2.mkv",
                "http://example.com/series/3.mkv"
            ]
        );
    }
}
