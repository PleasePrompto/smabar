import base64
import json
import urllib.request
from pathlib import Path

class Client:
    def __init__(self, port=7627):
        self.url = f"http://127.0.0.1:{port}/mcp"
        self.headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}
        self.id = 0
        self.request("initialize", {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "memory-check", "version": "1"}})

    def request(self, method, params):
        self.id += 1
        body = {"jsonrpc": "2.0", "id": self.id, "method": method, "params": params}
        with urllib.request.urlopen(urllib.request.Request(self.url, json.dumps(body).encode(), self.headers), timeout=30) as response:
            session = response.headers.get("Mcp-Session-Id")
            if session: self.headers["Mcp-Session-Id"] = session
            text = response.read().decode()
        parsed = next((json.loads(line[6:]) for line in text.splitlines() if line.startswith("data: {")), None)
        if parsed is None: parsed = json.loads(text)
        if "error" in parsed: raise RuntimeError(parsed["error"])
        return parsed["result"]

    def call(self, name, **args):
        result = self.request("tools/call", {"name": name, "arguments": args})
        if result.get("isError"): raise RuntimeError(result["content"])
        return result

    def screenshot(self, target, path):
        result = self.call("bar_screenshot", target=target)
        block = next(item for item in result["content"] if item["type"] == "image")
        Path(path).write_bytes(base64.b64decode(block["data"]))
