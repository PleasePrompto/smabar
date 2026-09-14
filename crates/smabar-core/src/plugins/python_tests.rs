//! No network or shared environment: a stub uv prepares a private test venv.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use tempfile::TempDir;
use tokio::io::AsyncReadExt;

use super::SupervisorOptions;
use super::logfile::PluginLog;
use super::manifest::PluginManifest;
use super::process::{PluginProcess, spawn_plugin};

const STUB: &str = r#"#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys, time
root = pathlib.Path(os.environ['SMABAR_PLUGIN_DATA_DIR'])
with (root / 'calls').open('a') as calls:
    calls.write(json.dumps(sys.argv[1:]) + '\n')
if sys.argv[1] == 'sync':
    assert sys.argv[2] == '--inexact'
    if (root / 'fail').exists():
        print('error: dependency cannot be resolved', file=sys.stderr)
        sys.exit(2)
    if (root / 'hang').exists():
        child = subprocess.Popen(['sleep', '60'])
        (root / 'descendant').write_text(str(child.pid))
        time.sleep(60)
    if (root / 'no-env').exists():
        sys.exit(0)
    subprocess.check_call([sys.executable, '-m', 'venv', '--without-pip', str(root / 'env')])
elif sys.argv[1:3] == ['python', 'find']:
    env = root / 'env' / 'bin' / 'python3'
    print(env if env.exists() else sys.executable)
elif sys.argv[1] == 'run':
    sys.exit(subprocess.call([sys.executable, sys.argv[3]]))
else:
    sys.exit(3)
"#;

const SCRIPT: &str = r#"# /// script
# dependencies = []
# ///
import json, os, sys
print(json.dumps({'pid': os.getpid(), 'id': os.environ['SMABAR_PLUGIN_ID'],
    'data': os.environ['SMABAR_PLUGIN_DATA_DIR'], 'cwd': os.getcwd(),
    'prefix': sys.prefix, 'venv': os.environ.get('VIRTUAL_ENV'),
    'path': os.environ['PATH'].split(os.pathsep)[0], 'sdk': os.environ['PYTHONPATH']}))
"#;

struct Fixture {
    dir: TempDir,
    manifest: PluginManifest,
    options: SupervisorOptions,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("smabar python ")
            .tempdir()
            .expect("temp dir");
        let uv = dir.path().join("uv");
        std::fs::write(&uv, STUB).expect("stub uv");
        std::fs::set_permissions(&uv, std::fs::Permissions::from_mode(0o755)).expect("executable");
        std::fs::write(dir.path().join("plugin.py"), SCRIPT).expect("script");
        std::fs::create_dir(dir.path().join("data")).expect("data dir");
        let manifest = serde_json::from_value(serde_json::json!({
            "id":"test", "name":"Test", "version":"1", "protocolVersion":1,
            "runtime":"python", "entry":"plugin.py", "tiles":[{"id":"tile","name":"Tile"}]
        }))
        .expect("manifest");
        let options = SupervisorOptions {
            uv_override: Some(uv),
            sdk_path: Some(dir.path().join("sdk path")),
            ..SupervisorOptions::default()
        };
        Self {
            dir,
            manifest,
            options,
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    async fn start(
        &self,
        log: &mut PluginLog,
    ) -> Result<PluginProcess, super::process::SpawnError> {
        spawn_plugin(
            &self.manifest,
            self.path(),
            &self.path().join("data"),
            &self.options,
            log,
        )
        .await
    }
}

/// The spawned pid and the state the script printed before exiting cleanly.
async fn finish(mut process: PluginProcess) -> (u32, Value) {
    let pid = process.child.id().expect("pid");
    let mut output = String::new();
    process
        .child
        .stdout
        .take()
        .expect("stdout")
        .read_to_string(&mut output)
        .await
        .expect("read");
    assert!(process.child.wait().await.expect("wait").success());
    (pid, serde_json::from_str(&output).expect("state"))
}

#[tokio::test]
async fn python_is_the_direct_child_with_its_environment_and_sdk() {
    let fixture = Fixture::new();
    let mut log = PluginLog::open(fixture.path(), "test");
    let process = fixture.start(&mut log).await.expect("start");
    let (pid, state) = finish(process).await;
    assert_eq!(
        state["pid"], pid,
        "no resident uv between supervisor and Python"
    );
    assert_eq!(state["id"], "test");
    assert_eq!(state["cwd"], fixture.path().to_string_lossy().as_ref());
    let env = fixture.path().join("data/env");
    assert_eq!(state["prefix"], env.to_string_lossy().as_ref());
    assert_eq!(state["venv"], state["prefix"]);
    assert_eq!(state["path"], env.join("bin").to_string_lossy().as_ref());
    assert!(
        state["sdk"]
            .as_str()
            .expect("SDK path")
            .starts_with(fixture.path().join("sdk path").to_str().expect("path"))
    );
    let calls = std::fs::read_to_string(fixture.path().join("data/calls")).expect("calls");
    assert_eq!(calls.lines().count(), 2);
}

#[tokio::test]
async fn dependency_failure_is_logged_without_starting_the_plugin() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path().join("data/fail"), "").expect("failure marker");
    let mut log = PluginLog::open(fixture.path(), "test");
    let error = fixture
        .start(&mut log)
        .await
        .err()
        .expect("failed preparation");
    assert!(
        error.to_string().contains("dependency cannot be resolved"),
        "{error}"
    );
    let calls = std::fs::read_to_string(fixture.path().join("data/calls")).expect("calls");
    assert_eq!(calls.lines().count(), 1);
    assert!(!fixture.path().join("data/env").exists());
}

