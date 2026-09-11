//! Fixtures and fakes shared by the store's tests.

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

/// `catalog-v1.json` as the deployed store served it on 2026-09-03.
pub(super) const FIXTURE_CATALOG: &[u8] = include_bytes!("fixtures/catalog-v1.json");
/// Its detached signature, made by the deployed store's key.
pub(super) const FIXTURE_SIGNATURE: &str = include_str!("fixtures/catalog-v1.json.sig");

/// One entry of a test archive.
pub(super) enum ZipEntry {
    File {
        name: String,
        bytes: Vec<u8>,
        /// Unix mode as `git archive` records it (`0o100755`); `None`
        /// leaves the external attributes empty like GitHub does for plain files.
        mode: Option<u32>,
        deflate: bool,
    },
    Dir(String),
    Symlink {
        name: String,
        target: String,
    },
}

/// Builds a zip in memory.
pub(super) fn zip_bytes(entries: &[ZipEntry]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for entry in entries {
        match entry {
            ZipEntry::File {
                name,
                bytes,
                mode,
                deflate,
            } => {
                let mut options = SimpleFileOptions::default().compression_method(if *deflate {
                    CompressionMethod::Deflated
                } else {
                    CompressionMethod::Stored
                });
                if let Some(mode) = mode {
                    options = options.unix_permissions(*mode);
                }
                writer.start_file(name, options).expect("start file");
                writer.write_all(bytes).expect("write file");
            }
            ZipEntry::Dir(name) => {
                writer
                    .add_directory(name, SimpleFileOptions::default())
                    .expect("add directory");
            }
            ZipEntry::Symlink { name, target } => {
                writer
                    .add_symlink(name, target, SimpleFileOptions::default())
                    .expect("add symlink");
            }
        }
    }
    writer.finish().expect("finish").into_inner()
}

/// Writes a test archive under `dir` and returns its path.
pub(super) fn write_zip(dir: &Path, entries: &[ZipEntry]) -> PathBuf {
    let path = dir.join(format!("archive-{}.zip", crate::util::now_ms()));
    std::fs::write(&path, zip_bytes(entries)).expect("write zip");
    path
}

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use url::Url;

use super::fetch::{FetchBody, FetchError, FetchFuture, FetchOutcome, ProgressFn, StoreFetcher};

/// What the fake answers for one URL.
#[derive(Clone)]
pub(super) enum FakeResponse {
    Body {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
    NotModified,
    Status(u16),
    Offline,
}

/// An in-memory [`StoreFetcher`]: canned responses per URL, every request
/// recorded with the `If-None-Match` it carried.
#[derive(Default)]
pub(super) struct FakeFetcher {
    responses: Mutex<HashMap<String, FakeResponse>>,
    requests: Mutex<Vec<(String, Option<String>)>>,
}

impl FakeFetcher {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(super) fn respond(&self, url: &str, response: FakeResponse) {
        crate::util::lock_unpoisoned(&self.responses).insert(url.to_string(), response);
    }

    pub(super) fn body(&self, url: &str, bytes: &[u8], etag: Option<&str>) {
        self.respond(
            url,
            FakeResponse::Body {
                bytes: bytes.to_vec(),
                etag: etag.map(str::to_string),
            },
        );
    }

    pub(super) fn requests(&self) -> Vec<(String, Option<String>)> {
        crate::util::lock_unpoisoned(&self.requests).clone()
    }

    fn answer(&self, url: &Url, if_none_match: Option<&str>) -> Result<FetchOutcome, FetchError> {
        crate::util::lock_unpoisoned(&self.requests)
            .push((url.to_string(), if_none_match.map(str::to_string)));
        let response = crate::util::lock_unpoisoned(&self.responses)
            .get(url.as_str())
            .cloned()
            .unwrap_or(FakeResponse::Status(404));
        match response {
            FakeResponse::Body { bytes, etag } => Ok(FetchOutcome::Body(FetchBody { bytes, etag })),
            FakeResponse::NotModified => Ok(FetchOutcome::NotModified),
            FakeResponse::Status(status) => Err(FetchError::Status {
                url: url.to_string(),
                status,
            }),
            FakeResponse::Offline => Err(FetchError::Offline {
                host: url.host_str().unwrap_or_default().to_string(),
                reason: "fake".to_string(),
            }),
        }
    }
}

impl StoreFetcher for FakeFetcher {
    fn get<'a>(
        &'a self,
        url: &'a Url,
        if_none_match: Option<&'a str>,
        max_bytes: usize,
    ) -> FetchFuture<'a, FetchOutcome> {
        Box::pin(async move {
            let outcome = self.answer(url, if_none_match)?;
            if let FetchOutcome::Body(body) = &outcome
                && body.bytes.len() > max_bytes
            {
                return Err(FetchError::TooLarge {
                    what: "fake body",
                    limit: max_bytes as u64,
                });
            }
            Ok(outcome)
        })
    }

    fn download<'a>(
        &'a self,
        url: &'a Url,
        target: &'a std::path::Path,
        max_bytes: u64,
        progress: ProgressFn<'a>,
    ) -> FetchFuture<'a, u64> {
        Box::pin(async move {
            let FetchOutcome::Body(body) = self.answer(url, None)? else {
                return Err(FetchError::Other("304 to a download".to_string()));
            };
            let len = body.bytes.len() as u64;
            if len > max_bytes {
                return Err(FetchError::TooLarge {
                    what: "fake download",
                    limit: max_bytes,
                });
            }
            progress(len / 2, Some(len));
            std::fs::write(target, &body.bytes).map_err(|source| FetchError::Io {
                path: target.to_path_buf(),
                source,
            })?;
            progress(len, Some(len));
            Ok(len)
        })
    }
}

