#!/usr/bin/env python3
"""Loopback preview of local assets against existing public, read-only API responses.

No cloud credentials, collection writes, deployment or external dependencies.
"""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit, parse_qs
from urllib.request import urlopen
from urllib.error import HTTPError, URLError
import argparse
import json
import mimetypes
import threading
import time

PUBLIC = Path(__file__).resolve().parents[1] / "apps/dashboard/public"
UPSTREAM = "https://crypto-prediction-lab-308289525742.us-central1.run.app"
READ_PATHS = {"/api/assets", "/api/outcomes", "/api/context", "/api/context/history", "/api/feeds",
              "/api/snapshot", "/api/history/reference", "/api/history/market", "/api/combos/quote"}
CACHE = {}
LOCK = threading.Lock()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def reply(self, code, body, content_type="application/json"):
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_GET(self):
        parsed = urlsplit(self.path)
        if parsed.path == "/api/archive/window":
            self.reply(200, b'{"status":"not_configured","candles":[]}')
            return
        if parsed.path in READ_PATHS:
            if parsed.path == "/api/outcomes" and parse_qs(parsed.query).get("asset") not in (["BTC"], ["ETH"], ["SOL"]):
                self.reply(400, b'{"error":"Unsupported asset"}')
                return
            with LOCK:
                saved = CACHE.get(self.path)
            if saved and time.monotonic() - saved[0] < 60:
                self.reply(200, saved[1])
                return
            try:
                # Production has not been redeployed: preview its existing short history.
                upstream_path = "/api/context" if parsed.path == "/api/context/history" else self.path
                with urlopen(UPSTREAM + upstream_path, timeout=10) as response:
                    body = response.read(1_048_577)
                    if len(body) > 1_048_576:
                        raise ValueError()
                    json.loads(body)
                with LOCK:
                    if len(CACHE) >= 64:
                        CACHE.pop(next(iter(CACHE)))
                    CACHE[self.path] = (time.monotonic(), body)
                self.reply(200, body)
            except (HTTPError, URLError, OSError, ValueError):
                self.reply(502, b'{"error":"Public source unavailable"}')
            return
        name = "index.html" if parsed.path == "/" else parsed.path.removeprefix("/assets/")
        if name == "lightweight-charts-5.2.1.js":
            name = "vendor/" + name
        path = (PUBLIC / name).resolve()
        if not path.is_relative_to(PUBLIC) or not path.is_file():
            self.reply(404, b'{}')
            return
        body = path.read_bytes()
        if name == "feed-client.js":
            body = b'window.DASHBOARD_POLLING=true;\n' + body
        self.reply(200, body, mimetypes.guess_type(path.name)[0] or "application/octet-stream")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=3002)
    args = parser.parse_args()
    print(f"Local preview: http://127.0.0.1:{args.port}/?view=data", flush=True)
    print("Uses deployed public short-history responses; the new 24h query and private archived minute quotes need the configured Rust backend.", flush=True)
    ThreadingHTTPServer(("127.0.0.1", args.port), Handler).serve_forever()
