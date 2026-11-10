# Settled contract archive and outcome room

## What is stored

A dedicated Firestore Standard database named crypto-prediction-lab in gitunderstand/us-central1 stores settled BTC, ETH and SOL 15-minute contracts. The existing default database is untouched. Each native market ticker is an idempotent document ID in contracts_btc, contracts_eth or contracts_sol.

Each document contains an indexed close_time, an unindexed summary and an unindexed payload. The payload retains schema_version, the full public market record and the available one-minute candlesticks from Kalshi. The summary contains the official result, target, expiration value, source update time, capture time, candle count and a hindsight winning-side quote observation.

There is no TTL deletion or truncation of older documents. The collector revisits the recent 48-hour settlement range on each run, capturing missed/revised records and retrying records without candles. A source outage longer than this range needs a manual backfill extension; gaps must remain visible. This archive does not reconstruct historical per-tick data or order-book depth.

## Collection and security

A separate private Cloud Run service runs the same binary with DASHBOARD_ROLE=collector. Its only data-write route is POST /collect, protected by Cloud Run IAM. It has no Kalshi secret mounts and does not start live feed workers. The scheduler uses its own OIDC identity to invoke it every 15 minutes. The collector identity has roles/datastore.user conditioned on this database only. The public dashboard identity has roles/datastore.viewer conditioned on the same database. The public application has no write route.

A collector run reads up to 220 existing summaries per asset, then pages recent settled markets. Identical source update timestamps with nonempty candle data are skipped. Stable document IDs avoid duplicate contracts; a per-instance collection lock prevents overlapping requests. Per-record failures produce a failed run so the schedule retries. Successful records remain durable. No credentials are included in stored market data.

## Public display

GET /api/outcomes?asset=BTC returns up to 96 summaries, cached for 60 seconds per instance with a short public HTTP cache. Unsupported asset input is rejected. Raw payloads are not exposed to browsers. The UI defaults to 48 15-minute slots, with 24/48/96 selectors. Missing or not-yet-settled windows are neutral, rather than silently skipped. Times follow the viewer's device timezone. Green means YES at or above target, red means NO below target. A gap breaks the displayed run count.

## Hindsight entry analysis

For a YES winner, use the minimum positive yes_ask.low_dollars during the contract window. For a NO winner, derive NO ask as 1 minus YES bid; the lowest NO ask in a minute is 1 minus yes_bid.high_dollars. Reject invalid/zero/one values and candles outside open/close. Use the earliest minute to break equal-price ties.

The gross multiple is $1 divided by the winning-side ask. A winning binary contract settles at $1; the net hypothetical per-contract profit before fees is $1 minus the ask. The price appears somewhere inside the named minute, not necessarily at its start. We do not know quote size, fillability or queue position and do not label the result as a trade, a forecast, a maximum executable payout or a recommended entry. Fees are not modeled. Past red/green runs do not establish an edge for the next contract.

## Price decision

Firestore has no fixed server cost and fits small indexed market summaries. Its named database has no free quota because the project already has a database. Current us-central1 list rates are $0.03 per 100,000 reads and $0.09 per 100,000 writes. At roughly 288 new documents/day plus a recent-summary scan every 15 minutes, database operations should be well below $1/month at pilot scale. Storage, repeated partial-data retries, public reads, collector CPU and network also cost money. Plan approximately $1–$2/month incremental history costs for a small pilot, not a guaranteed bill. The existing live dashboard can still exceed the overall $10 target with sustained traffic.

A Scheduler job costs $0.10/month before any shared free allowance. A scale-to-zero private HTTP collector avoids a continuously running worker and Cloud Run Jobs' minimum execution billing. Cloud Storage is a candidate for future bulk tick archives but is less convenient for this initial queryable results board. Cloud SQL adds server cost that this feature does not require.

References checked 6 November 2026:
- https://cloud.google.com/firestore/pricing
- https://cloud.google.com/scheduler/pricing
- https://cloud.google.com/storage/pricing
- https://docs.kalshi.com/api-reference/market/get-markets
- https://docs.kalshi.com/api-reference/market/get-market-candlesticks
- https://docs.cloud.google.com/firestore/native/docs/manage-databases

## Recovery and scope

The collector is invoked by an authenticated schedule, not by visitors, and runs even when nobody has the public page open. No all-day live WebSocket connection is required for settled-contract archival. A restart does not remove stored results. Repeated collection is idempotent for unchanged completed records. Store raw market and candle payloads for later research; do not advertise these as a full order-book replay dataset.

To extend a backfill beyond 48 hours, first consult Kalshi's historical cutoff and archive endpoints and explicitly adjust the bounded collector. The current collector intentionally does not scan all exchange history. The UI shows up to 24 hours; the database keeps older captured contracts without automatic deletion. Check actual reads, writes and stored bytes before expanding to per-tick capture.

## Initial verification

The initial collector encountered public-source HTTP 429 responses. Successfully written records remained durable. Recovery adds 750 ms pacing per request, bounded Retry-After/exponential backoff, and at most 80 new or incomplete captures per asset per run. Bootstrap fills older intervals over multiple runs; the public board retains neutral gaps. A mock HTTP test verifies a 429 followed by a successful JSON response.

On 6 November 2026, independent database checks counted 195 BTC, 194 ETH and 189 SOL contracts (578 total). Sample stored winning-side quote calculations were independently recomputed using source minute candles and matched for all three assets. The private collector rejected an unauthenticated POST with 403, the database rejected unauthenticated direct queries, and the public application does not expose /collect. Counts grow as collection continues.

The read-only runtime and writer identities have database-conditioned IAM roles approved by the user. The scheduler identity has only run.invoker on the private collector. Scheduler: crypto-history-quarter-hour in us-central1, minutes 2/17/32/47 UTC, one retry. This is a delayed settled-results archive, not a tick-by-tick trading record.

Final release: public revision crypto-prediction-lab-00003-64v; private collector revision crypto-history-collector-00002-bgg. Image tag: outcomes-paced-20261106. The public board passed browser selection and hindsight-detail checks with actual stored outcomes. The most recent 96 contracts for each asset had no internal 15-minute gaps at verification. Checks passed: 21 Rust tests, six JavaScript tests, formatting and clippy. The public /api/health route replaces the earlier externally intercepted /healthz path.
