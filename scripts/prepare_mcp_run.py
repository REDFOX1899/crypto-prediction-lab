#!/usr/bin/env python3
"""Trusted operator: issue one frozen MCP scope without printing credentials/data."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import urllib.error
import urllib.request

COLLECTOR = "https://crypto-history-collector-a53petnlfa-uc.a.run.app"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--asset", choices=["BTC", "ETH", "SOL"], required=True)
    parser.add_argument("--open-time", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    # Local captures must not enter source control, even with a custom destination.
    root = Path(__file__).resolve().parents[1]
    destination = args.output.resolve()
    if root in destination.parents and root / "data" not in destination.parents:
        parser.error("Use the ignored data directory or a path outside this repository")
    token = subprocess.check_output(["gcloud", "auth", "print-identity-token"], text=True).strip()
    body = json.dumps({"asset": args.asset, "open_time": args.open_time}).encode()
    request = urllib.request.Request(COLLECTOR + "/research/mcp-scope", data=body,
                                    headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            value = json.load(response)
    except urllib.error.HTTPError as error:
        raise SystemExit("Private scope issuance failed (HTTP %s); no eligible packet or operator access" % error.code)
    destination.mkdir(mode=0o700, parents=True, exist_ok=True)
    for name, record in [("scope.json", value["scope"]), ("capability.json", {key: value[key] for key in ["capability", "expires_at", "run_id", "packet_hash"]})]:
        path = destination / name
        descriptor = os.open(str(path), os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)
        with os.fdopen(descriptor, "w") as output:
            os.chmod(path, 0o600)
            json.dump(record, output)
    print("Prepared %s frozen scope in %s (expires %s)" % (args.asset, destination, value["expires_at"]))


if __name__ == "__main__":
    main()
