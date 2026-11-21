use anyhow::{Context, Result};
use chrono::Local;
use std::{fs, sync::LazyLock};

static USE_LOGGER: LazyLock<bool> = LazyLock::new(|| init_logger());

pub fn log(message: String) {
    if *USE_LOGGER {
        log::error!("{message}");
    } else {
        eprintln!("{message}");
    }
}

/// Logs a URL the app is about to request or play.
pub fn log_url(what: &str, url: &str) {
    log(format!("{what}: {}", redact_url(url)));
}

/// `url` with its password hidden, since people attach the logs to bug
/// reports: the `password` query value, and the password in Xtream stream
/// paths ("/live/user/PASSWORD/1.ts").
pub fn redact_url(url: &str) -> String {
    let (base, query) = match url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (url, None),
    };
    let mut parts: Vec<&str> = base.split('/').collect();
    // "http://host/..." splits into "http:", "", "host" before the path.
    let path_start = if base.contains("://") { 3 } else { 0 };
    let kind = parts
        .iter()
        .skip(path_start)
        .position(|part| matches!(*part, "live" | "movie" | "series" | "timeshift"));
    if let Some(i) = kind.map(|i| i + path_start) {
        // kind/user/password/file at least.
        if parts.len() > i + 3 {
            parts[i + 2] = "***";
        }
    }
    let mut redacted = parts.join("/");
    if let Some(query) = query {
        let pairs: Vec<String> = query
            .split('&')
            .map(|pair| match pair.split_once('=') {
                Some((key, _)) if key.eq_ignore_ascii_case("password") => format!("{key}=***"),
                _ => pair.to_string(),
            })
            .collect();
        redacted.push('?');
        redacted.push_str(&pairs.join("&"));
    }
    redacted
}

fn init_logger() -> bool {
    let path = match get_and_create_log_path() {
        Ok(val) => val,
        Err(e) => {
            eprint!("Failed to determine log path, {:?}", e);
            return false;
        }
    };
    let file = match fs::File::create(path) {
        Ok(val) => val,
        Err(e) => {
            eprint!("Failed to create file for logger, {:?}", e);
            return false;
        }
    };
    match simplelog::WriteLogger::init(
        simplelog::LevelFilter::Error,
        simplelog::Config::default(),
        file,
    ) {
        Ok(_) => true,
        Err(e) => {
            eprint!("Failed to init logger, {:?}", e);
            return false;
        }
    }
}

fn get_and_create_log_path() -> Result<String> {
    let mut path = crate::paths::cache_dir()?;
    path.push("logs");
    if !path.exists() {
        std::fs::create_dir_all(&path).context("Failed to create log directory")?;
    }
    path.push(get_log_name());
    Ok(path.to_string_lossy().to_string())
}

fn get_log_name() -> String {
    let current_time = Local::now();
    let formatted_time = current_time.format("%Y-%m-%d-%H-%M-%S").to_string();
    format!("{formatted_time}.log")
}

#[cfg(test)]
mod tests {
    use super::redact_url;

    #[test]
    fn hides_the_password_in_the_query() {
        assert_eq!(
            redact_url("http://host:8080/player_api.php?username=me&password=secret&action=x"),
            "http://host:8080/player_api.php?username=me&password=***&action=x"
        );
    }

    #[test]
    fn hides_the_password_in_stream_paths() {
        assert_eq!(
            redact_url("http://host/live/me/secret/1.ts"),
            "http://host/live/me/***/1.ts"
        );
        assert_eq!(
            redact_url("http://host/timeshift/me/secret/60/2026-01-01:10-00/1.m3u8"),
            "http://host/timeshift/me/***/60/2026-01-01:10-00/1.m3u8"
        );
        assert_eq!(redact_url("/movie/me/secret/2.mkv"), "/movie/me/***/2.mkv");
    }

    #[test]
    fn leaves_other_urls_alone() {
        for url in [
            "https://example.com/playlist.m3u8",
            "http://live/stream.ts",
            "http://cdn.example/live/1.ts",
        ] {
            assert_eq!(redact_url(url), url);
        }
    }
}
