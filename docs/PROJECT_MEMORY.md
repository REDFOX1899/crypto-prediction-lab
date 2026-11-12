# Project memory

Last updated: 11 November 2026, America/New_York. This is the short orientation for a fresh chat. Confirm changing operational facts before acting on them.

## What we are building

Yes No Tape is the public name of Crypto Prediction Lab. The read-only dashboard covers BTC, ETH and SOL 15-minute Kalshi contracts and open price thresholds settling at the next hourly close. Hourly snapshots are fetched on demand, kept separate from 15-minute history, and do not add a collector. It helps users understand price, target, time, liquidity, settlement observations and broader market context. The owner is Shantanu Patil. The product is intended to become useful enough for returning users and eventual paying customers, but payments, trading and external AI inference are not implemented. A private numerical forecasting pilot is now available; it has no validated market edge.

- Repository: https://github.com/REDFOX1899/crypto-prediction-lab
- Website: https://yesnotape.com/ (also https://www.yesnotape.com/). Both custom hostnames passed HTTPS, application and API checks on 11 November 2026.
- Custom domain: `yesnotape.com`, purchased by the owner at IONOS and connected on 11 November 2026 to the existing public `crypto-prediction-lab` service through Cloud Run domain mapping in `us-central1`. This mapping is a Google preview feature. IONOS remains authoritative DNS; Google manages HTTPS. Check the latest handoff for activation status.
- Working directory: the Git checkout attached to the Crypto Prediction Lab project. An older Desktop checkout also exists; use one active checkout at a time. The current project folder was initially empty and was populated from GitHub during session setup.
- Application commit `fdaf488` adds native hourly quote sizes, contract volume and open interest. Commit `c24061b` adds the live quote tape, technical charts and forecast study. Prior release `ea7bea4` publishes scorecards and private paper replay. Earlier `bb5e8b5` added the final-minute threshold. Later documentation commits do not imply a new deployed image.

## Implemented product

**Home** (`/`): three live market cards with CF reference values, fixed contract targets, signed distance from target, countdown to the final minute, short observed-price SVG charts, YES/NO asks, visible bid-size shares and recent official outcome cells. Visitors share server-side feeds. Visible tabs poll every two seconds and background tabs slow down. Page startup has separate request and transfer allowances. Home requests a twelve-result projection from the shared outcomes archive cache.

**Market research visuals.** Home shows 15-minute settlement mix and a short, receipt-timestamped YES quote tape. The hourly view shows price ranges and selected-strike liquidity. Data Lab adds CMC price bands, simple rolling RSI and realized volatility, with an observation-time scrubber. The historical opening-model calibration chart uses published aggregates for the model, midpoint and 50% baseline. No live external model scores are displayed. The offline Gemini rehearsal uses frozen opening projections and blinded hourly strike inputs, keeps labels separate, and measures 1,000 candidate feature slots with explicit gaps. [Protocol, evidence and commands](gemini-decisions-research.md).

**Single market** (`/?view=market&asset=BTC`; also ETH/SOL): shared selector, TradingView Lightweight Charts 5.2.1 with line/one-minute candlesticks, reference and contract history where available, an always-visible order book with depth bars, recorded outcomes, and final-minute countdown/observed average/data coverage. Observations and historical candles have explicit source/availability semantics.

**Combo lab** (`/?view=combos`): two/three aligned asset selections, existing public combo books where an exact matching market exists, hypothetical payout exploration and joined outcome history. Missing quotes are legitimate: Kalshi's private RFQ-derived payouts are not generally public book quotes. The app explains this. RFQ creation/acceptance and trades are disabled.

**CoinMarketCap context**, below the homepage market watch: aggregate prices; 1h/24h/7d changes; market caps; rolling 24-hour volumes; global metrics; BTC dominance; dated Fear & Greed; sampled-price SVG history. This is a separate source, not the contract's CF settlement index. Historical samples begin with collection; no earlier ticks are fabricated.

The Single market final-minute panel includes a required remaining reference average based on the unrounded sum of complete observed source seconds. It suppresses values on gaps, stale data, invalid targets and close, and rounds the displayed threshold upward. This describes the observation model and preserves the distinction from official settlement sampling and rounding. See `final-minute.md`.

## Market replay and scheduled checkpoint

