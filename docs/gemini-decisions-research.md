# Gemini Decisions research plan for Yes No Tape

Proposed experiment, 11 November 2026. Gemini is a private forecasting challenger. The public dashboard currently displays observed markets and separately dated historical model results. The first pilot asks whether the model adds information beyond the live market midpoint for a 15-minute opening decision. Hourly strikes need a separate experiment and point-in-time archive.

## What 1,000 means

Google accepts one shared input with up to 1,000 independently scored questions. Its reported median is 50–100 ms model compute and 150–250 ms end to end in its benchmark. The published near-flat timing covers 1–100 questions and contexts up to 16K tokens. The maximum input and question limits do not establish latency on our deployment. The shared input is charged once per request and each question adds its own input tokens. At launch the price is $0.10 per million input tokens. Use the Gemini API version and hosting location actually available to the project. [Google launch and API documentation links](https://blog.google/innovation-and-ai/technology/developers-tools/gemini-decisions-api/)

Use three distinct counts in every experiment:

- A feature is a measured value with a unit and an as-of time. The catalog below defines 1,000 *candidate* measurements per asset and cutoff. Many require future collection.
- A question defines a future observable event. Most market decisions need only a handful. Correlated answers cannot be added as independent votes.
- An example is one committed contract forecast with a later official outcome. Aim for 1,000 eligible examples to test plumbing. Three assets at 96 openings/day yield at most 288 scheduled examples/day before misses. Shared expiries and overlapping hourly strikes reduce effective independent sample size. Separately count cycles, trading days and missing opportunities.

## A concrete catalog of 1,000 candidate measurements

Each key is a stream below × one trailing window (5, 10, 20, 30, 60, 120, 300 or 600 seconds) × one statistic (time-weighted mean, time-weighted standard deviation, minimum, maximum or last minus first). This gives 25 × 8 × 5. For example, `yes_spread_cents.30s.mean` is the average cost of crossing the YES book over the prior half minute. `ref_gap_bps.120s.delta` measures whether the reference moved toward or away from the strike. The catalog is a research menu. Drop redundant fields using training data alone.

| Family | Streams and units | Collection needed |
|---|---|---|
| Binary quotes (5) | YES bid ¢, YES ask ¢, YES midpoint ¢, YES spread ¢, microprice ¢ weighted by opposite-side displayed top size | Existing opening packets support point values. A recent series of consistent books is needed for the windows. |
| Displayed liquidity (8) | YES bid size and NO bid size at best (2), cumulative native YES/NO quantities over top 3 levels (2), top 10 (2), top-level imbalance and top-10 imbalance (2). Sizes in contracts, imbalances in signed fractions | Full depth with matching provider, ticker, sequence and receipt time. Top sizes describe resting interest. |
| Reference and contract (5) | CF reference USD, signed reference-minus-target bps, absolute gap bps, reference change from first eligible observation in window bps (fixed contract opening anchor), observed final-minute average minus target bps | Entitled CF observations via Kalshi with source timestamps. The final-minute stream is undefined before its averaging window. Require a known opening reference for the opening-anchored change. |
| Cross-asset context (3) | BTC, ETH and SOL trailing one-minute reference log returns in bps | Three aligned, same-source point-in-time series with both endpoints. No substitution of CMC spot for CF. |
| Book dynamics (4) | Last successive YES midpoint change ¢, last successive YES spread change ¢, last successive change to YES top-10 depth, last successive change to NO top-10 depth | Ordered consecutive valid book updates. Depth change alone cannot distinguish cancellations from trades. |

Calculate windows on a one-second as-of grid, using only observations with source and receipt times at or before the grid instant and durable availability before the forecast cutoff. Define side conversion and tick precision from the contract rules. For an opening-anchored measure the first eligible opening observation stays fixed. Require complete coverage under a predeclared per-source age tolerance, minimum two distinct fresh observations and no crossed quotes. Preserve zeros as measurements and nulls with reason codes. Never carry a contract book across a ticker change. Histories shorter than the lookback, unavailable final-minute inputs and future timestamps produce nulls. Track coverage, newest source age and distinct observation count alongside each feature. These quality fields are outside the 1,000 feature count.

Start with the already frozen point inputs: YES midpoint, spread, top-ten depth imbalance and time to expiry, plus asset and contract rule identity. Compare the market midpoint alone against that packet with and without Gemini. When prospective reference samples exist, test strike distance, reference momentum and realized volatility. Prefer distance divided by plausible remaining movement to an arbitrary RSI cutoff. Test technical-analysis additions as separate challengers and record the exact source, lookback, missing-data rule and calibration set. For RSI or moving-average crossovers specify sampling interval and warm-up; a 14-period indicator on a 15-minute contract can require price history before the contract existed. Earlier underlying reference prices and earlier contract quotes are different series.

## Questions with labels we can audit

| Shared evidence and question | Allowed output | How to grade later |
|---|---|---|
| Given the exact contract rule and frozen opening packet, will this contract officially settle YES? | YES / NO score | Exact Kalshi official label version matched to ticker. Primary question. |
| At a fixed +45-second checkpoint will the YES ask exceed the opening YES ask by at least 2¢? | YES / NO score | Eligible later two-sided book. Missing or late capture is unscored. |
| At that checkpoint will the YES spread exceed 3¢? | YES / NO score | Recompute from the later book. Diagnostic for execution conditions. |
| Will the best displayed YES-side offer support at least five contracts at the checkpoint? | YES / NO score | Match native NO bid quantity at the complementary price. This measures displayed size. |
| Which of five disjoint price bands contains the official hourly expiration value? | Choice over five bands | One asset, one expiry, identical settlement rules, thresholds fixed at cutoff. Derive nested YES probabilities from the one distribution. Include open tails and define equality. |
| Which reference-to-target distance band will contain the next observed average at a declared time? | Choice over disjoint signed-distance bands | Only after the required prospective reference capture exists. |

For offline stress testing the 1,000-question interface, use 20 real same-expiry strikes × 10 future checkpoints (30s to 300s in 30s steps) × five binary targets: YES ask up at least 2¢, YES bid down at least 2¢, spread over 3¢, best offer size at least five, midpoint above its opening value. Each prompt names one contract and one horizon. Admit only checkpoints before close. Unsupported captures stay unevaluated. Reusing strikes, future checkpoints and shared inputs creates dependence. Use this grid for timing and coverage experiments; predeclare a small primary set separately.

Do arithmetic, staleness checks, order eligibility and fee calculations in deterministic code. A model confidence or rubric score is not a market win frequency. Freeze prompt text, option ordering, input units, model ID, call settings, timeout, source manifest and packet hash. Use only as-of data. In retrospective probes, redact calendar dates, tickers that encode dates and identifiable absolute prices where possible to reduce memorization. Live prospectively committed tests are the primary evidence.

## Storage and latency

Use the existing application-only named Firestore database for bounded operational records and the existing private research bucket for immutable experiment artifacts. A future BigQuery dataset can index append-only features and settled labels for large offline scans once a concrete query cannot fit the existing pipeline. A database per model would add operational cost without repairing absent observations.

For a future bounded pilot, persist four logically separate records: feature packet, committed model output, subsequent book and final official label/corrections. Give each asset/ticker/expiry/cutoff/model/prompt combination an idempotent identity. Preserve original provider times, first receipt, durable availability, request start, response receipt and commitment. Freeze the packet first, send allowlisted fields to the model from the private host, and grade against later versioned labels in the evaluator. Keep private captures and credentials outside public assets. Confirm the provider transfer terms, data rights, daily token cap and retained fields before enabling calls. The $40 monthly project alert remains a target.

Measure local extraction, queue, API round trip and durable commit separately with a monotonic clock. Report p50/p95/p99 total latency, timeouts, end-of-window misses, coverage and effective cost per timely score. Replay comparable input sizes at 1, 10, 100 and 1,000 questions; also compare one pooled request versus independent per-asset packets to expose cross-asset dependencies. Sample cold and warm starts. Price formula is input tokens × $0.10 / 1,000,000 at launch. Fully freeze deadlines. A late result expires; the evaluator records the miss. A fresh quote and actual available size must be rechecked at the hypothetical paper entry.

## Display and promotion

The private pilot card should lead with the named contract, exact strike and expiry; separate raw model score, calibrated YES frequency, market midpoint, current executable asks and after-cost break-even. The headline can read “Pass”, “YES paper candidate”, “NO paper candidate” or “Insufficient data”. Include a reason and source ages. When a lower uncertainty bound for YES exceeds YES ask plus verified costs, it can clear the YES paper hurdle. Use (1 minus the upper YES bound) for NO. Check fees, size, slippage allowance and timing in both directions. Example only: estimated YES 58%, YES ask 54¢, costs 2¢ -> point margin +2¢. If a supported lower bound is 51%, the decision is Pass. A confidence score returned by Gemini cannot supply that bound. Reuse this card separately for each asset; rank only eligible paper candidates on the same after-cost convention.

Before promotion, register an independent prospective Gemini experiment, separate from the existing J1 protocol. Keep all same-expiry strikes and BTC/ETH/SOL cycles together in chronological training, calibration and later evaluation blocks; embargo overlapping label horizons. Compare Gemini, market midpoint, constant 50%, existing logistic and a frozen blend on identical timestamps. Report paired Brier loss, log loss, reliability bins with support, coverage and after-cost paper outcomes with later books and no-fills. Compare a sample-matched baseline *and* report the opportunities the challenger missed. Use day-block uncertainty to account for dependence and predeclare feature ablations. The current baseline protocol uses 30 complete UTC days and >=95% coverage per asset for descriptive intervals. A thousand packets or a better point estimate alone does not justify ranking bets.

The first ship target is a private shadow run for the settlement question with zero execution authority. First acceptance evidence: reproducible cutoff-safe packet, known label blinding, successful abstention on stale inputs, measured latency and a paired comparison. The public site can display dated historical calibration and sample sizes while evidence accumulates.

## Implemented local rehearsal

The Rust role `gemini-rehearsal` prepares one binary settlement question per eligible committed 15-minute opening. It reuses the opening packet validator and the fitter’s decision identity check. It excludes official labels, future observations, absolute timestamps, raw tickers and target prices from the provider request. The local NDJSON envelope separately retains evaluation provenance. Only the nested `request` object is a candidate provider body. Written files are private and create-only. The role does not contain an API client.

```sh
DASHBOARD_ROLE=gemini-rehearsal FORECAST_INPUT=/private/path/frozen-extract.json \
  GEMINI_REHEARSAL_OUTPUT=/private/path/opening-requests.ndjson target/debug/dashboard
python3 scripts/gemini_research.py catalog --output /private/path/feature-catalog.json
```

The hourly tool emits either one primary settlement question or a checkpoint stress request. It can assemble a joint request using real strikes across three assets, with one shared cutoff, asset-specific quote ages and a common expiry. The selection rotates across assets from the most balanced two-sided offers. The rehearsal’s 1,000-question request used 20 actual strikes. Each asset’s central floor serves as its own offset origin. Dates and contract identifiers stay outside the provider body. Checkpoints at or after expiry are excluded. The grading tool chooses the nearest later snapshot within five seconds of each checkpoint and rejects snapshots retrieved over 15 seconds after their server timestamp. Paired quote labels require a valid two-sided book. The hourly response does not include size, so the offer-size question remains ungraded.

```sh
python3 scripts/capture_research_quotes.py --mode hourly --output-dir /private/path/hourly
python3 scripts/gemini_research.py portfolio --btc /private/path/hourly/BTC-00.json \
  --eth /private/path/hourly/ETH-00.json --sol /private/path/hourly/SOL-00.json \
  --output /private/path/hourly-request.json
python3 scripts/gemini_research.py replay-portfolio --checkpoint-dir /private/path/hourly \
  --output /private/path/label-coverage.json
```

The book recorder takes one bounded sample of public REST order books and writes a post-flush availability watermark per file. A full-window example takes 156 observations per asset four seconds apart, starting in the next quarter-hour. It stops after that run. `market_feature_windows.py` emits all 1,000 slots with counts and null reasons. Its book-only schema supports 17 streams. The eight reference streams need separate entitled, timestamped inputs. The rehearsal uses the receipt clock because exchange publication time is unavailable. Each second must be at or after both receipt and durable availability, and within six seconds of the relevant book. Derived changes must wait for both of their source observations. Locked, crossed, unavailable and expired books leave missing values.

```sh
python3 scripts/capture_research_quotes.py --mode books --steps 156 --interval 4 \
  --next-window --output-dir /private/path/books
python3 scripts/market_feature_windows.py --manifest /private/path/books/manifest.json \
  --asset BTC --cutoff YOUR_RECORDED_UTC_CUTOFF --output /private/path/btc-features.json
```

Use a cutoff within the exact captured contract and after the last included record was durable. The long-run example uses the first whole second following the final durable receipt of each asset. Rolling statistics require complete coverage throughout the relevant window; change features also require a previous valid book. A reported local preparation time excludes external inference, database writes and network transit.
