// Where the core keeps its database (the data folder) and its logs,
// downloaded playlists and guides, and re-stream files (the cache folder).
// On desktop these are the OS's usual folders. Android and iOS have none a
// library can find, so apps there pass the folders the OS gave them to
// set_dirs before calling anything else.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, anyhow};

struct Dirs {
    data: PathBuf,
    cache: PathBuf,
}

static DIRS: OnceLock<Dirs> = OnceLock::new();

/// Uses these folders instead of the OS's usual ones. Only works before the
/// first call that needs one.
pub fn set_dirs(data: PathBuf, cache: PathBuf) -> Result<()> {
    DIRS.set(Dirs { data, cache })
        .map_err(|_| anyhow!("the app's folders are already in use"))
}

/// The data folder, created if it's missing.
pub fn data_dir() -> Result<PathBuf> {
    create(&dirs()?.data)
}

/// The cache folder, created if it's missing.
pub fn cache_dir() -> Result<PathBuf> {
    create(&dirs()?.cache)
}

fn dirs() -> Result<&'static Dirs> {
    if let Some(dirs) = DIRS.get() {
        return Ok(dirs);
    }
    let project = directories::ProjectDirs::from("dev", "iptelly", "iptelly")
        .context("Could not determine the app's folders")?;
    Ok(DIRS.get_or_init(|| Dirs {
        data: project.data_dir().to_owned(),
        cache: project.cache_dir().to_owned(),
    }))
}

fn create(path: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(path)
        .with_context(|| format!("Failed to create {}", path.display()))?;
    Ok(path.to_owned())
}