Home, Single market results and Data Lab offer a replay dialog with play/pause and minute 0–15 scrubbing. It reveals completed archived YES ask candles incrementally, preserves gaps and shows the official result at minute 15. Prospective minute-10 books appear only after their actual receipt time. Read `market-replay.md` for the timing contract.

The owner authorized recurring minute-10 snapshots. The existing private collector receives IAM-authenticated `POST /collect-minute-ten` from job `crypto-minute-ten` at UTC 10/25/40/55 minutes. It stores immutable first-attempt records by asset and window in named Firestore collection `minute_ten_v1`, including request/receipt times, delay, target and displayed YES/NO bids and quantities. Failed, late and missing captures are explicit. Collection only starts during minute 10 and has an internal 50-second deadline. Public readers cannot initiate it. The baseline plan is 288 small records/day with no new always-on worker or provider subscription.

## Opening and follow-up research captures

The owner authorized recurring capture and the later timing repair. Private `POST /collect-opening` is triggered in the minute before each quarter by `crypto-cycle-checkpoints`, using bounded early retries and an in-request wait until +5s. Separate `crypto-cycle-followup` calls private `POST /collect-followup` at quarter-hour minutes and waits until +45s. Both retain the existing invocation identity. Opening source work ends by +28s with receipt eligibility before +30s and decision commitment before +40s. Exact-window discovery and direct metadata refresh bypass the stale open-market list and wait for a positive target and usable two-sided depth. Stable first-attempt source IDs remain in `cycle_snapshots_v1`. First working trigger metadata is recorded in `cycle_dispatch_v1`. The legacy private `/collect-cycle` manual route remains available.

## Private decision runner

Ticket C enables `market_midpoint_v1` after opening capture and links its committed outputs to the later execution candidates. The deterministic numerical benchmark uses an allowlist packet with exact source identity, ordered component times, immutable first-write version, freshness/rule/book checks and SHA-256 hash. A snapshot must be durably available by the host-selected cutoff. Missing/late/invalid inputs abstain. Official outcomes and later books cannot change model inputs. Durable object creation before +40s is the commitment. Candidate requests must follow commitment plus a one-second routing allowance and match the ticker. Links describe timing-eligible evidence for a future simulator. A separate private MCP tool now provides conditional fee-aware price previews. Verified order eligibility, simulated fills and validated predictive edge remain future work.

Private runs are stored in the app-only GCS bucket `crypto-prediction-lab-research-308289525742`, with uniform access, public access prevention and bucket-scoped collector object-creator/viewer permissions. The dashboard receives no bucket role. `DECISION_BUCKET` enables the runner only on the private collector. Private `POST /run-decisions` resumes current/recent cycles under server-selected cutoffs, and `GET /research/decision` is the authenticated operator review surface. A separate private MCP server now serves host-assigned frozen packets and bounded book-depth costs; no external inference provider is configured. See `private-decision-runner.md` for contracts and limits. Scheduled delivery can exceed the opening gate; those misses remain explicit abstentions.

## Private research MCP

Ticket E adds a separate IAM-protected `crypto-research-mcp` service at `/mcp`, using the pinned official Rust SDK, stateless Streamable HTTP and local stdio. The collector-only `POST /research/mcp-scope` issues a ten-minute capability for one existing timely decision packet, reconstructed and hash-checked at its original cutoff. Agents can call `get_cycle_packet`, `get_book_depth`, `get_market_rules`, `get_forecast`, conditional `preview_paper_levels` and versioned methodology resources; asset/run/time/phase overrides, outcomes, review, performance and writes are denied. The fee-aware preview uses a frozen assumed general taker policy, fixed minimum edge/residual allowance, and a bounded decimal tick search. The book packet stays unchanged. New prospective openings also commit a separate verified exchange fee/tick proof; old decisions cannot acquire later rules. The original v1 price calculator still uses its frozen assumptions and has no calibrated bounds, so its previews remain conditional with null eligible order prices. It supports the observed-midpoint benchmark or explicit hypothetical bounds; it never promotes these into committed forecasts. There is no external model invocation, forecast submission, intent or fill in this adapter. See `fee-aware-paper-levels.md`.

