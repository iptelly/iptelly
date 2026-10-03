// Everything IPTelly does apart from drawing the UI. Frontends call these
// modules directly, hold one AppState for the life of the app, and pass an
// EventSink for anything the backend needs to tell them unprompted.

pub mod api;
pub mod app_data;
pub mod bulk_action_type;
pub mod downloads;
pub mod epg;
pub mod events;
pub mod external_player;
pub mod log;
pub mod m3u;
pub mod media_type;
pub mod mpv;
pub mod restream;
pub mod settings;
pub mod share;
pub mod sort_type;
pub mod source_type;
pub mod sql;
pub mod types;
pub mod utils;
pub mod view_type;
pub mod vlc;
pub mod xmltv;
pub mod xtream;

#[cfg(test)]
mod test_db;
