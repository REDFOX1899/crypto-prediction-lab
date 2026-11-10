# Session handoff

Updated: 9 November 2026, America/New_York.

## Current checkout and deployment

- Work in `/Users/shantanu/Documents/ChatGPT/Crypto Prediction Lab`, the attached Rust checkout. Do not switch to the older Desktop checkout.
- GitHub: https://github.com/REDFOX1899/crypto-prediction-lab.
- Current branch: `codex/blockworks-pilot`. This session's changes are local and uncommitted; nothing was pushed or deployed.
- Public release remains `63c67ac`, revision `crypto-prediction-lab-00016-mer`, with the previous Data Lab and response compression. Private collector remains `crypto-history-collector-00005-msp`.
- Preserve existing cloud limits, dedicated database/identities, disabled trading/RFQs, Coinbase hold and pending direct CF licensing. No cloud resources, IAM, schedules, subscriptions or limits changed in this session.

## Completed: working Blockworks local evaluation

The owner requested use of the supplied Blockworks API key to improve Data Lab, with 10,000 promotional credits / $100 shown in the supplied account screenshot. Publication was not requested in this session.

**Authentication works.** Initial Python ping/catalog probes returned 403, including when the owner ran the diagnostic in Terminal. A keyless response identified Cloudflare error 1010 (`browser_signature_banned`) against Python's default client identity. Standard curl returned pong, including with the saved key. Both the diagnostic and Rust client now truthfully identify `CryptoPredictionLab/0.1 (read-only Blockworks evaluation)`; the corrected Python ping returned HTTP 200 / authenticated true. Do not report the earlier 403 as an invalid key or unresolved dataset activation issue.

- Added `apps/dashboard/src/blockworks.rs` in the existing Rust application. `DASHBOARD_ROLE=blockworks-pilot` performs at most three sequential requests and exits: accessible timeseries catalog, exact canonical asset lookup, one batched five-minute `asset-price` query for BTC/ETH/SOL. It uses the supplied external key file, follows no redirects, never logs credentials/provider error bodies, has no retry loop, bounds responses at 2 MB and limits the window to 24 hours / 288 points per asset. It refuses to overwrite an existing output.
- **The real pilot succeeded:** 288 valid candles for each asset, 864 total; zero rejected points and no null volumes. Requested provider timestamp range `[2026-11-09T04:40:00Z, 2026-11-10T04:40:00Z)`. Captured `2026-11-10T04:46:22Z`. The single collection used three API requests; other authentication diagnostics also occurred. Credit cost and actual remaining balance were not read.
- Normalized snapshot is `data/blockworks-pilot-20261110.json`, ignored by Git and mode 0600. The key remains outside the repository at the owner's supplied secret path; it was never copied into source, browser code or cloud resources.
- The normal dashboard loads `BLOCKWORKS_LOCAL_SNAPSHOT` once, accepts it only with a loopback bind, validates/projections the normalized schema and serves it through read-only `GET /api/blockworks`. No provider request, key access, collection write or file selection is possible through that HTTP route. Without a snapshot it returns `not_configured`.
- Data Lab now has a distinct Blockworks panel: BTC/ETH/SOL selection, five-minute close-price chart, valid-candle coverage, latest candle close/open change, relative volume against the preceding 12 contiguous candles, and exact OHLC/volume tables. Missing timestamps break the chart; missing volume and zero remain distinct. Source and retrieval times remain separate. JSON observation export still excludes Blockworks.
- Provider `point_schema` defines metric order; requested selection order is not assumed. Price units must be USD. Duplicate series/timestamps or ambiguous schemas fail; invalid/outside-range records are omitted, preserving gaps. Unknown/extra response fields are not exposed.
- Updated `scripts/blockworks-audit.py` with a one-request `--ping-only` diagnostic and fixed error categories, including Cloudflare failures. Its default catalog-only mode remains available and uses the same truthful application identity. No body or key is printed.

## Scope and unresolved questions

