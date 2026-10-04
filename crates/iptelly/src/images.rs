// Channel logos and posters. Each URL is downloaded once into the cache
// directory (named by a hash of the URL), then decoded and shrunk off the UI
// thread. Only the finished pixel buffer crosses to the UI thread, where it
// becomes a slint::Image.

use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use slint::{Rgba8Pixel, SharedPixelBuffer};

// Tile images are 50x72 logical pixels, so this covers 2x scaling without
// keeping full-size posters (often 1000px+) in memory.
const MAX_SIZE: u32 = 160;

// A page of 36 tiles shouldn't open 36 connections to one provider at once.
static DOWNLOADS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(8);

// A poster decodes to its full size (often 10-25 MB) before it's shrunk, and
// a page of them decoding at once peaked at over 300 MB.
static DECODES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

static CACHE_DIR: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    let dir = directories::ProjectDirs::from("dev", "iptelly", "iptelly")?
        .cache_dir()
        .join("images");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
});

pub async fn load(url: String) -> Result<SharedPixelBuffer<Rgba8Pixel>> {
    let bytes = match cache_path(&url) {
        Some(path) => match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(_) => {
                let bytes = download(&url).await?;
                // A failed write only costs a re-download next time.
                let _ = tokio::fs::write(&path, &bytes).await;
                bytes
            }
        },
        None => download(&url).await?,
    };
    let _permit = DECODES.acquire().await?;
    tokio::task::spawn_blocking(move || decode(&bytes)).await?
}

fn cache_path(url: &str) -> Option<PathBuf> {
    let hash = Sha256::digest(url.as_bytes());
    let name: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    Some(CACHE_DIR.as_ref()?.join(name))
}

async fn download(url: &str) -> Result<Vec<u8>> {
    let _permit = DOWNLOADS.acquire().await?;
    let client = iptelly_core::utils::new_http_client("IPTelly")?;
    let response = client.get(url).send().await?.error_for_status()?;
    Ok(response.bytes().await?.to_vec())
}

fn decode(bytes: &[u8]) -> Result<SharedPixelBuffer<Rgba8Pixel>> {
    let image = image::load_from_memory(bytes).context("unsupported image")?;
    let image = if image.width() > MAX_SIZE || image.height() > MAX_SIZE {
        image.thumbnail(MAX_SIZE, MAX_SIZE)
    } else {
        image
    };
    let rgba = image.into_rgba8();
    Ok(SharedPixelBuffer::clone_from_slice(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
    ))
}
