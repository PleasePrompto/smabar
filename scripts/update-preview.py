#!/usr/bin/env python3
"""Preview the real update notification in a native smabar dev build.

Run this in one terminal, scripts/dev.sh or scripts\\dev.bat in another.
Then open System > Updates > Check now. No package can be installed here.
"""

import argparse
import json
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from release import PLATFORMS

PORT = 8787
PACKAGE_PATH = "/updates/preview-package"
# Deliberately unmistakable and newer than the app: this is never a release.
MANIFEST = {
    "version": "999.0.0-dev-preview",
    "notes": "DEV PREVIEW / DEV-VORSCHAU: No installation. Keine Installation.",
    "platforms": {
        target: {
            "url": f"http://127.0.0.1:{PORT}{PACKAGE_PATH}",
            "signature": "dev-preview-no-package",
        }
        for target in PLATFORMS.values()
    },
}


class PreviewHandler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        if self.path == "/updates/latest.json":
            body = json.dumps(MANIFEST).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)
            return
        if self.path == PACKAGE_PATH:
            # Leave time to inspect the real progress UI before the test error.
            time.sleep(3)
        # Never serve files, including packages left by an earlier release test.
        self.send_error(404, "DEV PREVIEW: no installation package is provided")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    try:
        server = ThreadingHTTPServer(("127.0.0.1", PORT), PreviewHandler)
    except OSError as error:
        parser.exit(1, f"Cannot start update preview: {error}. Stop the server on port {PORT}.\n")
    with server:
        print(f"DEV PREVIEW: http://127.0.0.1:{PORT}/updates/latest.json", flush=True)
        print("Open smabar > System > Updates > Check now. Ctrl+C stops the preview.", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            print("Update preview stopped.")


if __name__ == "__main__":
    main()
