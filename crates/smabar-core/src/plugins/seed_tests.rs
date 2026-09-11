//! Seeding, managed updates, and the orphan sweep.
//!
//! One test per row of the decision matrix in `seed.rs`, because "is this a
//! user edit or a new bundled version?" is the whole point of that module.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::SmabarPaths;

use super::seed::{plugins_with_pending_update, seed_bundled_plugins, sweep_orphaned_data};

struct Fixture {
    _temp: tempfile::TempDir,
    paths: SmabarPaths,
    bundled: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("create temp dir");
        let paths = SmabarPaths::new(temp.path().join("home/.smabar"));
        let bundled = temp.path().join("resources/plugins");
        Self {
            _temp: temp,
            paths,
            bundled,
        }
    }

    /// Writes the bundled resource copy of a plugin.
    fn bundle(&self, id: &str, body: &str) {
        write_plugin(&self.bundled.join(id), body);
    }

    /// Writes the installed copy under `~/.smabar/plugins/`.
    fn install(&self, id: &str, body: &str) {
        write_plugin(&self.paths.plugins_dir().join(id), body);
    }

    fn seed(&self) {
        seed_bundled_plugins(&self.paths, &self.bundled).expect("seed plugins");
    }

    fn sweep(&self) -> Vec<String> {
        sweep_orphaned_data(&self.paths)
    }

    fn installed_body(&self, id: &str) -> String {
        fs::read_to_string(self.paths.plugins_dir().join(id).join("plugin.py"))
            .expect("read installed plugin")
    }
}

fn write_plugin(dir: &Path, body: &str) {
    fs::create_dir_all(dir.join("locales")).expect("create plugin dir");
    fs::write(dir.join("plugin.py"), body).expect("write plugin");
    fs::write(dir.join("locales/en.json"), "{}").expect("write locale");
}

#[test]
fn removing_a_seeded_plugin_survives_restart_and_bundle_updates() {
    for preexisting_copy in [false, true] {
        let fixture = Fixture::new();
        fixture.bundle("optional", "v1");
        if preexisting_copy {
            fixture.install("optional", "user version without an old seed receipt");
        }
        fixture.seed();
        let installed = fixture.paths.plugins_dir().join("optional");
        fs::remove_dir_all(&installed).expect("uninstall");
        fixture.seed();
        assert!(!installed.exists());
        fixture.bundle("optional", "v2");
        fixture.seed();
        assert!(!installed.exists());
        assert!(plugins_with_pending_update(&fixture.paths).is_empty());
        // An explicit reinstall works and resumes normal managed updates.
        fixture.install("optional", "v2");
        fixture.seed();
        fixture.bundle("optional", "v3");
        fixture.seed();
        assert_eq!(fixture.installed_body("optional"), "v3");
    }
}

#[test]
fn missing_folder_is_installed_with_all_nested_files() {
    let fixture = Fixture::new();
    fixture.bundle("weather", "v1");

    fixture.seed();

    assert_eq!(fixture.installed_body("weather"), "v1");
    assert!(
        fixture
            .paths
            .plugins_dir()
            .join("weather/locales/en.json")
            .is_file()
    );
}

#[test]
fn an_untouched_install_is_updated_when_the_bundle_moves_on() {
    let fixture = Fixture::new();
    fixture.bundle("clock", "v1");
    fixture.seed(); // establishes the baseline

    fixture.bundle("clock", "v2");
    fixture.seed();

    assert_eq!(fixture.installed_body("clock"), "v2");
    assert!(plugins_with_pending_update(&fixture.paths).is_empty());
}

#[test]
fn a_locally_edited_plugin_is_kept_and_reported_instead_of_overwritten() {
    let fixture = Fixture::new();
    fixture.bundle("clock", "v1");
    fixture.seed();
    fixture.install("clock", "my own version");

    fixture.bundle("clock", "v2");
    fixture.seed();

    assert_eq!(fixture.installed_body("clock"), "my own version");
    assert!(plugins_with_pending_update(&fixture.paths).contains("clock"));
}

#[test]
fn a_local_edit_alone_is_never_reported_as_an_update() {
    let fixture = Fixture::new();
    fixture.bundle("clock", "v1");
    fixture.seed();
    fixture.install("clock", "my own version");

    fixture.seed();

    assert_eq!(fixture.installed_body("clock"), "my own version");
    assert!(plugins_with_pending_update(&fixture.paths).is_empty());
}

#[test]
fn an_install_without_a_baseline_is_treated_as_modified() {
    // Folders that predate the seed state carry no proof of origin, so a
    // differing bundle must never overwrite them silently.
    let fixture = Fixture::new();
    fixture.bundle("crypto", "bundled");
    fixture.install("crypto", "user edited");

    fixture.seed();

    assert_eq!(fixture.installed_body("crypto"), "user edited");
    assert!(plugins_with_pending_update(&fixture.paths).contains("crypto"));
}

