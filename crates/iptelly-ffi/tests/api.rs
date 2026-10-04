// The API as an app sees it, against a database in a temporary folder.
// init can only run once per process, so this is one test.

use iptelly_core::types::Filters;
use iptelly_core::{source_type, sql, view_type};

#[tokio::test]
async fn an_app_can_add_and_browse_a_source() {
    let dir = std::env::temp_dir().join(format!("iptelly-ffi-test-{}", std::process::id()));
    iptelly_ffi::init(
        dir.join("data").to_string_lossy().into_owned(),
        dir.join("cache").to_string_lossy().into_owned(),
    )
    .await
    .unwrap();
    assert!(dir.join("data").join(sql::DB_NAME).exists());

    let source = sql::get_custom_source("My channels".to_string());
    assert_eq!(source.source_type, source_type::CUSTOM);
    iptelly_ffi::add_source(source).await.unwrap();
    assert!(
        iptelly_ffi::source_name_exists("My channels".to_string())
            .await
            .unwrap()
    );
    let sources = iptelly_ffi::get_sources().await.unwrap();
    assert_eq!(sources.len(), 1);

    let channels = iptelly_ffi::search(Filters {
        query: None,
        source_ids: vec![sources[0].id.unwrap()],
        media_types: None,
        view_type: view_type::ALL,
        page: 1,
        series_id: None,
        group_id: None,
        use_keywords: false,
        sort: 0,
        season: None,
    })
    .await
    .unwrap();
    assert!(channels.is_empty());

    let mut settings = iptelly_ffi::get_settings().await.unwrap();
    settings.volume = Some(40);
    iptelly_ffi::update_settings(settings).await.unwrap();
    assert_eq!(iptelly_ffi::get_settings().await.unwrap().volume, Some(40));

    let error = iptelly_ffi::refresh_source(9999).await.unwrap_err();
    assert!(!error.to_string().is_empty());

    let _ = std::fs::remove_dir_all(dir);
}
