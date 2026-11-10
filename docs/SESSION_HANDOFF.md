# Session handoff

Updated: 9 November 2026, America/New_York.

## Current checkout and deployment

- Work in `/Users/shantanu/Documents/ChatGPT/Crypto Prediction Lab`, the attached Rust checkout. Do not switch to the older Desktop checkout.
- GitHub: https://github.com/REDFOX1899/crypto-prediction-lab.
- Current local branch: `codex/data-lab`, based on `543028f`. Feature changes are local and uncommitted. No push or deployment was performed.
- Last application release remains `66e3bf7`: public revision `crypto-prediction-lab-00012-jx6`, collector `crypto-history-collector-00005-msp`, as recorded previously. No fresh production deployment/configuration audit was performed this session.
- Preserve existing cloud limits, dedicated database/identities, disabled trading/RFQs, Coinbase hold and pending direct CF licensing.

## Completed local work

The owner asked for a richer data website and an evaluation of a one-month Blockworks credit grant, then authorized continued local development while away.

- Added `/?view=data` to the existing Rust/Axum/plain-JS application. Existing Home, Single market and Combo lab routes remain available.
- Outcome atlas aligns BTC, ETH and SOL on explicit completed quarter-hour slots. It supports 24/48/96 windows, preserves gaps, reports coverage and computes three-asset direction alignment only on complete windows. Source identity, duration, future data and nonbinary results are validated.
- Per-window inspector displays official results, target text, expiration values and source candle counts. Asset buttons load normalized archived one-minute YES-ask OHLC with fixed 0–100¢ charts, numeric tables and visible missing minutes. These are quotes, not fills or forecasts.
- Rebased CMC comparison chart and sample-value table keep collection and source timestamps separate. Correlation uses synchronized consecutive returns, excludes gaps, unchanged source timestamps and source times more than two minutes apart, requires at least six variable return pairs, and shows sample counts. CMC is not the CF settlement reference.
- Source ledger and JSON observation export retain source/availability semantics. Export is explicitly not a point-in-time training dataset and contains no Blockworks data.
- `GET /api/archive/window?asset=BTC&ticker=...` validates the asset/ticker association before reading a single document from its asset collection. Only normalized, completed, in-window candles leave the server; raw payloads do not. Response read is capped at 2 MiB, route timeout is ten seconds, and shared detail cache is limited to 32 entries / five minutes. No write route was added.
- `GET /api/context/history` projects at most 288 saved CMC samples within 24 hours, with a five-minute shared cache. Homepage history remains at 24 samples. This adds no provider polling or collection writes, but the larger optional history query can increase Firestore reads if deployed; measure before publication.
- New requests have timeouts and are cancelled on leaving the workspace. Stale quote-detail replies cannot paint a different selection.
- Added local catalog-only Blockworks audit utility and loopback visual-preview helper. Added Python cache ignores. No production dependencies were added.

## Blockworks findings and unresolved access

The owner's screenshot shows 10,000 credits / $100, expiring 9 December 2026. Account balance, actual credit charges, exact expiry time and per-request pricing have not been independently verified. This is not Google Cloud credit.

The external saved API key exists and is a single unquoted token. Bounded authenticated catalog requests returned HTTP 403; they did not return market data. No key contents were printed, copied into this checkout or uploaded to a new secret resource. Stop further account queries until access is resolved rather than starting a retry loop. The CLI now prints a sanitized HTTP status on access failure.

First-party documentation confirms asset-price supports 5m history, perpetual OI/volume history is daily, and funding is settled-interval data with 1h buckets and intervalHours. Do not market this as a tick feed or minute-by-minute OI/liquidations.

Published terms restrict bulk extraction, public redistribution and AI/ML use without appropriate permission, and address retention on agreement termination. Confirm the actual accepted account agreement and exceptions. Credit expiry alone is not proof the data license terminates that day. No bulk download, model training, public Blockworks display or cloud collection was implemented.

`blockworks-data-plan.md` records the ranked data shortlist, a proposed 8,000-credit ceiling with a 2,000-credit reserve, request-rate considerations, a private Cloud Storage raw/normalized/manifest layout, timestamp/revision requirements and a staged pilot. These are proposals, not active schedules, spending caps or created resources.

## Verification performed

- `/Users/shantanu/.cargo/bin/cargo fmt --check` passed.
- `/Users/shantanu/.cargo/bin/cargo clippy --offline --locked --workspace --all-targets -- -D warnings` passed.
- `/Users/shantanu/.cargo/bin/cargo test --offline --locked --workspace` passed: 40 tests total (36 dashboard, four client). Local mock-server tests required permission to bind loopback ports; the first restricted run failed on that environment restriction, then the enabled run passed.
- `node --test apps/dashboard/tests/*.test.cjs` passed: 49 tests, including ten new Data Lab tests plus an additional source-synchronization test (11 new total).
- `python3 -m unittest discover -s scripts/tests -v` passed: three audit tests, including stopping at 403 and rejecting redirects.
- Local HTTP checks: archive route rejected wrong-asset tickers, path escapes and unsupported assets with 400; GET without a configured archive returned explicit `not_configured`; POST returned 405. CMC history GET returned explicit `not_configured` locally; POST returned 405.
- Browser inspected desktop and 390px mobile, light and dark. Real existing public outcome summaries and CMC observations populated the preview. Window selection, 96-window control, source inspector and JSON download were exercised; the downloaded file's schema, window count and asset keys were independently checked.
- The preview only uses the currently deployed public short-history response. A populated cloud read through the new minute-candle and 24-hour routes has **not** been verified end to end. Backend normalization/projection and failure behavior are tested. Do not claim a deployed candle inspector.
- `git diff --check` and local documentation-link checks are the final documentation checks. No unrelated production resources were changed.

## Local review

- Visual preview: http://127.0.0.1:3002/?view=data, started with `python3 scripts/preview-data-lab.py`. This helper reads existing public responses only and does not have cloud credentials or private candle access. It maps the new history path onto the currently deployed short-context response for visual QA.
- Rust app: http://127.0.0.1:3001/?view=data, started with `DASHBOARD_PORT=3001 /Users/shantanu/.cargo/bin/cargo run --offline --locked -p dashboard`. This local process has no archive identity configured and reports missing archive data honestly.
- Full-page screenshot: `/private/tmp/crypto-data-lab-preview.jpg` (temporary local artifact).
- A browser-test JSON export was downloaded to `/Users/shantanu/Downloads/crypto-data-lab-2026-11-10.json`; its exported_at and file date are UTC. It contains public observations, not credentials, and is outside the repository.

## Next concrete steps

1. Review the local Data Lab and pending diff. Keep it local unless the owner requests publication.
2. Resolve Blockworks API access and confirm internal storage, long-term retention, public display and model-use rights. Measure actual credit cost with one permitted small query before allocating the trial.
3. If authorized, implement a bounded ingestion pilot in the existing Rust collector, with idempotence, raw/revision manifests and observed availability times. Measure cloud storage/operation costs before any new schedule or always-on worker.
4. Before deploying this feature, verify populated new routes using the dedicated configured database identity, check actual response sizes/read costs, then obtain publication authorization. No production resources were changed during local development.
