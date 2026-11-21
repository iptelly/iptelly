// A source can have up to three URLs (Source::urls): when the first doesn't
// answer, the next is tried. The one that answered is remembered for a while,
// so later requests go straight to it rather than waiting on a dead server
// each time.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::log;

const REMEMBER_FOR: Duration = Duration::from_secs(5 * 60);

static WORKING: LazyLock<Mutex<HashMap<i64, (String, Instant)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The URL that last answered for the source, if it did recently and is
/// still one of `urls`.
pub fn remembered(source_id: Option<i64>, urls: &[String]) -> Option<String> {
    let working = WORKING.lock().ok()?;
    let (url, when) = working.get(&source_id?)?;
    (when.elapsed() < REMEMBER_FOR && urls.contains(url)).then(|| url.clone())
}

pub fn remember(source_id: Option<i64>, url: &str) {
    let (Some(id), Ok(mut working)) = (source_id, WORKING.lock()) else {
        return;
    };
    working.insert(id, (url.to_string(), Instant::now()));
}

/// `urls` with the one that last answered moved to the front.
pub fn ordered(source_id: Option<i64>, urls: Vec<String>) -> Vec<String> {
    let Some(first) = remembered(source_id, &urls) else {
        return urls;
    };
    let mut ordered = vec![first.clone()];
    ordered.extend(urls.into_iter().filter(|url| *url != first));
    ordered
}

/// Runs `attempt` on each of `urls` in turn until one succeeds, and returns
/// that URL with its result. The URL is remembered for the source.
pub async fn first_working<T, F, Fut>(
    source_id: Option<i64>,
    urls: Vec<String>,
    mut attempt: F,
) -> Result<(String, T)>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let urls = ordered(source_id, urls);
    if urls.is_empty() {
        bail!("The source has no URL");
    }
    let mut failures = Vec::new();
    for url in &urls {
        match attempt(url.clone()).await {
            Ok(value) => {
                if !failures.is_empty() {
                    log::log(format!("Failed over to {url}"));
                }
                remember(source_id, url);
                return Ok((url.clone(), value));
            }
            Err(e) => {
                log::log(format!("{url} failed: {e:?}"));
                failures.push(e);
            }
        }
    }
    if failures.len() == 1 {
        return Err(failures.remove(0));
    }
    let details: Vec<String> = urls
        .iter()
        .zip(failures)
        .map(|(url, e)| format!("{url}: {e:#}"))
        .collect();
    bail!("None of the source's URLs worked:\n{}", details.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(list: &[&str]) -> Vec<String> {
        list.iter().map(|u| u.to_string()).collect()
    }

    #[tokio::test]
    async fn moves_on_to_the_next_url_and_remembers_it() {
        let (url, value) = first_working(Some(-101), urls(&["a", "b", "c"]), |url| async move {
            if url == "a" {
                bail!("down")
            }
            Ok(url.len())
        })
        .await
        .unwrap();
        assert_eq!((url.as_str(), value), ("b", 1));
        assert_eq!(ordered(Some(-101), urls(&["a", "b", "c"])), ["b", "a", "c"]);
    }

    #[tokio::test]
    async fn reports_every_failure() {
        let error = first_working(Some(-102), urls(&["a", "b"]), |url| async move {
            Err::<(), _>(anyhow::anyhow!("{url} is down"))
        })
        .await
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("a: a is down") && error.contains("b: b is down"),
            "{error}"
        );
    }

    #[test]
    fn forgets_a_url_the_source_no_longer_has() {
        remember(Some(-103), "old");
        assert_eq!(remembered(Some(-103), &urls(&["new"])), None);
        assert_eq!(ordered(Some(-103), urls(&["new"])), ["new"]);
    }

    #[test]
    fn remembers_nothing_without_a_source_id() {
        remember(None, "a");
        assert_eq!(ordered(None, urls(&["b", "a"])), ["b", "a"]);
    }
}
