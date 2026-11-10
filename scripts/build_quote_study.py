#!/usr/bin/env python3
"""Build a three-day quote/outcome study from this app's existing Firestore archive."""
import argparse
from datetime import date, datetime, time, timedelta, timezone
from decimal import Decimal, InvalidOperation
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import time as clock
from urllib.error import HTTPError
from urllib.request import Request, HTTPRedirectHandler, build_opener

ROOT = "projects/gitunderstand/databases/crypto-prediction-lab/documents"
API = "https://firestore.googleapis.com/v1/" + ROOT
COLLECTION = "quote_studies_v1"
ASSETS = ("BTC", "ETH", "SOL")
MINUTES = (5, 10, 14)
UTC = timezone.utc


def encode(value):
    return json.dumps(value, separators=(",", ":"), sort_keys=True, allow_nan=False)


def digest(value):
    return hashlib.sha256(encode(value).encode()).hexdigest()


def instant(value):
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("Archive time has no timezone")
    return parsed


def midnight(day):
    return datetime.combine(day, time.min, tzinfo=UTC)


def cutoff_asks(bars, opened):
    """Exact minute-end observations. Duplicates, invalid OHLC and endpoints are gaps."""
    quotes = {}
    for minute in MINUTES:
        matching = [b for b in bars if b.get("end_period_ts") == opened + 60 * minute]
        if len(matching) != 1 or not isinstance(matching[0].get("yes_ask"), dict):
            continue
        try:
            values = [Decimal(str(matching[0]["yes_ask"].get(k + "_dollars")))
                      for k in ("open", "high", "low", "close")]
            if not all(v.is_finite() and 0 <= v <= 1 for v in values):
                continue
            op, high, low, close = values
            units = close * 10000
            if low <= min(op, close) <= max(op, close) <= high and 0 < units < 10000 and units == int(units):
                quotes[str(minute)] = int(units)
        except InvalidOperation:
            continue
    return quotes


def partition(asset, day, docs):
    features, labels, slots = {}, {}, set()
    start = int(midnight(day).timestamp())
    for doc in docs:
        fields = doc["fields"]
        payload = json.loads(fields["payload"]["stringValue"])
        summary = json.loads(fields["summary"]["stringValue"])
        market, candles = payload["market"], payload["candlesticks"]
        ticker = market["ticker"]
        if (payload.get("schema_version") != 1 or not re.fullmatch(f"KX{asset}15M-[A-Za-z0-9-]+", ticker)
                or doc["name"] != f"{ROOT}/contracts_{asset.lower()}/{ticker}"
                or summary["asset"] != asset or summary["ticker"] != ticker or candles["ticker"] != ticker
                or summary["result"] != market["result"]):
            raise ValueError("Archive identity or result mismatch")
        opened, closed = [instant(market[field]).timestamp() for field in ("open_time", "close_time")]
        if (opened % 900 or closed - opened != 900 or not start <= opened < start + 86400
                or closed != instant(fields["close_time"]["timestampValue"]).timestamp()):
            raise ValueError("Unexpected contract window")
        if opened in slots:
            raise ValueError("Duplicate asset window")
        slots.add(opened)
        if market["result"] not in ("yes", "no"):
            continue
        opened = int(opened)
        features[ticker] = {"open_ts": opened, "yes_ask_bps": cutoff_asks(candles["candlesticks"], opened),
                            "archive_captured_at": summary["captured_at"], "source_revision": doc["updateTime"]}
        labels[ticker] = market["result"]
    return {"schema_version": 1, "day": day.isoformat(), "asset": asset, "features": features, "labels": labels}


def cohort(parts, minute):
    bins = [{"n": 0, "yes": 0, "sum_ask_bps": 0} for _ in range(5)]
    settled = 0
    for part in parts:
        settled += len(part["features"])
        for ticker, record in part["features"].items():
            quote = record["yes_ask_bps"].get(str(minute))
            if quote is None:
                continue
            bucket = bins[quote // 2000]
            bucket["n"] += 1
            bucket["yes"] += int(part["labels"][ticker] == "yes")
            bucket["sum_ask_bps"] += quote
    return {"minute": minute, "expected": 96 * len(parts), "settled": settled, "bins": bins}


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *args):
        return None