#[tokio::test]
async fn cancelling_preparation_kills_its_process_group() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path().join("data/hang"), "").expect("hang marker");
    let descendant = fixture.path().join("data/descendant");
    let mut log = PluginLog::open(fixture.path(), "test");
    let deadline = tokio::time::timeout(Duration::from_secs(5), async {
        while !descendant.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    tokio::select! {
        _ = fixture.start(&mut log) => panic!("preparation should still be running"),
        result = deadline => result.expect("uv spawned a child"),
    }
    let pid: i32 = std::fs::read_to_string(descendant)
        .expect("pid")
        .parse()
        .expect("numeric pid");
    // A killed orphan can briefly remain a zombie before init reaps it.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            // SAFETY: signal 0 checks existence of the fixture's own child.
            if unsafe { libc::kill(pid, 0) } != 0 {
                break;
            }
            let status = std::process::Command::new("ps")
                .args(["-o", "stat=", "-p", &pid.to_string()])
                .output()
                .expect("read descendant status");
            if String::from_utf8_lossy(&status.stdout)
                .trim_start()
                .starts_with('Z')
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("uv descendant stopped after cancellation");
}

#[tokio::test]
async fn a_missing_script_environment_falls_back_to_uv_run() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path().join("data/no-env"), "").expect("no-env marker");
    let mut log = PluginLog::open(fixture.path(), "test");
    let process = fixture.start(&mut log).await.expect("start");
    let (pid, state) = finish(process).await;
    assert_ne!(
        state["pid"], pid,
        "the plugin runs under the resident stub uv"
    );
    assert_eq!(state["id"], "test");
    let calls = std::fs::read_to_string(fixture.path().join("data/calls")).expect("calls");
    assert_eq!(calls.lines().count(), 3, "{calls}");
    assert_eq!(
        calls.lines().last(),
        Some("[\"run\", \"--script\", \"plugin.py\"]")
    );
    let plugin_log =
        std::fs::read_to_string(fixture.path().join("plugin-test.log")).expect("plugin log");
    assert!(
        plugin_log.contains("no persistent script environment"),
        "{plugin_log}"
    );
}

#[tokio::test]
async fn scripts_without_metadata_keep_the_original_uv_run_path() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path().join("plugin.py"), "pass\n").expect("plain script");
    let mut log = PluginLog::open(fixture.path(), "test");
    let mut process = fixture.start(&mut log).await.expect("start");
    assert!(process.child.wait().await.expect("wait").success());
    let calls = std::fs::read_to_string(fixture.path().join("data/calls")).expect("calls");
    assert_eq!(calls.trim(), "[\"run\", \"--script\", \"plugin.py\"]");
}
