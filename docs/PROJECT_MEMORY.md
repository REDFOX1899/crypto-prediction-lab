# Project memory

Last updated: 10 November 2026, America/New_York. This is the short orientation for a fresh chat. Confirm changing operational facts before acting on them.

## What we are building

Crypto Prediction Lab is a public, read-only research dashboard for people following **BTC, ETH and SOL 15-minute Kalshi contracts**. It helps users understand price, target, time, liquidity, settlement observations and broader market context. The owner is Shantanu Patil. The product is intended to become useful enough for returning users and eventual paying customers, but payments, trading and AI prediction models are not implemented.

- Repository: https://github.com/REDFOX1899/crypto-prediction-lab
- Website: https://crypto-prediction-lab-308289525742.us-central1.run.app/
- Working directory: the Git checkout attached to the Crypto Prediction Lab project. An older Desktop checkout also exists; use one active checkout at a time. The current project folder was initially empty and was populated from GitHub during session setup.
- Application release commit: `a0da6c2` (seven-day price/volume pilot and cutoff research, Data Lab only). Deployed and published to GitHub main with the owner’s explicit approval. Later documentation commits do not imply a new deployed image.

## Implemented product

**Home** (`/`): three live market cards with CF reference values, fixed contract targets, signed distance from target, countdown to the final minute, short observed-price SVG charts, YES/NO asks, visible bid-size shares and recent official outcome cells. Visitors share server-side feeds; production browser polling is every two seconds while visible and slower in background.

**Single market** (`/?view=market&asset=BTC`; also ETH/SOL): shared selector, TradingView Lightweight Charts 5.2.1 with line/one-minute candlesticks, reference and contract history where available, an always-visible order book with depth bars, recorded outcomes, and final-minute countdown/observed average/data coverage. Observations and historical candles have explicit source/availability semantics.

**Combo lab** (`/?view=combos`): two/three aligned asset selections, existing public combo books where an exact matching market exists, hypothetical payout exploration and joined outcome history. Missing quotes are legitimate: Kalshi's private RFQ-derived payouts are not generally public book quotes. The app explains this. RFQ creation/acceptance and trades are disabled.

**CoinMarketCap context**, below the homepage market watch: aggregate prices; 1h/24h/7d changes; market caps; rolling 24-hour volumes; global metrics; BTC dominance; dated Fear & Greed; sampled-price SVG history. This is a separate source, not the contract's CF settlement index. Historical samples begin with collection; no earlier ticks are fabricated.

## Quote study in the local redesign

Branch `codex/quote-outcome-desk` redesigns Home around a retrospective comparison of YES asks with official outcomes by asset, cutoff and price band. The underlying three-day study is stored in the existing named database, collection `quote_studies_v1`. An operator batch writes immutable normalized asset-day partitions plus a compact, conditional latest manifest. Labels are stored separately from pre-cutoff quote fields. Only the aggregate is served by the new read-only dashboard route. This release has no recurring study job. The public Cloud Run revision still serves the earlier Home. [Metric definitions and operator workflow](quote-study.md)

## Data Lab release

Branch `codex/data-lab` adds a fourth Data Lab workspace, aligned outcomes and coverage, shared CMC comparisons, source/sample inspection and observation export. New read-only routes normalize archived minute quotes and project up to 24 hours of existing CMC history. The owner authorized publication, and this release is now on GitHub main and deployed. Public responses use fast gzip before outbound-byte accounting to prevent large page assets being crowded out by polling; existing bandwidth/resource limits remain. Real archived minute quotes and the bounded CMC history route passed public checks. The current handoff has verification details.

Blockworks authentication is verified. The owner explicitly confirmed public-display permission on 10 November 2026 and requested production publication of the price/volume pilot and “Before this outcome” inspector, limited to Data Lab. A one-time seven-day batch returned 6,048 valid five-minute BTC/ETH/SOL candles. Normalized observations were atomically saved as three immutable asset documents plus a manifest in `blockworks_pilots`, using the existing named database; no API key or new cloud identity is needed by the dashboard. Public display requires an explicit approval flag and the recorded manifest confirmation; it reads a ten-minute shared cache, shows at most 24 hours in the chart and never invokes collection. Public revision `crypto-prediction-lab-00019-fab` serves this release. No new schedule or always-on worker is authorized or enabled.

Before-outcome research filters completed intervals to open/+5/+10/+14 minute cutoffs. Retrospective Blockworks/Kalshi downloads are distinguished from CMC collection timestamps; missing CF reference and historical book data are explicit. Outcomes, expiration values and hindsight entries are excluded from feature calculations. This is not a point-in-time training dataset or model. Credit charges, volume denomination and provider timestamp convention/publication delay remain unverified; model-use and long-term retention rights remain separate questions. See `before-outcome.md` for source semantics and `blockworks-data-plan.md` for the earlier pilot. Coinbase remains on hold.

## Stack and source map

- Cargo workspace: `apps/dashboard`, `apps/market-cli`, `crates/kalshi-client`.
- Axum/Tokio/Reqwest backend. Main routes/role selection: `apps/dashboard/src/main.rs`.
- Asset allowlist: `assets.rs`. Kalshi stream/book/reference state: `stream.rs`, `order_book.rs`, `reference.rs`. Final-minute observations: `final_minute.rs`. Chart history: `history.rs`. Public response compression: `delivery.rs`. Durable settled outcomes: `outcomes.rs`. Combo matching: `combos.rs`. CMC collection/quota normalization: `context.rs`.
- Frontend: `apps/dashboard/public/index.html`, `homepage.js`, `home-visuals.js`, `feed-client.js`, `market-context.js`, `theme.js`, `focus.css` and the existing workspace modules. No frontend framework is required.
- Source priority: Kalshi for rules/targets/books/official results; CF publications via Kalshi for reference observations; CMC for broad aggregate context. An exchange price or CMC quote cannot substitute for the settlement reference.

## Production and data

Google Cloud project `gitunderstand`, number `308289525742`, region `us-central1`. This is a shared project; do not modify unrelated services or project-wide billing/IAM.

- Public service `crypto-prediction-lab`: revision `crypto-prediction-lab-00019-fab` at the last release. Dashboard identity has database viewer access, and retains only its Kalshi secret access.
- Private service `crypto-history-collector`: revision `crypto-history-collector-00005-msp`. Collector identity writes only the dedicated database and reads the dedicated CMC key. No Kalshi trading credentials are added.
- Public image tag `before-outcome-20261110-r1`; successful build `9160cf3d-fcd2-4550-9c2f-c14a3a468aa6`, digest `sha256:776f5d3005205291a53c5d2e3a3b517b18353042329d321842012ae01c6e9ab8`. The private collector remains on its previous CMC release.
- Both services retain request-based CPU, first-generation execution, 0.25 vCPU, 256 MiB, concurrency one, min zero, max one and startup CPU boost off. Dashboard timeout 30 seconds; collector timeout 60 seconds. These settings do not guarantee a particular latency or exact monthly bill.
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
