# Session handoff

Updated: 9 November 2026, America/New_York.

## Current checkout and deployment

- Work in `/Users/shantanu/Documents/ChatGPT/Crypto Prediction Lab`, the attached Rust checkout. Do not switch to the older Desktop checkout.
- GitHub: https://github.com/REDFOX1899/crypto-prediction-lab.
- Current local branch: `codex/data-lab`. Application release `63c67ac` is pushed to both GitHub `main` and `codex/data-lab`. A subsequent documentation-only commit records deployment results.
- Deployed release is `63c67ac`: public revision `crypto-prediction-lab-00016-mer`, serving 100% of public traffic. Private collector remains `crypto-history-collector-00005-msp`. The public service configuration was compared before/after deployment; identity, environment, secret mounts, resource limits, concurrency and timeout are unchanged.
- Preserve existing cloud limits, dedicated database/identities, disabled trading/RFQs, Coinbase hold and pending direct CF licensing.

## Completed work

The owner asked for a richer data website and a Blockworks trial evaluation, authorized local development while away, then explicitly requested deployment and GitHub publication.

- Added `/?view=data` to the existing Rust/Axum/plain-JS application. Existing Home, Single market and Combo lab routes remain available.
- Outcome atlas aligns BTC, ETH and SOL on explicit completed quarter-hour slots. It supports 24/48/96 windows, preserves gaps, reports coverage and computes three-asset direction alignment only on complete windows. Source identity, duration, future data and nonbinary results are validated.
- Per-window inspector displays official results, target text, expiration values and source candle counts. Asset buttons load normalized archived one-minute YES-ask OHLC with fixed 0–100¢ charts, numeric tables and visible missing minutes. These are quotes, not fills or forecasts.
- Rebased CMC comparison chart and sample-value table keep collection and source timestamps separate. Correlation uses synchronized consecutive returns, excludes gaps, unchanged source timestamps and source times more than two minutes apart, requires at least six variable return pairs, and shows sample counts. CMC is not the CF settlement reference.
- Source ledger and JSON observation export retain source/availability semantics. Export is explicitly not a point-in-time training dataset and contains no Blockworks data.
- `GET /api/archive/window?asset=BTC&ticker=...` validates the asset/ticker association before reading a single document from its asset collection. Only normalized, completed, in-window candles leave the server; raw payloads do not. Response read is capped at 2 MiB, route timeout is ten seconds, and shared detail cache is limited to 32 entries / five minutes. No write route was added.
- `GET /api/context/history` projects at most 288 saved CMC samples within 24 hours, with a five-minute shared cache. Homepage history remains at 24 samples. This adds no provider polling or collection writes. The larger optional history query can increase Firestore reads; the shared cache and fixed limit remain in place.
- New requests have timeouts and are cancelled on leaving the workspace. Stale quote-detail replies cannot paint a different selection.
- Added local catalog-only Blockworks audit utility and loopback visual-preview helper. Added Python cache ignores. Added `flate2` for fast gzip encoding of public responses before outbound-byte accounting.

## Blockworks findings and unresolved access

The owner's screenshot shows 10,000 credits / $100, expiring 9 December 2026. Account balance, actual credit charges, exact expiry time and per-request pricing have not been independently verified. This is not Google Cloud credit.

The external saved API key exists and is a single unquoted token. Bounded authenticated catalog requests returned HTTP 403; they did not return market data. No key contents were printed, copied into this checkout or uploaded to a new secret resource. Stop further account queries until access is resolved rather than starting a retry loop. The CLI now prints a sanitized HTTP status on access failure.

First-party documentation confirms asset-price supports 5m history, perpetual OI/volume history is daily, and funding is settled-interval data with 1h buckets and intervalHours. Do not market this as a tick feed or minute-by-minute OI/liquidations.

Published terms restrict bulk extraction, public redistribution and AI/ML use without appropriate permission, and address retention on agreement termination. Confirm the actual accepted account agreement and exceptions. Credit expiry alone is not proof the data license terminates that day. No bulk download, model training, public Blockworks display or cloud collection was implemented.

`blockworks-data-plan.md` records the ranked data shortlist, a proposed 8,000-credit ceiling with a 2,000-credit reserve, request-rate considerations, a private Cloud Storage raw/normalized/manifest layout, timestamp/revision requirements and a staged pilot. These are proposals, not active schedules, spending caps or created resources.

## Verification performed

