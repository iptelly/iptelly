// Channel logos and posters. The first time a URL is needed it's downloaded,
// shrunk off the UI thread, and the shrunk copy saved as a WebP in the cache
// directory (named by a hash of the URL). Later loads decode that small file
// instead. Only the finished pixel buffer crosses to the UI thread, where it
// becomes a slint::Image.

use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use image::codecs::webp::WebPEncoder;
use image::{ImageFormat, RgbaImage};
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
    let cache = directories::ProjectDirs::from("dev", "iptelly", "iptelly")?
        .cache_dir()
        .to_path_buf();
    // Older versions cached the full-size downloads here, often over 1 GB.
    let old = cache.join("images");
    if old.exists() {
        std::thread::spawn(move || std::fs::remove_dir_all(old));
    }
    let dir = cache.join("thumbnails");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
});

pub async fn load(url: String) -> Result<SharedPixelBuffer<Rgba8Pixel>> {
    load_sized(url, MAX_SIZE).await
}

/// Like load, but shrunk to fit `max_size` instead of a tile, for the
/// movie details' poster and backdrop. Each size is cached separately.
pub async fn load_sized(url: String, max_size: u32) -> Result<SharedPixelBuffer<Rgba8Pixel>> {
    let path = cache_path(&url, max_size);
    if let Some(path) = &path
        && let Ok(bytes) = tokio::fs::read(path).await
        // A cached file that won't decode (say, half written) is replaced.
        && let Ok(buffer) = crate::blocking(move || decode(&bytes)).await
    {
        return Ok(buffer);
    }
    let bytes = download(&url).await?;
    let _permit = DECODES.acquire().await?;
    let (buffer, webp) = tokio::task::spawn_blocking(move || shrink(&bytes, max_size)).await??;
    if let Some(path) = path {
        // A failed write only costs a re-download next time.
        let _ = tokio::fs::write(&path, webp).await;
    }
    Ok(buffer)
}

/// Deletes every cached thumbnail; they're downloaded again as needed.
pub fn clear_cache() -> Result<()> {
    let Some(dir) = CACHE_DIR.as_ref() else {
        return Ok(());
    };
    for entry in std::fs::read_dir(dir)? {
        std::fs::remove_file(entry?.path())?;
    }
    Ok(())
}

fn cache_path(url: &str, max_size: u32) -> Option<PathBuf> {
    let hash = Sha256::digest(url.as_bytes());
    let mut name: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    // Tile-sized copies keep their original names.
    if max_size != MAX_SIZE {
        name += &format!("-{max_size}");
    }
    Some(CACHE_DIR.as_ref()?.join(name + ".webp"))
}

async fn download(url: &str) -> Result<Vec<u8>> {
    let _permit = DOWNLOADS.acquire().await?;
    let client = iptelly_core::utils::new_http_client("IPTelly")?;
    let response = client.get(url).send().await?.error_for_status()?;
    Ok(response.bytes().await?.to_vec())
}

/// Shrinks a downloaded image to fit `max_size`, returning its pixels and a
/// lossless WebP of them for the cache.
fn shrink(bytes: &[u8], max_size: u32) -> Result<(SharedPixelBuffer<Rgba8Pixel>, Vec<u8>)> {
    let image = image::load_from_memory(bytes).context("unsupported image")?;
    let image = if image.width() > max_size || image.height() > max_size {
        image.thumbnail(max_size, max_size)
    } else {
        image
    };
    let rgba = image.into_rgba8();
    let mut webp = Vec::new();
    rgba.write_with_encoder(WebPEncoder::new_lossless(&mut webp))?;
    Ok((pixels(&rgba), webp))
}

/// Decodes a cached WebP, which is already shrunk.
fn decode(bytes: &[u8]) -> Result<SharedPixelBuffer<Rgba8Pixel>> {
    let image = image::load_from_memory_with_format(bytes, ImageFormat::WebP)?;
    Ok(pixels(&image.into_rgba8()))
}

fn pixels(rgba: &RgbaImage) -> SharedPixelBuffer<Rgba8Pixel> {
    SharedPixelBuffer::clone_from_slice(rgba.as_raw(), rgba.width(), rgba.height())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        });
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn large_images_are_shrunk_to_tile_size() {
        let (buffer, _) = shrink(&png(1000, 1500), MAX_SIZE).unwrap();
        assert_eq!(buffer.height(), MAX_SIZE);
        assert!(buffer.width() < MAX_SIZE);
    }

    #[test]
    fn small_images_keep_their_size() {
        let (buffer, _) = shrink(&png(64, 48), MAX_SIZE).unwrap();
        assert_eq!((buffer.width(), buffer.height()), (64, 48));
    }

    #[test]
    fn the_cached_webp_decodes_to_the_same_pixels() {
        let (buffer, webp) = shrink(&png(1000, 1500), MAX_SIZE).unwrap();
        assert_eq!(image::guess_format(&webp).unwrap(), ImageFormat::WebP);
        let cached = decode(&webp).unwrap();
        assert_eq!(
            (cached.width(), cached.height()),
            (buffer.width(), buffer.height())
        );
        assert_eq!(cached.as_bytes(), buffer.as_bytes());
    }

    #[test]
    fn a_broken_image_is_an_error() {
        assert!(shrink(b"not an image", MAX_SIZE).is_err());
        assert!(decode(b"not an image").is_err());
    }
}
