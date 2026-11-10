#!/usr/bin/env python3
"""Read-only entitlement discovery. Never downloads market observations or prints keys."""
import argparse
import json
import os
from pathlib import Path
import sys
from datetime import datetime, timezone
from urllib.request import Request, build_opener, HTTPRedirectHandler
from urllib.error import HTTPError, URLError

MODELS = {"asset-price", "market", "market-price", "perpetual-markets",
          "perpetual-market-funding-rates", "perp-dex-protocols", "asset-volatility"}
MAX_BYTES = 4_000_000


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None  # Never forward the authentication header to another location.


def audit(key):
    opener = build_opener(NoRedirect())
    report = {"checked_at": datetime.now(timezone.utc).isoformat(), "catalogs": {},
              "scope": "Catalog metadata only; no timeseries or market records fetched.",
              "credit_cost": "Unverified; rate-limit headers do not measure API credits."}
    for kind in ("tabular", "timeseries"):
        request = Request(f"https://api.blockworks.com/query/{kind}/catalog?accessible=true&publicOnly=true",
                          headers={"X-Blockworks-API-Key": key, "Accept": "application/json"})
        try:
            with opener.open(request, timeout=20) as response:
                raw = response.read(MAX_BYTES + 1)
                if len(raw) > MAX_BYTES:
                    raise ValueError("Response exceeds size limit")
                payload = json.loads(raw)
                if payload.get("error"):
                    raise ValueError("Provider returned an error")
                models = payload.get("data", {}).get("models")
                if not isinstance(models, list):
                    raise ValueError("Unexpected catalog schema")
                report["catalogs"][kind] = {
                    "model_count": len(models),
                    "relevant_models": [m for m in models if m.get("slug") in MODELS],
                    "rate_limit": {h: response.headers.get(h) for h in
                                   ("X-RateLimit-Limit", "X-RateLimit-Remaining", "X-RateLimit-Reset")}}
        except HTTPError as error:
            # Do not print response bodies, request headers or key-bearing exceptions.
            raise ValueError(f"Catalog unavailable: HTTP {error.code}; stopped without retry") from None
        except (URLError, TimeoutError, OSError):
            raise ValueError("Catalog connection unavailable; stopped without retry") from None
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--key-file", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=Path("data/blockworks-access-audit.json"))
    args = parser.parse_args()
    try:
        key = args.key_file.read_text().strip()
        if not key or len(key) > 512 or any(c.isspace() for c in key):
            raise ValueError("Key file must contain only the API key")
        report = audit(key)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        fd = os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, "w") as output:
            json.dump(report, output, indent=2)
        print(f"Catalog audit saved to {args.output}. No market data collected.")
        for kind, catalog in report["catalogs"].items():
            print(f"{kind}: {catalog['model_count']} accessible models")
            for model in catalog["relevant_models"]:
                print(f"  {model['slug']}: intervals={model.get('intervals', [])}")
    except (ValueError, OSError, KeyError, TypeError) as error:
        message = str(error) if str(error).startswith("Catalog unavailable: HTTP ") else "Audit could not complete. Check local key access, account entitlements and network. No automatic retries."
        print(message, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
