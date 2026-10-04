use crate::playback::{self, PlayRequest};
use crate::types::{AppState, Source};
use crate::utils::get_bin;
use crate::{external_player, settings::get_settings, types::Channel};
use anyhow::Result;

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
    let source = playback::get_source(&channel);
    let args = get_play_args(&channel, &source)?;
    external_player::run(&VLC_PATH, args, &channel, &source, state).await
}

fn get_play_args(channel: &Channel, source: &Option<Source>) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let settings = get_settings()?;
    let request = playback::build(channel, source, &settings)?;
    args.push(ARG_PLAY_AND_EXIT.to_string());
    args.push(format!("{ARG_TITLE}{}", request.title));
    if !request.stream_caching {
        args.push(format!("{ARG_NETWORK_CACHING}300"));
    }
    if request.hardware_decoding {
        args.push(ARG_HWDEC_ON.to_string());
    } else {
        args.push(ARG_HWDEC_OFF.to_string());
    }
    if request.live {
        args.push("--loop".to_string());
    }
    if let Some(volume) = request.volume {
        args.push(format!("{ARG_GAIN}{}", volume as f32 / 100.0));
    }
    set_headers(&request, &mut args);
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
    args.extend(request.urls);
    Ok(args)
}

fn set_headers(request: &PlayRequest, args: &mut Vec<String>) {
    if let Some(referrer) = &request.referrer {
        args.push(format!("{ARG_REFERRER}{referrer}"));
    }
    if let Some(user_agent) = &request.user_agent {
        args.push(format!("{ARG_USER_AGENT}{user_agent}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{media_type, settings, test_db};

    fn header_args(request: PlayRequest) -> Vec<String> {
        let mut args = Vec::new();
        set_headers(&request, &mut args);
        args
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
    fn set_headers_adds_nothing_without_headers() {
        assert!(header_args(PlayRequest::default()).is_empty());
    }

    #[test]
    fn set_headers_passes_the_referrer_and_user_agent() {
        let args = header_args(PlayRequest {
            referrer: Some("https://referrer.example".to_string()),
            user_agent: Some("Channel UA".to_string()),
            ..Default::default()
        });
        assert_eq!(
            args,
            [
                "--http-referrer=https://referrer.example",
                "--http-user-agent=Channel UA",
            ]
        );
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