- This is a manually invoked local evaluation, not public Blockworks delivery, an automated archive or a prediction model. No recurring process spends Blockworks credits. Browser refreshes/asset switches only read the existing snapshot.
- The real schema omitted volume units, so the UI marks the volume denomination unverified. Broader documentation says USD, but the pilot does not assert that interpretation without endpoint confirmation.
- Provider timestamp start/end convention, publication latency, revisions and historical availability remain unverified. No join to Kalshi settlement windows or historical predictor-availability claim is made. The five-minute embargo is conservative, not measured latency.
- Confirm actual account rights for bulk backfills, cloud retention, public display and model use before expanding beyond local evaluation. Promotional credit expiry (shown as 9 December) is not itself proof of license termination.
- The 8,000-credit ceiling / 2,000-credit reserve remains a proposal. The pilot enforces a request/row bound, not an account credit or monetary cap. Check actual usage before another data query. Monitoring and additional dataset activation were not needed for the candle pilot.
- Other sources are unconfigured in the local Rust preview, so local Kalshi/CMC gaps are expected. Production data and the existing deployed application were not altered.

## Verification

- `/Users/shantanu/.cargo/bin/cargo fmt --check` passed.
- `/Users/shantanu/.cargo/bin/cargo clippy --offline --locked --workspace --all-targets -- -D warnings` passed.
- `/Users/shantanu/.cargo/bin/cargo test --offline --locked --workspace` passed: 50 tests (46 dashboard, four client). The first restricted run could not bind test ports; the run with local test-server access passed.
- `node --test apps/dashboard/tests/*.test.cjs` passed: 51 tests. Includes Blockworks identity/gap/zero-volume validation, relative-volume prerequisites and cancellation of all five Data Lab reads on workspace departure.
- `python3 -m unittest discover -s scripts/tests -v` passed: seven tests, including Cloudflare/error categorization, truthful client identity, no key/body disclosure, stop-on-403 and rejected redirects.
- Actual HTTP checks: unconfigured GET returned 200 / `not_configured`; configured synthetic and real snapshots returned 200 / available; POST `/api/blockworks` returned 405; `/collect-blockworks` and `/blockworks-pilot` returned 404. Configuring a snapshot with `0.0.0.0` was rejected at startup before binding.
- Real API verification: authenticated curl ping 200; corrected Python ping 200; Rust catalog/lookup/batch pilot completed. Raw API error bodies were never printed with a key present. A single public keyless Cloudflare error was inspected to identify the diagnostic problem.
- Browser inspected the actual 864-candle snapshot at CSS desktop width 1280 and mobile width 390 in light/dark themes. Asset switches, close charts and exact-value disclosure worked; document width did not overflow. A separately labeled synthetic fixture checked gaps before the real pilot succeeded; no synthetic data was published or mixed with real observations.
- `git diff --check` passed. No credentials, captures or normalized pilot data are tracked.

## Preview and next concrete step

- Actual local preview: http://127.0.0.1:3005/?view=data#lab-blockworks-title. It serves the immutable real snapshot and does not hold the Blockworks key. The process was left available for owner review. It can be restarted with the command in `docs/blockworks-data-plan.md#verified-local-pilot`.
- Real-data visual check: `/private/tmp/blockworks-data-lab-real.jpg`. Synthetic fixture and helper are temporary `/private/tmp` files only. Earlier empty/synthetic preview processes were stopped.
- Next: read the actual credit charge on the Blockworks account page; verify volume units, candle timing and permitted uses. Then choose a small settled-funding pilot or a measured backfill. Do not activate the proposed collection schedule or public deployment by inference.

## Existing public release reference

Public image remains `us-central1-docker.pkg.dev/gitunderstand/crypto-prediction-lab/dashboard:data-lab-20261109-r2`, digest `sha256:8b2d525eea3b187853d4b2d51663d94f50e121ec8194f180151ee7b672f387bd`. Build `9be3254e-d6ae-4a13-8360-bd7433b64466` succeeded. Previous publication verified stored Kalshi minute quotes for all three assets and bounded CMC history; a browser cold-load issue was resolved with fast gzip before outbound-byte accounting. The previous temporary deployment tag was removed. Resource/IAM configuration is unchanged. See project memory and Git history for the prior release's full verification record.