The dedicated `crypto-research-mcp` identity has only a conditional reader grant to `mcp/scopes/v1/` in the private research bucket, with no project role, database, collector invocation or secret grant. Capabilities are random, stored by hash and carried by the trusted host in a header alongside service IAM authentication. Public visitors cannot discover tools or issue scopes. This is a first-party IAM profile; customer OAuth onboarding is future work. Per-process call limits reset on restart, while fixed cutoff and expiry persist. New service limits remain request-based min zero/max one, 0.25 vCPU/256 MiB; no recurring model calls, collection job or paid provider was added. Read `private-research-mcp.md` before connecting agents.

## Numerical forecasting and calibration

Original Ticket F adds `opening_logistic_sigmoid_v1`: a deterministic pooled regularized logistic model with asset intercept effects and a disjoint later sigmoid calibration fit. It uses only validated immutable opening projections (midpoint, spread, time remaining and native top-ten size imbalance). Official label versions are separately joined and conservatively available at max(current Firestore updateTime, captured_at). Fixed v1 training/calibration/test blocks with 30-minute gaps preserve all assets within cycles; normalization and calibration use only earlier labels available by their respective fit cutoffs. The two-date pilot does not meet whole-day validation and does not beat the pooled midpoint on its 54 held-out observations.

Frozen datasets, evaluations and linking experiment manifests are separately archived create-only in the existing private bucket under `forecast_datasets/v1/`, `forecast_evaluations/v1/` and `forecast_experiments/v1/`; they stay operator/evaluator-only. Models are create-only private GCS objects under `forecast_models/v1/<hash>.json`, selected on the collector by `FORECAST_MODEL_OBJECT` and preloaded once per process. Authoritative model creation must precede the original packet cutoff. New decisions independently freeze `forecast_proof`/`forecast_hash`; the original midpoint strategy and packet remain unchanged. Private MCP `get_forecast({})` serves only that saved proof's probabilities/baselines/support, with no raw fitting records or outcomes. Old decisions remain unavailable; scopes cannot attach a future model. The MCP identity still reads only its scope prefix. Calibration is diagnostic, probability intervals are null and paper order eligibility stays false. No automatic retraining, extra schedule, new paid provider or resource-limit increase. Read [forecasting-calibration.md](forecasting-calibration.md) for fixed mathematics, first results, reproduction and limits.

## Prospective evaluation (J1)

J1 adds on-demand private daily/summary evaluation of already committed numerical forecasts. A content-addressed experiment fixes model/generation, baselines, metrics, day grouping and support before a 12 November 2026 UTC confirmatory start. Earlier 11 November live forecasts are engineering checks. All completed scheduled rows stay in coverage; only validated saved forecasts and available exact official versions score. Future objects/label versions are excluded before hashing, and missing current outcomes never inherit a prior report's result. Immutable private input shards, report revisions and predecessor references preserve observed corrections. The collector also invalidates binary labels on a settled non-binary correction and detects result changes independent of provider update time.

Daily reports and summaries live under `prospective/` in the existing private research bucket, outside the MCP identity's scope-only grant. Only the existing collector gets operator evaluation routes; no evaluation/performance/outcome tool enters MCP. Reports never fit/recalibrate models or attach predictions to old decisions. Thirty full confirmatory UTC days with >=95% forecast and scored coverage per asset are needed for a descriptive day-block uncertainty interval; it is a loss-difference diagnostic, not a YES probability bound. Eligibility remains false. Bounded on-demand work yields to capture windows; no report scheduler, paid provider, always-on process or resource increase is added. Read [prospective-evaluation.md](prospective-evaluation.md) and the latest handoff for operation and actual deployment evidence.

## Paper research scorecards

Published in application commit `ea7bea4`. Single market computes a two-sided break-even map from one recent book and explicit visitor assumptions. Data Lab compares contiguous official outcomes and can display an operator-selected paper report entirely within the browser. The private scope adds a verified-rule one-contract point-estimate scorecard. Experimental IOC simulation and economic comparison run as offline operator jobs. The frozen J1 study and its private labels stay separate from cutoff-safe MCP. The conservative policy passes until probability bounds have independent support. Optional GCS registration/archive uses the dedicated private bucket. See [paper-scorecard.md](paper-scorecard.md) and the current handoff for implementation and publication state.

## Verified exchange fee and tick evidence

