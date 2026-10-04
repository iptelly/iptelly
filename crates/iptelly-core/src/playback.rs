// What any player needs to play a channel: its URLs, the HTTP headers the
// provider expects, and the playback settings. mpv and VLC turn this into
// command-line options; an app with its own player (on a TV, say) gets it
// from request() and hands it to that player.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::settings::get_settings;
use crate::types::{Channel, ChannelHttpHeaders, Settings, Source};
use crate::{media_type, sql};

#[derive(Clone, PartialEq, Debug, Default, Deserialize, Serialize)]
pub struct PlayRequest {
    pub title: String,
    /// The channel's URL, followed by the rest of the season when it's an
    /// episode, to play one after another.
    pub urls: Vec<String>,
    pub user_agent: Option<String>,
    pub referrer: Option<String>,
    pub origin: Option<String>,
    /// Accept invalid TLS certificates.
    pub ignore_ssl: bool,
    /// A live stream: reconnect when it ends rather than stopping.
    pub live: bool,
    /// Remember where playback stopped. Not for live streams.
    pub save_position: bool,
    /// Start from the remembered position. Episodes always start at the
    /// beginning.
    pub resume: bool,
    pub hardware_decoding: bool,
    pub stream_caching: bool,
    /// 0-100.
    pub volume: Option<u8>,
}

/// How to play `channel`, for an app that plays it itself.
pub fn request(channel: &Channel) -> Result<PlayRequest> {
    build(channel, &get_source(channel), &get_settings()?)
}

pub(crate) fn get_source(channel: &Channel) -> Option<Source> {
    channel.source_id.and_then(|id| {
        sql::get_source_from_id(id)
            .with_context(|| format!("failed to fetch source with id {}", id))
            .ok()
    })
}

pub(crate) fn build(
    channel: &Channel,
    source: &Option<Source>,
    settings: &Settings,
) -> Result<PlayRequest> {
    let headers = sql::get_channel_headers_by_id(channel.id.context("no channel id?")?)?;
    let mut urls = vec![channel.url.clone().context("no url")?];
    if channel.episode_num.is_some() {
        urls.extend(sql::find_all_episodes_after(channel)?);
    }
    // Catch-up/timeshift streams commonly don't start exactly on a clean
    // keyframe boundary (the server seeks into an already-recorded
    // segment), which hardware decoding tends to choke on - confirmed via
    // testing: the same URL played fine with --hwdec=no but produced
    // "non-existing PPS referenced" / decoder failures with --hwdec=auto.
    // Regular live/VOD playback is unaffected and keeps using hwdec.
    let is_timeshift = urls[0].contains("/timeshift/");
    let live = channel.media_type == media_type::LIVESTREAM;
    let mut request = PlayRequest {
        title: channel.name.clone(),
        urls,
        live,
        save_position: !live,
        resume: channel.episode_num.is_none(),
        hardware_decoding: settings.enable_hwdec.unwrap_or(true) && !is_timeshift,
        stream_caching: settings.use_stream_caching != Some(false),
        volume: settings.volume,
        ..Default::default()
    };
    set_headers(&mut request, headers, source);
    Ok(request)
}

fn set_headers(
    request: &mut PlayRequest,
    headers: Option<ChannelHttpHeaders>,
    source: &Option<Source>,
) {
    let headers = headers.unwrap_or_default();
    request.user_agent = headers
        .user_agent
        .or_else(|| source.as_ref().and_then(|s| s.stream_user_agent.clone()));
    request.referrer = headers.referrer;
    request.origin = headers.http_origin;
    request.ignore_ssl = headers.ignore_ssl == Some(true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{settings, test_db};

    fn source_with_user_agent(user_agent: &str) -> Option<Source> {
        let mut source = sql::get_custom_source("unused".to_string());
        source.stream_user_agent = Some(user_agent.to_string());
        Some(source)
    }

    fn with_headers(headers: Option<ChannelHttpHeaders>, source: &Option<Source>) -> PlayRequest {
        let mut request = PlayRequest::default();
        set_headers(&mut request, headers, source);
        request
    }

    #[test]
    fn no_headers_without_channel_headers_or_a_source() {
        assert_eq!(with_headers(None, &None), PlayRequest::default());
    }

    #[test]
    fn prefers_the_channel_user_agent_over_the_source() {
        let request = with_headers(
            Some(ChannelHttpHeaders {
                user_agent: Some("Channel UA".to_string()),
                ..Default::default()
            }),
            &source_with_user_agent("Source UA"),
        );
        assert_eq!(request.user_agent.as_deref(), Some("Channel UA"));
    }

    #[test]
    fn falls_back_to_the_source_user_agent() {
        let request = with_headers(None, &source_with_user_agent("Source UA"));
        assert_eq!(request.user_agent.as_deref(), Some("Source UA"));
    }

    #[test]
    fn passes_the_channel_headers() {
        let request = with_headers(
            Some(ChannelHttpHeaders {
                referrer: Some("https://referrer.example".to_string()),
                http_origin: Some("https://origin.example".to_string()),
                ignore_ssl: Some(true),
                ..Default::default()
            }),
            &None,
        );
        assert_eq!(
            request.referrer.as_deref(),
            Some("https://referrer.example")
        );
        assert_eq!(request.origin.as_deref(), Some("https://origin.example"));
        assert!(request.ignore_ssl);
    }

    #[test]
    fn describes_a_livestream_with_default_settings() {
        let _db = test_db::lock();
        let source = test_db::add_source("playback livestream");
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
            request(&channel).unwrap(),
            PlayRequest {
                title: "News".to_string(),
                urls: vec!["http://example.com/live/1.ts".to_string()],
                live: true,
                resume: true,
                hardware_decoding: true,
                stream_caching: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn movies_save_their_position() {
        let _db = test_db::lock();
        let source = test_db::add_source("playback movie");
        let channel = test_db::add_channel(
            &source,
            test_db::channel("Film", "http://example.com/movie/1.mkv", media_type::MOVIE),
            None,
        );
        let request = request(&channel).unwrap();
        assert!(request.save_position && request.resume);
        assert!(!request.live);
    }

    #[test]
    fn applies_playback_settings() {
        let _db = test_db::lock();
        test_db::set_setting(settings::ENABLE_HWDEC, "false");
        test_db::set_setting(settings::USE_STREAM_CACHING, "false");
        test_db::set_setting(settings::VOLUME, "50");
        let source = test_db::add_source("playback settings");
        let channel = test_db::add_channel(
            &source,
            test_db::channel(
                "News",
                "http://example.com/live/1.ts",
                media_type::LIVESTREAM,
            ),
            None,
        );
        let request = request(&channel).unwrap();
        assert!(!request.hardware_decoding);
        assert!(!request.stream_caching);
        assert_eq!(request.volume, Some(50));
    }
}