#[test]
fn an_identical_install_without_a_baseline_adopts_one() {
    // This is the path a user takes right after upgrading: the folder matches
    // the bundle, so it can be adopted and updated automatically from then on.
    let fixture = Fixture::new();
    fixture.bundle("clock", "v1");
    fixture.install("clock", "v1");

    fixture.seed();
    fixture.bundle("clock", "v2");
    fixture.seed();

    assert_eq!(fixture.installed_body("clock"), "v2");
}

#[test]
fn unbundled_plugins_are_never_touched() {
    let fixture = Fixture::new();
    fixture.bundle("clock", "v1");
    fixture.install("paket-tracker", "mine");

    fixture.seed();

    assert_eq!(fixture.installed_body("paket-tracker"), "mine");
    assert!(plugins_with_pending_update(&fixture.paths).is_empty());
}

#[test]
fn build_artifacts_are_neither_hashed_nor_copied() {
    // Running or linting a plugin in the source tree leaves __pycache__
    // behind. Copying it would ship junk, and its byte-compiled files change
    // on their own — every start would then claim a new bundled version.
    let fixture = Fixture::new();
    fixture.bundle("clock", "v1");
    fixture.seed();
    assert!(
        !fixture
            .paths
            .plugins_dir()
            .join("clock/__pycache__")
            .exists()
    );

    let cache = fixture.bundled.join("clock/__pycache__");
    fs::create_dir_all(&cache).expect("create pycache");
    fs::write(cache.join("plugin.cpython-312.pyc"), "compiled").expect("write pyc");
    fs::write(fixture.bundled.join("clock/.DS_Store"), "junk").expect("write junk");

    fixture.seed();

    assert!(
        !fixture
            .paths
            .plugins_dir()
            .join("clock/__pycache__")
            .exists()
    );
    assert!(!fixture.paths.plugins_dir().join("clock/.DS_Store").exists());
    // The hash did not move, so no phantom update was recorded.
    assert!(plugins_with_pending_update(&fixture.paths).is_empty());
}

#[test]
fn missing_resource_directory_is_tolerated() {
    let fixture = Fixture::new();
    seed_bundled_plugins(&fixture.paths, &fixture.bundled).expect("missing resources are optional");
    assert!(!fixture.paths.plugins_dir().exists());
}

#[test]
fn the_sweep_removes_data_of_gone_plugins_and_keeps_the_rest() {
    let fixture = Fixture::new();
    fixture.install("clock", "v1");
    let alive = fixture.paths.plugin_data_dir("clock");
    let gone = fixture.paths.plugin_data_dir("deleted-plugin");
    fs::create_dir_all(&alive).expect("create live data dir");
    fs::create_dir_all(gone.join("nested")).expect("create orphan data dir");
    fs::write(gone.join("nested/state.sqlite3"), "x").expect("write orphan file");

    let removed = sweep_orphaned_data(&fixture.paths);

    assert_eq!(removed, vec!["deleted-plugin".to_string()]);
    assert!(alive.is_dir());
    assert!(!gone.exists());
}

#[test]
fn the_sweep_removes_logs_of_gone_plugins_including_their_rotation() {
    let fixture = Fixture::new();
    fixture.install("clock", "v1");
    let logs = fixture.paths.logs_dir();
    fs::create_dir_all(&logs).expect("create logs dir");
    for name in [
        "plugin-clock.log",
        "plugin-ghost.log",
        "plugin-ghost.log.1",
        "smabar.log.2026-08-23",
        "notes.txt",
    ] {
        fs::write(logs.join(name), "x").expect("write log");
    }

    let removed = fixture.sweep();

    assert_eq!(removed, vec!["ghost".to_string()]);
    assert!(
        logs.join("plugin-clock.log").is_file(),
        "a live plugin keeps its log"
    );
    assert!(!logs.join("plugin-ghost.log").exists());
    assert!(!logs.join("plugin-ghost.log.1").exists());
    assert!(
        logs.join("smabar.log.2026-08-23").is_file(),
        "the core log is untouched"
    );
    assert!(
        logs.join("notes.txt").is_file(),
        "unrelated files are untouched"
    );
}

#[test]
fn a_plugin_id_is_reported_once_even_with_data_and_two_log_files() {
    let fixture = Fixture::new();
    let logs = fixture.paths.logs_dir();
    fs::create_dir_all(&logs).expect("create logs dir");
    fs::create_dir_all(fixture.paths.plugin_data_dir("ghost")).expect("create data dir");
    fs::write(logs.join("plugin-ghost.log"), "x").expect("write log");
    fs::write(logs.join("plugin-ghost.log.1"), "x").expect("write rotation");

    assert_eq!(fixture.sweep(), vec!["ghost".to_string()]);
}

#[test]
fn the_sweep_leaves_the_seed_state_alone() {
    // The state file lives under cache/, not data/ — regression guard for the
    // sweep eating its own bookkeeping.
    let fixture = Fixture::new();
    fixture.bundle("clock", "v1");
    fixture.seed();

    sweep_orphaned_data(&fixture.paths);

    assert!(fixture.paths.seed_state_file().is_file());
}

#[test]
fn sweeping_an_untouched_home_does_nothing() {
    let fixture = Fixture::new();
    assert!(sweep_orphaned_data(&fixture.paths).is_empty());
}
