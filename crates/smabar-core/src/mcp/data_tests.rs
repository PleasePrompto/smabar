//! `plugin_data`: listing, reading, and the guards around both.

use std::{fs, path::PathBuf};

use rmcp::handler::server::wrapper::Parameters;

use super::plugin_types::PluginDataParams;
use super::tests::{test_handler, unwrap_json};

fn params(id: &str, path: Option<&str>) -> Parameters<PluginDataParams> {
    Parameters(PluginDataParams {
        id: id.to_string(),
        path: path.map(str::to_string),
    })
}

#[tokio::test]
async fn a_plugin_that_never_wrote_anything_reports_an_empty_directory() {
    let (_temp, handler) = test_handler().await;
    let result =
        unwrap_json(handler.plugin_data(params("clock", None)).await).expect("list data dir");
    assert!(!result.exists);
    assert!(result.files.is_empty());
    assert_eq!(
        PathBuf::from(&result.dir),
        handler.paths.plugin_data_dir("clock")
    );
}

#[tokio::test]
async fn the_listing_reports_nested_files_with_sizes_but_no_content() {
    let (_temp, handler) = test_handler().await;
    let dir = handler.paths.plugin_data_dir("clock");
    fs::create_dir_all(dir.join("icons")).expect("create data dir");
    fs::write(dir.join("state.json"), "{\"zones\":2}").expect("write state");
    fs::write(dir.join("icons/sun.png"), b"\x89PNG").expect("write icon");

    let result =
        unwrap_json(handler.plugin_data(params("clock", None)).await).expect("list data dir");

    assert!(result.exists);
    let paths: Vec<PathBuf> = result
        .files
        .iter()
        .map(|file| PathBuf::from(&file.path))
        .collect();
    assert_eq!(
        paths,
        vec![
            PathBuf::from("icons").join("sun.png"),
            PathBuf::from("state.json"),
        ]
    );
    assert_eq!(result.total_bytes, 15); // {"zones":2} plus the 4-byte icon
    assert!(result.files.iter().all(|file| file.modified.is_some()));
    assert!(result.file.is_none());
}

#[tokio::test]
async fn a_named_file_comes_back_with_its_content() {
    let (_temp, handler) = test_handler().await;
    let dir = handler.paths.plugin_data_dir("clock");
    fs::create_dir_all(&dir).expect("create data dir");
    fs::write(dir.join("cache.json"), "{\"t\":1}").expect("write cache");

    let result = unwrap_json(
        handler
            .plugin_data(params("clock", Some("cache.json")))
            .await,
    )
    .expect("read cache file");

    let file = result.file.expect("file result");
    assert_eq!(file.content.as_deref(), Some("{\"t\":1}"));
    assert!(!file.binary && !file.truncated);
    assert!(result.files.is_empty());
}

#[tokio::test]
async fn a_database_is_reported_as_binary_instead_of_garbled() {
    // The case that matters: an agent asking for a SQLite file must get a
    // clear "binary" instead of a screenful of replacement characters.
    let (_temp, handler) = test_handler().await;
    let dir = handler.paths.plugin_data_dir("scraper");
    fs::create_dir_all(&dir).expect("create data dir");
    fs::write(dir.join("state.sqlite3"), b"SQLite format 3\x00\xff\xfe").expect("write db");

    let result = unwrap_json(
        handler
            .plugin_data(params("scraper", Some("state.sqlite3")))
            .await,
    )
    .expect("read database file");

    let file = result.file.expect("file result");
    assert!(file.binary);
    assert!(file.content.is_none());
    assert_eq!(file.size, 18);
}

#[tokio::test]
async fn traversal_and_bad_ids_are_refused() {
    let (_temp, handler) = test_handler().await;
    let dir = handler.paths.plugin_data_dir("clock");
    fs::create_dir_all(&dir).expect("create data dir");
    fs::write(handler.paths.base_dir().join("config.json"), "{}").ok();

    for path in ["../config.json", "/etc/passwd", "a/../../config.json"] {
        assert!(
            unwrap_json(handler.plugin_data(params("clock", Some(path))).await).is_err(),
            "{path} must be refused"
        );
    }
    for id in ["../evil", "Clock", ""] {
        assert!(
            unwrap_json(handler.plugin_data(params(id, None)).await).is_err(),
            "{id} must be refused"
        );
    }
}

#[tokio::test]
async fn asking_for_a_directory_says_so_instead_of_failing_obscurely() {
    let (_temp, handler) = test_handler().await;
    let dir = handler.paths.plugin_data_dir("clock");
    fs::create_dir_all(dir.join("icons")).expect("create data dir");

    let error = unwrap_json(handler.plugin_data(params("clock", Some("icons"))).await)
        .expect_err("a directory is not a file");

    assert!(error.message.contains("call without `path` to list it"));
}