/// A store service over temp paths, a fresh config watcher and supervisor,
/// wired to `fetcher` and trusting `key`. Keep the tempdir alive.
pub(super) async fn service_with_key(
    fetcher: Arc<FakeFetcher>,
    key: minisign_verify::PublicKey,
) -> (
    tempfile::TempDir,
    crate::config::SmabarPaths,
    super::StoreService,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = crate::config::SmabarPaths::new(dir.path().to_path_buf());
    let config = Arc::new(crate::config::ConfigWatcher::spawn(paths.clone()).expect("config"));
    let supervisor = crate::plugins::PluginSupervisor::start(
        paths.clone(),
        crate::providers::ProviderHub::new(),
        Arc::clone(&config),
        crate::plugins::SupervisorOptions::default(),
    )
    .await;
    let service = super::StoreService::new(
        paths.clone(),
        config,
        supervisor,
        fetcher,
        super::StoreOptions {
            endpoint: Url::parse(ENDPOINT).expect("url"),
            key,
            app_version: "0.1.1".to_string(),
            reserved_ids: std::collections::BTreeSet::from(["clock".to_string()]),
        },
    )
    .expect("service");
    (dir, paths, service)
}

/// [`service_with_key`] with the deployed store's key (for the live fixture).
pub(super) async fn service(
    fetcher: Arc<FakeFetcher>,
) -> (
    tempfile::TempDir,
    crate::config::SmabarPaths,
    super::StoreService,
) {
    service_with_key(
        fetcher,
        super::catalog::embedded_key().expect("embedded key"),
    )
    .await
}

pub(super) const ENDPOINT: &str = "https://store.test/";
pub(super) const CATALOG_URL: &str = "https://store.test/catalog-v1.json";
pub(super) const SIGNATURE_URL: &str = "https://store.test/catalog-v1.json.sig";

/// Points the fake at the signed fixture.
pub(super) fn serve_fixture(fetcher: &FakeFetcher, etag: Option<&str>) {
    fetcher.body(CATALOG_URL, FIXTURE_CATALOG, etag);
    fetcher.body(SIGNATURE_URL, FIXTURE_SIGNATURE.as_bytes(), None);
}

use base64::Engine as _;
use blake2::{Blake2b512, Digest as _};
use ed25519_dalek::{Signer as _, SigningKey};

/// Signs test catalogs in the minisign layout the store uses, with a key of
/// its own — the deployed store's secret key never leaves the store.
pub(super) struct TestSigner {
    key: SigningKey,
    key_id: [u8; 8],
}

impl TestSigner {
    pub(super) fn new() -> Self {
        Self {
            key: SigningKey::from_bytes(&[7u8; 32]),
            key_id: *b"testkey!",
        }
    }

    pub(super) fn public_key(&self) -> minisign_verify::PublicKey {
        let mut raw = Vec::with_capacity(42);
        raw.extend_from_slice(b"Ed");
        raw.extend_from_slice(&self.key_id);
        raw.extend_from_slice(&self.key.verifying_key().to_bytes());
        minisign_verify::PublicKey::from_base64(
            &base64::engine::general_purpose::STANDARD.encode(raw),
        )
        .expect("test key")
    }