class Firestore:
    def __init__(self):
        token = subprocess.run(["gcloud", "auth", "print-access-token"], capture_output=True,
                               text=True, check=True).stdout.strip()
        if not token:
            raise ValueError("Google Cloud login unavailable")
        self.headers = {"Authorization": "Bearer " + token, "Content-Type": "application/json"}
        self.opener = build_opener(NoRedirect())

    def request(self, path, body=None):
        req = Request(API + path, headers=self.headers, data=None if body is None else encode(body).encode())
        for attempt in range(4):
            try:
                with self.opener.open(req, timeout=35) as response:
                    raw = response.read(16_000_001)
                    if len(raw) > 16_000_000:
                        raise ValueError("Archive response exceeded its limit")
                    return json.loads(raw)
            except HTTPError as err:
                if err.code != 429 or attempt == 3:
                    if err.code != 404:
                        print(f"Firestore {path.split('/')[-1]} returned HTTP {err.code}", file=sys.stderr)
                    raise
                delay = err.headers.get("Retry-After", "")
                clock.sleep(min(30, int(delay)) if delay.isdigit() else 2 ** (attempt + 1))

    def get(self, name):
        try:
            return self.request(f"/{COLLECTION}/{name}")
        except HTTPError as err:
            if err.code != 404:
                raise
            return None

    def archived_day(self, asset, day, read_time):
        def boundary(op, value):
            return {"fieldFilter": {"field": {"fieldPath": "close_time"}, "op": op,
                                    "value": {"timestampValue": midnight(value).isoformat()}}}
        query = {"from": [{"collectionId": "contracts_" + asset.lower()}],
                 "select": {"fields": [{"fieldPath": k} for k in ("summary", "payload", "close_time")]},
                 "where": {"compositeFilter": {"op": "AND", "filters": [
                     boundary("GREATER_THAN", day), boundary("LESS_THAN_OR_EQUAL", day + timedelta(days=1))]}},
                 "limit": 97}
        rows = self.request(":runQuery", {"structuredQuery": query, "readTime": read_time})
        docs = [row["document"] for row in rows if "document" in row]
        if len(docs) > 96:
            raise ValueError("Too many archived contracts for one asset-day")
        return docs


def content(doc):
    return json.loads(doc["fields"]["payload"]["stringValue"])


def write(name, value, existing=None):
    return {"update": {"name": f"{ROOT}/{COLLECTION}/{name}",
                       "fields": {"payload": {"stringValue": encode(value)}}},
            "currentDocument": {"updateTime": existing["updateTime"]} if existing else {"exists": False}}


def publish(db, parts, board, previous, read_time):
    if previous:
        prior = content(previous)
        if prior["board"]["end_day"] > board["end_day"] or instant(prior["archive_read_at"]) > instant(read_time):
            raise ValueError("A newer study already exists")
    shards = {}
    writes = []
    for part in parts:
        name = f'{part["day"]}_{part["asset"]}_{digest(part)}'
        shards[f'{part["day"]}_{part["asset"]}'] = name
        existing = db.get(name)
        if existing is None:
            writes.append(write(name, part))
        elif content(existing) != part:
            raise ValueError("Stored partition failed its content check")
    if not writes and previous and content(previous)["partitions"] == shards and content(previous)["board"] == board:
        return 0
    manifest = {"archive_read_at": read_time, "partitions": shards, "board": board}
    writes.append(write("latest", manifest, previous))
    request = {"writes": writes}
    if len(encode(request).encode()) > 2_000_000:
        raise ValueError("Study batch exceeded its size limit")
    result = db.request(":commit", request)
    if len(result["writeResults"]) != len(writes) or content(db.get("latest")) != manifest:
        raise ValueError("Study publication could not be verified")
    for part in parts:
        if content(db.get(shards[f'{part["day"]}_{part["asset"]}'])) != part:
            raise ValueError("Partition readback mismatch")
    return len(writes)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--end-day", type=date.fromisoformat, required=True, help="Last included complete UTC day")
    parser.add_argument("--publish", action="store_true", help="Atomically save partitions and the aggregate to Firestore")
    parser.add_argument("--preview", type=Path, help="Save the public aggregate for local preview")
    args = parser.parse_args()
    # A short lag tolerates local clock skew while keeping all nine queries on one snapshot.
    snapshot_time = datetime.now(UTC) - timedelta(seconds=10)
    if args.end_day >= snapshot_time.date():
        raise ValueError("Select a completed UTC date")
    db = Firestore()
    previous = db.get("latest") if args.publish else None
    read_time = snapshot_time.isoformat(timespec="microseconds")
    days = [args.end_day - timedelta(days=n) for n in (2, 1, 0)]
    parts = [partition(a, d, db.archived_day(a, d, read_time)) for a in ASSETS for d in days]
    board = {"schema_version": 1, "start_day": days[0].isoformat(), "end_day": days[-1].isoformat(), "series": []}
    for asset in ASSETS:
        series = {"asset": asset, "cutoffs": [cohort([p for p in parts if p["asset"] == asset], m) for m in MINUTES]}
        board["series"].append(series)
        counts = [sum(b["n"] for b in c["bins"]) for c in series["cutoffs"]]
        print(f'{asset}: {series["cutoffs"][0]["settled"]}/288 official results, eligible quotes at +5/+10/+14: {counts}')
    if not any(b["n"] for s in board["series"] for c in s["cutoffs"] for b in c["bins"]):
        raise ValueError("No eligible quotes across the study")
    if args.publish:
        count = publish(db, parts, board, previous, read_time)
        print(f"Verified study in crypto-prediction-lab ({count} writes).")
    if args.preview:
        args.preview.write_text(encode(board) + "\n")


if __name__ == "__main__":
    try:
        main()
    except HTTPError as err:
        sys.exit(f"Firestore rejected the study operation (HTTP {err.code}).")
    except subprocess.CalledProcessError:
        sys.exit("Could not obtain the active Google Cloud login.")
    except (OSError, ValueError, KeyError, TypeError) as err:
        sys.exit(f"Study stopped: {err}")
