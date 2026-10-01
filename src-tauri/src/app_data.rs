// Whole-app export/import (Settings > Export data / Import data) - a single
// file bundling everything a fresh install can't otherwise recover: every
// configured source (credentials included), favourites, watch history,
// hidden channels/categories, general settings, the adult PIN, and download
// history (metadata only, never the downloaded files themselves). See
// AppDataExport's own doc comment in types.rs for the exact shape.
//
// Distinct from share.rs's ExportedSource/import, which shares one *custom*
// source's groups/channels with someone else, not a personal backup of
// everything.
use crate::{
    downloads, settings, sql,
    types::{AppDataExport, ExportedSourceData},
};
use anyhow::{Context, Result};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Serialize, de::DeserializeOwned};
use std::io::{Read, Write};

const APP_DATA_EXPORT_VERSION: u32 = 1;

// A real backup (every source, every favourite/hidden channel, download
// history) is plain JSON text that compresses very well - gzipped instead
// of using utils::serialize_to_file, which share.rs's single-channel/group/
// source exports also use and which deliberately stay uncompressed, human-
// readable JSON (those are an order of magnitude smaller and sometimes
// hand-inspected/edited).
fn write_gzip_json<T: Serialize>(obj: &T, path: &str) -> Result<()> {
    let data = serde_json::to_vec(obj)?;
    let file = std::fs::File::create(path)?;
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder.write_all(&data)?;
    encoder.finish()?;
    Ok(())
}

fn read_gzip_json<T: DeserializeOwned>(path: &str) -> Result<T> {
    let file = std::fs::File::open(path)?;
    let mut data = String::new();
    GzDecoder::new(file).read_to_string(&mut data)?;
    Ok(serde_json::from_str(&data)?)
}

pub fn export_app_data(path: String) -> Result<()> {
    let settings = settings::get_settings()?;
    let adult_pin_hash = settings::get_adult_pin_hash()?;
    let downloads = downloads::get_history()?;
    let sources = sql::do_tx(|tx| {
        sql::get_sources()?
            .into_iter()
            .map(|source| {
                let preserve = sql::get_preserve(tx, source.id.context("source has no id")?)?;
                Ok(ExportedSourceData { source, preserve })
            })
            .collect::<Result<Vec<_>>>()
    })?;
    let export = AppDataExport {
        version: APP_DATA_EXPORT_VERSION,
        settings,
        adult_pin_hash,
        sources,
        downloads,
    };
    write_gzip_json(&export, &path)
}

pub fn import_app_data(path: String) -> Result<()> {
    let import: AppDataExport = read_gzip_json(&path)?;
    settings::update_settings(import.settings)?;
    settings::set_adult_pin_hash(import.adult_pin_hash)?;
    sql::do_tx(|tx| {
        for entry in import.sources {
            let source_id = sql::import_source_full(tx, &entry.source)?;
            // Applies immediately wherever this source's channels already
            // exist locally (e.g. restoring onto a machine that already has
            // it configured and refreshed) - a no-op otherwise, since a
            // brand new/not-yet-refreshed source has no channel rows yet to
            // match these entries' names against.
            sql::restore_preserve(tx, source_id, entry.preserve.clone())?;
            // Also stages the same data for the *next* refresh to pick up
            // (see pending_preserve's migration comment) - covers the
            // brand new/not-yet-refreshed case above, which the immediate
            // apply just silently skipped.
            sql::set_pending_preserve(tx, source_id, &entry.preserve)?;
        }
        Ok(())
    })?;
    for download in import.downloads {
        sql::upsert_download_row(&download)?;
    }
    Ok(())
}