Opening capture now fetches current series fee type/multiplier, announced series changes and complete event-filtered override history after a usable book. Requests run in parallel once per opening asset within a two-second/remaining-work bound. Actual request/receipt times and allowlisted market tick bands/notional are saved in the same immutable Firestore opening payload as `observation.market_rules`. No separate Firestore documents or schedule were added; at most nine extra public requests/cycle. Slow rule sources preserve the usable book and leave explicit unavailable evidence.

The runner projects rules from the same immutable source at its original cutoff and freezes `market_rules`/`rules_hash` in the existing private decision. Projection checks source availability, identity, full dynamic grids, book/complement prices, effective event overrides and fee transitions. Future-effective rows are removed. The private MCP returns only this committed proof and hash; late/tampered proofs are denied and old missing evidence stays missing. Account discounts, member rounding profile, calibration and fills remain unverified. See `verified-market-rules.md`. The first automatic 15:15 UTC cycle on 11 November saved verified rules for all three assets before cutoff and preserved them through duplicate runs.

## Public quote and outcome study

Home opens with a retrospective comparison of YES asks with official outcomes by asset, cutoff and price band. The underlying three-day study is stored in the existing named database, collection `quote_studies_v1`. An operator batch writes immutable normalized asset-day partitions plus a compact, conditional latest manifest. Labels are stored separately from pre-cutoff quote fields. Only the aggregate is served by the new read-only dashboard route. This release has no recurring study job. [Metric definitions and operator workflow](quote-study.md)

## Data Lab release

Branch `codex/data-lab` adds a fourth Data Lab workspace, aligned outcomes and coverage, shared CMC comparisons, source/sample inspection and observation export. New read-only routes normalize archived minute quotes and project up to 24 hours of existing CMC history. The owner authorized publication, and this release is now on GitHub main and deployed. Public responses use fast gzip before outbound-byte accounting to prevent large page assets being crowded out by polling; existing bandwidth/resource limits remain. Real archived minute quotes and the bounded CMC history route passed public checks. The current handoff has verification details.

Blockworks authentication is verified. The owner explicitly confirmed public-display permission on 10 November 2026 and requested production publication of the price/volume pilot and “Before this outcome” inspector, limited to Data Lab. A one-time seven-day batch returned 6,048 valid five-minute BTC/ETH/SOL candles. Normalized observations were atomically saved as three immutable asset documents plus a manifest in `blockworks_pilots`, using the existing named database; no API key or new cloud identity is needed by the dashboard. Public display requires an explicit approval flag and the recorded manifest confirmation; it reads a ten-minute shared cache, shows at most 24 hours in the chart and never invokes collection. This Data Lab pilot first shipped in revision `crypto-prediction-lab-00019-fab`. No new schedule or always-on worker is authorized or enabled.

Before-outcome research filters completed intervals to open/+5/+10/+14 minute cutoffs. Retrospective Blockworks/Kalshi downloads are distinguished from CMC collection timestamps; missing CF reference and historical book data are explicit. Outcomes, expiration values and hindsight entries are excluded from feature calculations. This is not a point-in-time training dataset or model. Credit charges, volume denomination and provider timestamp convention/publication delay remain unverified; model-use and long-term retention rights remain separate questions. See `before-outcome.md` for source semantics and `blockworks-data-plan.md` for the earlier pilot. Coinbase remains on hold.

## Stack and source map

- Cargo workspace: `apps/dashboard`, `apps/market-cli`, `crates/kalshi-client`.
- Axum/Tokio/Reqwest backend. Main routes/role selection: `apps/dashboard/src/main.rs`.
- Asset allowlist: `assets.rs`. Kalshi stream/book/reference state: `stream.rs`, `order_book.rs`, `reference.rs`. Final-minute observations: `final_minute.rs`. Chart history: `history.rs`. Public response compression: `delivery.rs`. Durable settled outcomes: `outcomes.rs`. Combo matching: `combos.rs`. CMC collection/quota normalization: `context.rs`.
- Frontend: `apps/dashboard/public/index.html`, `homepage.js`, `home-visuals.js`, `feed-client.js`, `market-context.js`, `theme.js`, `focus.css` and the existing workspace modules. No frontend framework is required.
- Source priority: Kalshi for rules/targets/books/official results; CF publications via Kalshi for reference observations; CMC for broad aggregate context. An exchange price or CMC quote cannot substitute for the settlement reference.

## Production and data

Google Cloud project `gitunderstand`, number `308289525742`, region `us-central1`. This is a shared project; do not modify unrelated services or project-wide billing/IAM.

