"""The native preview announces an update but cannot serve an installer."""

import importlib.util
import json
import threading
import unittest
from http.client import HTTPConnection
from http.server import ThreadingHTTPServer
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "update_preview", Path(__file__).with_name("update-preview.py")
)
assert spec is not None and spec.loader is not None
preview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preview)


class UpdatePreviewTests(unittest.TestCase):
    def test_real_http_manifest_and_rejected_downloads(self):
        with ThreadingHTTPServer(("127.0.0.1", 0), preview.PreviewHandler) as server:
            thread = threading.Thread(target=server.serve_forever)
            thread.start()
            try:
                connection = HTTPConnection("127.0.0.1", server.server_port, timeout=5)
                connection.request("GET", "/updates/latest.json")
                response = connection.getresponse()
                self.assertEqual(response.status, 200)
                self.assertEqual(response.getheader("Cache-Control"), "no-store")
                manifest = json.loads(response.read())
                self.assertTrue(manifest["version"].endswith("-dev-preview"))
                self.assertIn("windows-x86_64", manifest["platforms"])
                self.assertIn("linux-deb-x86_64", manifest["platforms"])
                self.assertIn("linux-rpm-x86_64", manifest["platforms"])
                self.assertTrue(
                    all(
                        entry["url"] == f"http://127.0.0.1:8787{preview.PACKAGE_PATH}"
                        for entry in manifest["platforms"].values()
                    )
                )
                with patch.object(preview.time, "sleep") as pause:
                    for path in (preview.PACKAGE_PATH, "/updates/real-setup.exe", "/../Cargo.toml"):
                        connection.request("GET", path)
                        response = connection.getresponse()
                        self.assertEqual(response.status, 404)
                        response.read()
                    pause.assert_called_once_with(3)
                connection.close()
            finally:
                server.shutdown()
                thread.join(timeout=5)
