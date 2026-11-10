# Blockworks: a focused research month

Research date: 9 November 2026, America/New_York. This is a collection proposal, not a deployed integration, license grant or prediction result.

## Decision

Use the trial to evaluate independent historical context for BTC, ETH and SOL. Prioritize five-minute asset price/volume candles, then a small number of venue series and settled funding histories. Do not spend the month duplicating CMC's latest price cards or collecting unrelated protocol data.

The screenshot supplied by the owner shows 10,000 credits worth $100, expiring 9 December 2026. This is a Blockworks allowance, not Google Cloud credit. The official claim page says promotional credits last 30 days. The balance and expiration time have not been independently read from the account. [Credit promotion](https://app.blockworks.com/claim).

The saved key was used only for bounded catalog discovery. Authenticated catalog requests returned HTTP 403; no market observations were downloaded, no key was logged or copied into the repository, and no automated retry loop was started. A 403 does not establish which datasets this account can access or whether the key itself is valid. Endpoint-specific credit costs and actual billed credits remain unverified.

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

Until those uses are confirmed, the integration stays at discovery and planning. The new public-facing Data Lab uses the existing Kalshi/CMC responses; it includes no Blockworks observations. Nothing has been uploaded to Google Cloud or scheduled from this work.

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

Next: obtain confirmed API access and permitted uses, then measure a tightly bounded collection pilot. No collection schedule or deployment should be inferred from this plan.

## Subsequent Data Lab publication

The owner subsequently requested publication. Application release `63c67ac` was pushed to GitHub and deployed as public revision `crypto-prediction-lab-00016-mer`. Real archived Kalshi minute quotes and existing CMC history passed deployed checks. This does not activate the proposed Blockworks collector or archive: access remains unverified after HTTP 403, and no Blockworks observations, subscriptions or new cloud collection were added.