- Public service `crypto-prediction-lab`: revision `crypto-prediction-lab-00021-rh7` at the last release. Dashboard identity has database viewer access, and retains only its Kalshi secret access.
- Private service `crypto-history-collector`: revision `crypto-history-collector-00016-j4q`. Collector identity writes the dedicated database and app-private research bucket and reads the dedicated CMC key. No Kalshi trading credentials are added.
- Public dashboard and private MCP use scorecard build `431f48bb-9a26-4683-a57b-ebbc0abcf41f`, digest `sha256:9bb72de8e0db1e1fdde2f5bf96c517c49989848a6363dcb5af921762ecf4b1d8`, source `ea7bea4`. The verified public traffic revision is `crypto-prediction-lab-00021-rh7`; revision numbers alone do not identify release order.
- Private MCP revision `crypto-research-mcp-00005-n75` serves the scorecard image. Collector remains on J1 build `8efae977-78db-45bf-98c0-eff68d7ff6b7`, digest `sha256:b6268f7567cf305c74959cdb4a88c79bfb2612ba685e37e8a20ba71b7e689bbf`, source `a2f308b`, revision `crypto-history-collector-00016-j4q`. Private reports are refreshed on demand.
- Dashboard and collector retain request-based CPU, first-generation execution, 0.25 vCPU, 256 MiB, concurrency one, min zero, max one and startup CPU boost off. Dashboard timeout 30 seconds; collector timeout 60 seconds. These settings do not guarantee a particular latency or exact monthly bill.
- Named Firestore database `crypto-prediction-lab`. Default database is unrelated. `contracts_btc`, `contracts_eth`, `contracts_sol` retain settled market records/minute candles and summaries. `cmc_context/latest` holds normalized current context; `cmc_samples` holds combined three-asset sampled observations.
- Existing private history schedule: `crypto-history-quarter-hour`, minutes 2/17/32/47 UTC. New private CMC schedule: `crypto-market-context`, every five minutes. Both use the existing dedicated scheduler identity and service-level invocation rights.
- CMC key is only in the private collector's Secret Manager mount. No key is needed by the browser or public dashboard. Do not retrieve secret values just to orient yourself.

CMC Basic was verified as 15,000 monthly credits and 50 requests/minute. Three asset quotes share one credit/request every five minutes; global metrics are hourly, sentiment daily. Expected 30-day baseline is 9,390 credits. Check actual usage; pause at the lower of 12,000 credits or 90% of allowance. No automatic paid upgrade. API success codes can be numeric `0` or text `"0"`; both must be accepted. Source and collection timestamps remain separate.

The app has a $40 monthly budget alert, not a hard cap. Preserve resource/egress/request controls. CMC is on its free plan; GCP builds, secrets, scheduling and database usage still incur applicable charges.

## Not implemented / current boundaries

- Coinbase is explicitly **on hold**; public feed connectivity was tested, but no integration or new Coinbase key was added.
- Direct CF API access is not configured. A trial/credits/two-package quote inquiry was emailed to CF licensing; do not assume an agreement or reply exists.
- No continuous, loss-aware all-day CF/Kalshi order-book archive. Live UI state is bounded and coalesced; settlement archives and CMC snapshots are not full tick replay.
- CMC observations are not yet joined onto individual settled-contract summaries. Current chart returns are broader periods, not 15-minute prediction probabilities.
- No accounts/private-login requirement, custom domain purchase, payment system, execution adapter, Nautilus integration or validated predictive model.

## Where to look next

- Cutoff research and saved Blockworks display: `before-outcome.md`.
- Latest handoff and pending work: `SESSION_HANDOFF.md`.
- Blockworks access findings, credit plan and proposed archive: `blockworks-data-plan.md`.
- Suggested, unapproved next features: `roadmap.md`.
- CMC schema, quotas and release checks: `coinmarketcap-context.md`.
- UI semantics: `focus-ui-design.md`, `final-minute.md`, `combo-lab.md`.
- Storage and deployment: `history-archive.md`, `cost-controls.md`, `public-deployment.md`.
- Future provider/storage architecture: `data-provider-engineering-plan.md` (a proposal, not an implemented system).
- `production-pilot.md` is an earlier private-pilot proposal; its pending-project/login statements are historical, not current instructions.