- `/Users/shantanu/.cargo/bin/cargo fmt --check` passed.
- `/Users/shantanu/.cargo/bin/cargo clippy --offline --locked --workspace --all-targets -- -D warnings` passed.
- `/Users/shantanu/.cargo/bin/cargo test --offline --locked --workspace` passed: 44 tests total (40 dashboard, four client). Local mock-server tests required permission to bind loopback ports; the first restricted run failed on that environment restriction, then the enabled run passed.
- `node --test apps/dashboard/tests/*.test.cjs` passed: 49 tests, including ten new Data Lab tests plus an additional source-synchronization test (11 new total).
- `python3 -m unittest discover -s scripts/tests -v` passed: three audit tests, including stopping at 403 and rejecting redirects.
- Local HTTP checks: archive route rejected wrong-asset tickers, path escapes and unsupported assets with 400; GET without a configured archive returned explicit `not_configured`; POST returned 405. CMC history GET returned explicit `not_configured` locally; POST returned 405.
- Browser inspected desktop and 390px mobile, light and dark. Real existing public outcome summaries and CMC observations populated the preview. Window selection, 96-window control, source inspector and JSON download were exercised; the downloaded file's schema, window count and asset keys were independently checked.
- The temporary Cloud Run revision and final public URL both passed 18 HTTP checks, including real stored minute quotes for all three assets (15 valid candles each), the bounded 24-hour history query, all three live market/reference feeds and rejected invalid/write requests. The history response contained 17 actual stored samples at verification (5,297 decoded bytes); each quote-detail response was 1,527 bytes. Older observations are not invented.
- `git diff --check` and local documentation links passed. Both GitHub Actions runs for `63c67ac` passed, including the added Python audit checks. No unrelated production resources were changed.

## Local preview helpers (stopped after publication)

- Visual preview: http://127.0.0.1:3002/?view=data, started with `python3 scripts/preview-data-lab.py`. The preview process was stopped after publication to remove its polling traffic. This helper reads existing public responses only and does not have cloud credentials or private candle access. It maps the new history path onto the currently deployed short-context response for visual QA.
- Rust app: http://127.0.0.1:3001/?view=data, started with `DASHBOARD_PORT=3001 /Users/shantanu/.cargo/bin/cargo run --offline --locked -p dashboard`. The local process was stopped after verification. It has no archive identity configured and reports missing archive data honestly.
- Full-page local preview screenshot: `/private/tmp/crypto-data-lab-preview.jpg`; live deployed screenshot: `/private/tmp/crypto-data-lab-live.jpg` (temporary local artifacts).
- A browser-test JSON export was downloaded to `/Users/shantanu/Downloads/crypto-data-lab-2026-11-10.json`; its exported_at and file date are UTC. It contains public observations, not credentials, and is outside the repository.

## Next concrete steps

1. Review the live Data Lab at https://crypto-prediction-lab-308289525742.us-central1.run.app/?view=data. Source revision is `63c67ac`.
2. Resolve Blockworks API access and confirm internal storage, long-term retention, public display and model-use rights. Measure actual credit cost with one permitted small query before allocating the trial.
3. If authorized, implement a bounded ingestion pilot in the existing Rust collector, with idempotence, raw/revision manifests and observed availability times. Measure cloud storage/operation costs before any new schedule or always-on worker.
4. Monitor actual read usage and data freshness after this release. Publication was authorized and completed; do not re-enable the temporary validation tag or redeploy for documentation-only updates.

## Published release details

- Cloud Build: `9be3254e-d6ae-4a13-8360-bd7433b64466`, succeeded.
- Public image: `us-central1-docker.pkg.dev/gitunderstand/crypto-prediction-lab/dashboard:data-lab-20261109-r2`. Deployment pins digest `sha256:8b2d525eea3b187853d4b2d51663d94f50e121ec8194f180151ee7b672f387bd`.
- Deployed to a temporary tag with zero public traffic, verified actual data, then routed 100% to `crypto-prediction-lab-00016-mer`. Temporary `data-lab` tag was removed afterward.
- Service-level and revision-level max one; 0.25 CPU / 256 MiB; concurrency one; min zero; gen1 request billing; CPU boost off; 30-second public timeout; existing service account and secret mounts preserved. Public invoker access is unchanged.
- Private collector, scheduler configuration, IAM, source subscriptions and database writes were not changed. No Blockworks integration or new collection was activated.
- GitHub checks: https://github.com/REDFOX1899/crypto-prediction-lab/actions/runs/42755805886 and https://github.com/REDFOX1899/crypto-prediction-lab/actions/runs/42755805787.
- Temporary verification reports: `/private/tmp/data-lab-candidate-checks.json` and `/private/tmp/data-lab-live-checks.json`. These contain public record IDs/statuses and response sizes, not secrets.

## Delivery issue resolved during public verification

Live API checks passed on the initial release, but a browser cold load exposed starvation of large HTML/library responses under the existing shared outbound allowance while other tabs polled feeds. The owner permitted increasing GCP capacity if necessary. Instead, the release adds fast gzip for eligible HTML, scripts, CSS, SVG and JSON before accounting for encoded body bytes; no GCP capacity increase was needed. The same 1 MiB burst / 8 KiB per second allowance, 1 MiB decoded-body ceiling and resource limits remain. Identity clients and explicit gzip refusal are supported; cache responses vary by Accept-Encoding. Tests cover gzip round trips, negotiation, double-encoding avoidance and a page loading within the existing depleted allowance.

Release `63c67ac` includes this correction. Public HTML measured 36,736 gzip bytes versus 107,948 decoded bytes. All 18 final public HTTP checks passed, all three market/reference feeds reported live, and browser verification on the public URL rendered actual 15/15-minute archived quote charts. The local preview polling was stopped. Local test total is now 96 (44 Rust, 49 JavaScript, three Python); formatting, clippy and both GitHub checks passed.
