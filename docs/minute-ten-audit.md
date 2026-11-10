# Minute-10 coverage and cutoff audit

Ticket A supplies a bounded read-only operator script, `scripts/audit_minute_ten.py`, an offline versioned model-packet projection and synthetic regression fixtures. Its GCP scope is the application's named Firestore database `crypto-prediction-lab` in `gitunderstand`. It reuses the existing quote-study reader and uses a single server read time for all snapshot and archive queries. Each requested UTC day checks exactly 96 snapshot IDs per asset and the matching bounded archive query. Limit each run to one through seven days. These are charged reads, including missing-document lookups.

## Verified extract

The [aggregate report](research-audits/minute-ten-2026-11-10.json) was read at **2026-11-10 16:21:41 UTC**. The explicit range is November 9–10 UTC, with the exclusive end November 11. There are 192 possible slots per asset in the full range. Only the 161 already completed slots count toward audit coverage. The collector first saved the window opening November 10 at 08:00 UTC, with a checkpoint after 08:10. All 33 completed windows from that opening through 16:00 have snapshots for all three assets.

| Asset | Completed slots audited | Official YES/NO results | Usable books | Books matched to official result | Available at exact minute 10 |
|---|---:|---:|---:|---:|---:|
| BTC | 161 | 161 | 33 | 33 | 0 |
| ETH | 161 | 161 | 33 | 33 | 0 |
| SOL | 161 | 161 | 33 | 33 | 0 |

Each asset has 128 earlier missing snapshots in this range, including all 96 November 9 windows. There are zero missing snapshots since the first saved opening, zero mismatched tickers and zero usable books awaiting an official result in the completed cohort. Calendar slots are the denominator. Missing archives do not establish whether a market was listed, eligible or open at that time. The audit does not invent historical market status.

Median receipt delays after minute 10 were 18.396445s BTC, 18.552356s ETH and 18.679124s SOL. The p95 delays were 28.819064s, 28.985830s and 29.137194s respectively. Exact timing, daily counts and the frozen-extract SHA-256 are in the report. A usable book requires a valid asset/window identity, available capture status, capture within minute 10, positive target, two nonempty valid ladders with positive sizes and an uncrossed best bid/derived ask. It describes observed displayed liquidity. The source event timestamp was not captured, so provider quote age remains unknown. Usability does not establish source freshness, available fees, fillability or a profitable opportunity.

## Repeat the audit

Save captures under the Git-ignored `data/` directory. The script refuses to overwrite an extract when fetching. Only aggregate output and synthetic fixtures belong in Git.

```sh
python3 scripts/audit_minute_ten.py \
  --start 2026-11-09 --end-exclusive 2026-11-11 \
  --extract data/minute-ten-audit-20261110.json --fetch \
  --report data/minute-ten-report.json
```

Replay the frozen extract without GCP access by omitting `--fetch`. The same extract yields the same report, including its manifest hash. A fresh extraction must use a new local filename. All cloud operations are queries or batch reads. The tool does not collect provider data or write to Firestore.

## Model visibility contract

`model_packet(document, asset, open_epoch, cutoff_rfc3339)` takes a single prospective snapshot, never an outcome document. It returns schema version 1, a feature allowlist and canonical SHA-256 hash. It accepts cutoffs from minute 10 until close. Features appear only at or after the later of combined source receipt and Firestore document creation. This is a conservative first durable availability bound. Legacy snapshots have one combined receipt for sequential metadata/book requests, so separate component timing stays unknown. Nanosecond timestamps are rounded upward to Python microsecond precision to prevent early visibility. A packet older than 60 seconds relative to combined receipt becomes stale under this audit's explicit policy. This is not a provider freshness guarantee.

Features include ticker, numeric target, bid ladders in dollars, complementary ask, midpoint, spread and timing disclosures. The exact-minute-10 packet has no book, target or other observation fields when receipt is later. Official result, expiration value, hindsight entry, archive timestamps, future candles and arbitrary provider fields are excluded from the projection and hash. A missing record remains unavailable. A ticker from another asset or a rollover opening is rejected. Each result is a new packet for its selected cutoff.

Synthetic fixture tests verify a 21-second-late receipt, later durable visibility, late combined metadata, gaps, crossed/invalid depth, rollover, all three asset schemas, outcome mutation independence, timestamp precision and deterministic report denominators. A second offline check runs those guards against all 99 actual saved records without committing captures. These tests establish the data projection contract. A future MCP host must enforce that contract and deny direct archive access to agents.

## Next step

Ticket B can add opening and post-decision captures using these timing semantics. First measure the current roughly 18–30 second checkpoint delays when choosing deadlines. The existing dataset supports forecasts made after actual snapshot availability. It cannot supply contemporaneous books at the exact minute-10 boundary or evidence of later paper fills.
