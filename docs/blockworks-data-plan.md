# Blockworks: a focused research month

Research date: 9 November 2026, America/New_York. A bounded local pilot is now implemented and verified; the recurring collection/cloud archive below remains a proposal. This is not a license grant or prediction result.

## Decision

Use the trial to evaluate independent historical context for BTC, ETH and SOL. Prioritize five-minute asset price/volume candles, then a small number of venue series and settled funding histories. Do not spend the month duplicating CMC's latest price cards or collecting unrelated protocol data.

The screenshot supplied by the owner shows 10,000 credits worth $100, expiring 9 December 2026. This is a Blockworks allowance, not Google Cloud credit. The official claim page says promotional credits last 30 days. The balance and expiration time have not been independently read from the account. [Credit promotion](https://app.blockworks.com/claim).

Initial Python catalog/authentication probes returned HTTP 403. Follow-up diagnosis identified Cloudflare error 1010 (`browser_signature_banned`). A standard curl ping succeeded, including with the saved key. After identifying the actual application with a truthful User-Agent, both the Python diagnostic and Rust pilot succeeded. A single 24-hour batch returned 288 valid candles for each asset. No credentials were printed or copied into the repository. Endpoint-specific credit costs and actual billed credits remain unverified; authentication success does not verify every dataset entitlement.

## Data priorities and real resolution

| Priority | Candidate | What we would learn | Resolution / limitation |
| --- | --- | --- | --- |
| 1 | BTC, ETH, SOL asset OHLCV | Short returns, volatility, range, relative volume and cross-asset moves | Asset-price documents 5m, 1h, 1d, 1w. Five-minute candles cannot reconstruct ticks or a final-minute CF average. |
| 2 | A few spot venue/pair candles | Venue divergence and USD versus stablecoin differences | Discover actual model intervals, currencies, venue coverage and latency before choosing. |
| 3 | Settled perpetual funding | Crowding context and changes between funding settlements | Funding model uses 1h buckets but records actual settlements, with intervalHours. An empty hour is not zero funding. |
| 4 | Perpetual volume and open interest | Slower leverage and market-regime context | Documented perpetual-markets timeseries is daily. It is not intraday OI or a live liquidation stream. |
| 5 | Monitoring developments | Event timelines around unusual windows | Keep first receipt and subsequent revisions; an event's historical date is not when we could first know it. Latency and access need measurement. |
| Later | Stablecoin/network/ETF/protocol metrics | Broader regimes or explanatory panels | Lower priority for a 15-minute horizon; verify publication timing and incremental value. |

References: [asset candles](https://docs.blockworks.com/api-reference/data-api/assets/asset-price), [market data](https://docs.blockworks.com/api-reference/data-api/market-data), [perpetual history](https://docs.blockworks.com/api-reference/data-api/derivatives/perpetual-markets), [funding definitions](https://docs.blockworks.com/api-reference/data-api/derivatives/perpetual-market-funding-rates), [monitoring](https://docs.blockworks.com/api-reference/monitoring/overview).

API catalogs describe supported intervals and account-accessible models. Credit balance is not proof of access to every dataset. The documented REST timeseries window limit is 9,000 points at the requested interval; use explicit nonoverlapping windows. MCP has separate response limits, so do not transfer its limits to a REST collector. [Time ranges](https://docs.blockworks.com/getting-started/concepts/granularity), [catalog discovery](https://docs.blockworks.com/getting-started/concepts/catalog).

## Rights before a long-term archive

The public terms restrict bulk extraction, external display and AI/ML use unless expressly authorized. Their Data/API exhibit also addresses deletion after the applicable agreement ends. The exact accepted account terms and any order-form exceptions must be confirmed; promotional credit expiry alone does not tell us when the license ends. Ask Blockworks specifically about internal GCP storage, historical backfills, retention after the trial, statistical research/model use, derived outputs and public dashboard display. Do not infer permission from downloadable CSV support or marketing examples. [Terms, especially Exhibit D](https://blockworks.com/terms).

The implemented evaluation is limited to a manually invoked local 24-hour snapshot and a loopback Data Lab projection. Bulk backfills, recurring collection, cloud retention, public display and model training remain pending. The deployed public Data Lab still includes no Blockworks observations. Nothing has been uploaded to Google Cloud or scheduled from this work.

## Credit plan after access and prices are verified

Proposed ceiling: spend at most 8,000 of the supplied 10,000 credits; reserve 2,000 for source revisions, gap repairs and useful follow-up queries. This is a planning allocation, not an implemented account spending cap.

1. Resolve the 403 and query accessible catalogs. Resolve canonical asset IDs rather than trusting ambiguous ticker symbols. Record units, model schema and supported intervals.
2. Measure one small allowed query per chosen family. Compare the account's usage before/after and determine whether billing scales with calls, rows, metrics or series. Rate-limit headers measure request allowance, not credits.
3. Run one permitted 24-hour pilot. Measure bytes, returned observations, actual publication delay, duplicates and source gaps. Forecast the whole month including discovery, backfill, retries and repair.
4. Choose the backfill range from measured cost. A proposed starting target is 30–90 days of five-minute candles for three assets, narrowed if allowance, licensing or historical availability requires it. Do not promise that this fits before measuring.
5. Batch series only where the endpoint supports it. Poll finalized five-minute candles once per shared collector, with a small overlap for revisions. Fetch settled funding only at its genuine update cadence; daily OI does not need frequent polling.
6. Stop at the approved credit ceiling or before the account's exact expiry time, whichever comes first. Fail closed when usage cannot be checked. Disable automatic purchases/upgrades and retain an operational stop switch.

The documented trial rate limit is 25 requests/hour; other plans have different allowances. Confirm the account's actual headers. Three separate five-minute polls would already require 36 requests/hour before other work. A single batched request would need 12/hour, if permitted and adequately priced. The API supports up to ten named series on the documented perpetual endpoints. Do not confuse request rate with credit cost. [Authentication and rate limits](https://docs.blockworks.com/getting-started/authentication).

## Storage architecture when authorized

Keep Firestore for queryable contract summaries and the latest context. Put licensed bulk history in a dedicated private Cloud Storage bucket, isolated from the other applications in gitunderstand. Extend the existing Rust collector for scheduled ingestion; the Python access-audit utility is a local diagnostic, not a replacement production stack.

Proposed object organization:

```text
raw/provider=blockworks/model=asset-price/asset=BTC/date=YYYY-MM-DD/<capture-id>.jsonl.gz
normalized/model=asset-price/asset=BTC/date=YYYY-MM-DD/<schema-version>.parquet
manifests/provider=blockworks/date=YYYY-MM-DD/<capture-id>.json
```

Raw objects preserve provider payloads and revisions; immutable capture IDs avoid rewriting what was previously seen. Parquet partitions serve repeat analysis. Start with compressed JSONL if introducing Parquet adds unnecessary initial dependencies. Compact records into sensible files rather than writing one cloud object or Firestore document per tick. Choose file size after the pilot's measured volume.

Every manifest should include schema version, provider/model, asset and native instrument IDs, requested bounds and granularity, metric units, HTTP status, payload checksum, bytes, row count, source timestamp range, fetch start/end, earliest observed availability, revision identity, quality flags and permitted-use/retention policy. Exclude request authentication. Use decimal values or scaled integers for prices and quantities. Keep null, zero and missing observations distinct.

Store official outcomes separately from predictor observations. Join by asset and contract times with an availability cutoff, not merely the provider's candle timestamp. Retrospectively downloaded history has no established historical availability time unless the provider supplies it. Today's corrected news summaries or candles cannot automatically become valid historical prediction inputs.

Raw-provider retention, backups, normalized files and derived datasets must follow confirmed rights; moving data to cold storage does not resolve a license restriction. Grant the public dashboard only the narrow summaries it needs. No visitor-triggered provider collection and no public raw-data export endpoint.

Measure compressed bytes/day, storage operations, reads, egress and collector runtime before creating schedules. Retain the existing $40 app budget target and resource controls; Blockworks credit does not pay these costs. BigQuery is optional later, with explicit scan limits, rather than a prerequisite for three assets. [Cloud Storage pricing](https://cloud.google.com/storage/pricing).

## Prediction research sequence

The largest remaining granularity gap is durable CF/Kalshi ingress capture independent of website visitors. Blockworks' five-minute history does not repair that gap. Keep Coinbase on hold and direct CF access pending licensing, as already decided.

Use project data to establish a distance-to-target/time-to-close/volatility baseline and compare with contemporaneously recorded Kalshi quotes. Only after model-use rights are confirmed, evaluate whether each added Blockworks family improves chronological held-out Brier score, log loss and calibration. Keep correlated BTC/ETH/SOL windows in the same fold and prevent overlap across feature/label horizons. Never use hindsight winning-side entry values as prediction features.

A complete 30-day period would contain 2,880 shared quarter-hour windows and at most 8,640 asset outcomes, before gaps or nonbinary results. Those correlated outcomes are not 8,640 independent experiments. More rows and visuals alone establish neither accuracy nor profit.

## Local work delivered

- `/?view=data`: aligned official-outcome atlas; 24/48/96-window selection; coverage and alignment counts; per-window source inspector; up to 24 hours of synchronized CMC rebased returns; return correlations with sample counts; source ledger; JSON observation export.
- `GET /api/context/history`: projects at most 288 stored samples from the last 24 hours, with a five-minute shared cache. No provider requests or writes are added. This raises the potential historical document reads compared with the existing 24-sample homepage response; measure actual usage before publishing.
- `GET /api/archive/window?asset=BTC&ticker=...`: validated read-only access to normalized stored one-minute YES-ask OHLC. Invalid, incomplete and outside-window candles are omitted, and the browser leaves those minutes blank. Source payloads and credentials are not exposed.
- `scripts/blockworks-audit.py`: catalog-only entitlement diagnostic with no automatic retries or data extraction. The key is read server-side from the supplied external file.
- `scripts/preview-data-lab.py`: loopback visual preview against existing public API responses. It intentionally cannot read private stored minute-candle payloads. The Rust archive route needs the application's configured database identity for an end-to-end populated cloud check.

The initial access findings above were superseded by the verified local pilot below. No collection schedule or public deployment should be inferred from this plan.

## Subsequent Data Lab publication

The owner subsequently requested publication. Application release `63c67ac` was pushed to GitHub and deployed as public revision `crypto-prediction-lab-00016-mer`. Real archived Kalshi minute quotes and existing CMC history passed deployed checks. This does not activate the proposed Blockworks collector or archive: that publication contained no Blockworks observations, subscriptions or new cloud collection. The subsequent local-only pilot below resolved authentication.


## Verified local pilot

Completed 9 November 2026, America/New_York (capture `2026-11-10T04:46:22Z`). Branch `codex/blockworks-pilot`; not deployed or pushed. All three asset series returned 288 valid five-minute candles with zero rejected points and no null volume values. The half-open requested provider-timestamp range is `2026-11-09T04:40:00Z` to `2026-11-10T04:40:00Z`. The conservative five-minute embargo does not establish publication latency or historical availability. Actual API response `volume` metadata did not specify a unit, so the UI labels it unverified despite the broader reference text describing USD volume. Candle timestamp start/end convention remains unverified; no Kalshi window join is inferred.

The earlier 403 was a diagnostic error: Cloudflare blocked Python's default client signature before API authentication. The key was valid. `CryptoPredictionLab/0.1 (read-only Blockworks evaluation)` truthfully identifies this application for both clients; verified ping now returns HTTP 200 / authenticated true. The diagnostic classifies future Cloudflare failures separately from invalid-key or account-permission errors, never printing the provider body or key.

`apps/dashboard/src/blockworks.rs` implements the existing Rust application's explicit `blockworks-pilot` role. It exits after at most three requests: accessible timeseries catalog, canonical BTC/ETH/SOL ID lookup, then one batched asset-price request. It refuses pre-existing output paths, follows no redirects, has a 20-second per-request timeout and 2 MB response/file bounds. No retry loop or schedule exists. Provider point_schema determines metric order; USD price units, UUIDs, asset names, OHLC bounds and timestamp windows are validated. Duplicate series/timestamps and ambiguous schemas stop the pilot; bad points remain gaps. Missing series stay empty; missing volume is distinct from zero. Only the normalized schema is saved with owner-only file permissions.

The sole real snapshot is ignored local data at `data/blockworks-pilot-20261110.json` (not committed). To inspect that existing snapshot without additional Blockworks requests:

```sh
BLOCKWORKS_LOCAL_SNAPSHOT=data/blockworks-pilot-20261110.json \
DASHBOARD_PORT=3005 cargo run --offline --locked -p dashboard
```

Open `http://127.0.0.1:3005/?view=data`. The read-only `/api/blockworks` route serves an immutable normalized startup projection, with no key, request headers or raw response. POST returns 405. Without a snapshot it returns `not_configured`. A configured snapshot on a non-loopback bind is rejected at startup. Browser refreshes and asset switches cannot spend Blockworks credits. The source ledger distinguishes Blockworks; observation exports continue to exclude it.

For another explicitly chosen small pilot, after checking usage and permitted uses:

```sh
mkdir -p data
BLOCKWORKS_API_KEY_PATH=/Users/shantanu/Desktop/Secrets/blockworks-api-key.txt \
BLOCKWORKS_PILOT_OUTPUT=data/blockworks-next-pilot.json \
DASHBOARD_ROLE=blockworks-pilot cargo run --offline --locked -p dashboard
```

This is request-bounded, **not a monetary or credit spending cap**. The single verified collection used three requests; additional diagnostic pings also occurred. No actual credit balance/charge was read. Compare usage on the account page before another data fetch. No automatic purchase, paid upgrade, additional category enablement or GCP changes occurred. Keep the proposed 8,000/2,000 allocation as a proposal until actual prices are known.

Next: check the measured credit charge, verify bucket convention/volume units and source delay, and confirm permitted public display/retention before extending beyond local evaluation. The next useful family is settled funding; Monitoring is optional and does not need enabling for this candle explorer.


## Production extension — 10 November 2026

The owner requested the seven-day price/volume pilot plus a “Before this outcome” inspector in Data Lab and explicitly authorized production deployment. The owner separately confirmed that the account agreement permits public BTC/ETH/SOL price/volume display. This supersedes the earlier local-display-only boundary for that data; it does not authorize model training or establish indefinite retention rights.

A single three-request batch returned 6,048 valid candles, 2,016 per asset, with no rejected points. The normalized file is `data/blockworks-seven-day-20261110.json`, excluded from Git/build uploads. The operator publisher stored three immutable asset records and one latest manifest in the existing named app database. The public app reads those saved observations with its existing viewer identity; it receives no Blockworks key and has no collection route. Display is bounded to a day of chart candles plus a cutoff-filtered preceding hour per inspected window. No recurring collection or additional subscription was introduced. Exact timing, record names and verification are in [before-outcome.md](before-outcome.md) and the current handoff. Actual credit charges remain unverified.
