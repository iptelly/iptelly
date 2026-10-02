// The adult PIN (settings.rs) and the general settings round trip, against
// a throwaway database. Settings are global to the database, so every test
// holds setup()'s lock and starts from an empty settings table.

use std::env::consts::OS;
use std::sync::{Mutex, MutexGuard, Once, PoisonError};

use iptelly_lib::{settings, sql, types::Settings};

static INIT: Once = Once::new();
static LOCK: Mutex<()> = Mutex::new(());

fn setup() -> MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    INIT.call_once(|| {
        let db_path = std::env::temp_dir().join(format!(
            "iptelly_settings_test_{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&db_path);
        // Safety: this runs once, before the first sql:: call, while every
        // other test is blocked on LOCK - nothing else reads the
        // environment concurrently.
        unsafe {
            std::env::set_var("OPEN_TV_DB_PATH", &db_path);
        }
        sql::create_or_initialize_db().expect("failed to initialize test database");
    });
    sql::get_conn()
        .unwrap()
        .execute("DELETE FROM settings", [])
        .unwrap();
    guard
}

#[test]
fn no_pin_is_set_on_a_fresh_database() {
    let _db = setup();
    assert!(!settings::has_adult_pin().unwrap());
    assert_eq!(settings::get_adult_pin_hash().unwrap(), None);
    assert!(!settings::verify_adult_pin("").unwrap());
    assert!(!settings::verify_adult_pin("1234").unwrap());
}

#[test]
fn verifies_only_the_pin_that_was_set() {
    let _db = setup();
    settings::set_adult_pin(Some("1234".to_string())).unwrap();
    assert!(settings::has_adult_pin().unwrap());
    assert!(settings::verify_adult_pin("1234").unwrap());
    for wrong in ["", "0000", "12345", "123", " 1234"] {
        assert!(
            !settings::verify_adult_pin(wrong).unwrap(),
            "{wrong:?} was accepted"
        );
    }
}

#[test]
fn stores_a_hash_rather_than_the_pin() {
    let _db = setup();
    settings::set_adult_pin(Some("1234".to_string())).unwrap();
    let hash = settings::get_adult_pin_hash().unwrap().unwrap();
    assert_eq!(hash.len(), 64, "expected a hex SHA-256: {hash}");
    assert!(
        hash.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert!(!hash.contains("1234"));
}

#[test]
fn hashes_the_same_pin_the_same_way_every_time() {
    let _db = setup();
    settings::set_adult_pin(Some("1234".to_string())).unwrap();
    let first = settings::get_adult_pin_hash().unwrap();
    settings::set_adult_pin(Some("1234".to_string())).unwrap();
    assert_eq!(settings::get_adult_pin_hash().unwrap(), first);

    settings::set_adult_pin(Some("4321".to_string())).unwrap();
    assert_ne!(settings::get_adult_pin_hash().unwrap(), first);
}

#[test]
fn changing_the_pin_stops_the_old_one_working() {
    let _db = setup();
    settings::set_adult_pin(Some("1234".to_string())).unwrap();
    settings::set_adult_pin(Some("9876".to_string())).unwrap();
    assert!(settings::verify_adult_pin("9876").unwrap());
    assert!(!settings::verify_adult_pin("1234").unwrap());
}

#[test]
fn clearing_the_pin_removes_it() {
    let _db = setup();
    settings::set_adult_pin(Some("1234".to_string())).unwrap();
    settings::set_adult_pin(None).unwrap();
    assert!(!settings::has_adult_pin().unwrap());
    assert_eq!(settings::get_adult_pin_hash().unwrap(), None);
    assert!(!settings::verify_adult_pin("1234").unwrap());
}

// How Settings > Import data restores the PIN - from its hash alone.
#[test]
fn a_restored_hash_verifies_the_original_pin() {
    let _db = setup();
    settings::set_adult_pin(Some("1234".to_string())).unwrap();
    let hash = settings::get_adult_pin_hash().unwrap();
    settings::set_adult_pin(None).unwrap();

    settings::set_adult_pin_hash(hash).unwrap();
    assert!(settings::has_adult_pin().unwrap());
    assert!(settings::verify_adult_pin("1234").unwrap());
    assert!(!settings::verify_adult_pin("0000").unwrap());

    settings::set_adult_pin_hash(None).unwrap();
    assert!(!settings::has_adult_pin().unwrap());
}

fn all_settings() -> Settings {
    Settings {
        recording_path: Some("/tmp/recordings".to_string()),
        mpv_params: Some("--mute=yes".to_string()),
        use_stream_caching: Some(false),
        default_view: Some(2),
        volume: Some(75),
        refresh_on_start: Some(true),
        restream_port: Some(3333),
        enable_tray_icon: Some(true),
        zoom: Some(110),
        default_sort: Some(1),
        enable_hwdec: Some(false),
        always_ask_save: Some(true),
        enable_gpu: Some(true),
        theme: Some("classic".to_string()),
        player: Some("vlc".to_string()),
        vlc_params: Some("--no-audio".to_string()),
        network_interface: Some("10.0.0.2".to_string()),
        lightweight_mode: Some(true),
    }
}

#[test]
fn saved_settings_read_back_unchanged() {
    let _db = setup();
    settings::update_settings(all_settings()).unwrap();
    let mut expected = all_settings();
    // The tray icon is never enabled on Linux, whatever was saved.
    if OS == "linux" {
        expected.enable_tray_icon = Some(false);
    }
    assert_eq!(settings::get_settings().unwrap(), expected);
}

#[test]
fn clearing_a_text_setting_removes_it() {
    let _db = setup();
    settings::update_settings(all_settings()).unwrap();
    settings::update_settings(Settings {
        mpv_params: None,
        vlc_params: None,
        theme: None,
        player: None,
        network_interface: None,
        ..all_settings()
    })
    .unwrap();
    let saved = settings::get_settings().unwrap();
    assert_eq!(saved.mpv_params, None);
    assert_eq!(saved.vlc_params, None);
    assert_eq!(saved.theme, None);
    assert_eq!(saved.player, None);
    assert_eq!(saved.network_interface, None);
    assert_eq!(saved.volume, Some(75));
}

#[test]
fn saving_settings_keeps_the_pin() {
    let _db = setup();
    settings::set_adult_pin(Some("1234".to_string())).unwrap();
    settings::update_settings(all_settings()).unwrap();
    assert!(settings::verify_adult_pin("1234").unwrap());
}