    /// The four-line `.sig` for `bytes` (pre-hashed mode, trusted comment
    /// covered by the global signature).
    pub(super) fn sign(&self, bytes: &[u8], file: &str) -> String {
        let digest = Blake2b512::digest(bytes);
        let signature = self.key.sign(&digest).to_bytes();
        let mut payload = Vec::with_capacity(74);
        payload.extend_from_slice(b"ED");
        payload.extend_from_slice(&self.key_id);
        payload.extend_from_slice(&signature);
        let trusted = format!("timestamp:1\tfile:{file}\thashed");
        let mut global_input = signature.to_vec();
        global_input.extend_from_slice(trusted.as_bytes());
        let global = self.key.sign(&global_input).to_bytes();
        let b64 = base64::engine::general_purpose::STANDARD;
        format!(
            "untrusted comment: test signature\n{}\ntrusted comment: {trusted}\n{}\n",
            b64.encode(payload),
            b64.encode(global)
        )
    }
}

pub(super) const TEST_COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
pub(super) const TEST_REPO: &str = "octo/demo";

/// A plugin that answers the handshake and records which version ran.
pub(super) const RUNNING_SCRIPT: &str = r#"
import json
import pathlib
import sys

for line in sys.stdin:
    message = json.loads(line)
    if message.get("method") == "initialize":
        data = pathlib.Path(message["params"]["dataDir"])
        data.mkdir(parents=True, exist_ok=True)
        (data / "started-by").write_text("VERSION")
        print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": {}}), flush=True)
    elif message.get("method") == "shutdown":
        print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": {}}), flush=True)
        sys.exit(0)
"#;

/// A plugin that dies at once.
pub(super) const CRASHING_SCRIPT: &str = "import sys\nsys.exit(3)\n";

pub(super) fn test_manifest(id: &str, version: &str) -> String {
    format!(
        r#"{{"id": "{id}", "name": "Demo", "version": "{version}", "protocolVersion": 1,
  "runtime": "exec", "command": ["python3", "main.py"],
  "tiles": [{{"id": "main", "name": "Main"}}]}}
"#
    )
}

/// The catalog entry, archive and signature for one root-layout exec plugin,
/// all served by `fetcher`; `edit` may tweak the listing before it is signed.
pub(super) fn serve_plugin(
    fetcher: &FakeFetcher,
    signer: &TestSigner,
    id: &str,
    version: &str,
    script: &str,
    generated_at: &str,
    edit: impl FnOnce(&mut serde_json::Value),
) {
    let manifest = test_manifest(id, version);
    let main = script.replace("VERSION", version);
    let mut tree = super::treeoid::TreeBuilder::new();
    tree.add_blob("smabar.json", manifest.as_bytes(), false)
        .expect("blob");
    tree.add_blob("main.py", main.as_bytes(), false)
        .expect("blob");
    let tree_oid = tree.finish();
    let root = super::archive::archive_root("demo", TEST_COMMIT);
    let archive = zip_bytes(&[
        ZipEntry::Dir(root.clone()),
        ZipEntry::File {
            name: format!("{root}smabar.json"),
            bytes: manifest.clone().into_bytes(),
            mode: None,
            deflate: true,
        },
        ZipEntry::File {
            name: format!("{root}main.py"),
            bytes: main.into_bytes(),
            mode: None,
            deflate: false,
        },
    ]);
    let archive_url = super::fetch::expected_archive_url(TEST_REPO, TEST_COMMIT);
    let mut listing = serde_json::json!({
        "schema": 1,
        "generatedAt": generated_at,
        "items": [{
            "kind": "plugin", "id": id, "name": "Demo", "version": version,
            "description": "test plugin", "keywords": ["test"],
            "requires": {"smabar": null, "os": ["linux", "windows", "macos"], "external": []},
            "author": {"login": "octo", "url": "https://github.com/octo"},
            "repo": {"id": 42, "url": "https://github.com/octo/demo", "nameWithOwner": TEST_REPO,
                     "license": "MIT", "stars": 1, "openIssues": 0, "pushedAt": null, "archived": false},
            "path": ".", "updatedAt": generated_at,
            "detailSha256": "0", "runtime": "exec",
            "source": {"commit": TEST_COMMIT, "ref": "main", "archiveUrl": archive_url,
                        "treeOid": tree_oid, "manifestSha256": super::catalog::sha256_hex(manifest.as_bytes())}
        }],
        "blocklist": []
    });
    edit(&mut listing);
    let bytes = serde_json::to_vec(&listing).expect("listing json");
    fetcher.body(
        SIGNATURE_URL,
        signer.sign(&bytes, "catalog-v1.json").as_bytes(),
        None,
    );
    fetcher.body(CATALOG_URL, &bytes, None);
    fetcher.body(&archive_url, &archive, None);
}
