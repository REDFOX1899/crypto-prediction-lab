# A prospective paper market for 15-minute crypto contracts

Proposed implementation handoff, 10 November 2026. Scope covers Kalshi BTC, ETH and SOL singles. This document defines future work. The current public dashboard and collector continue to operate under their existing permissions.

## 1. Start with one question the data can answer

At the first usable snapshot after each quarter hour, can a frozen model estimate the probability of an official YES outcome more accurately than the simultaneous Kalshi midpoint? Separately, would a predeclared price-limited policy using that forecast earn positive simulated dollars after fees and realistic delay? Track both. A model can improve probability calibration while every available offer is too expensive.

Use the existing minute-10 checkpoint for the first offline rehearsal. Start prospective decisions at **open only**. Treat minute 10 as a separate strategy and evaluation cohort when added. Start with a fixed one-contract paper size, one decision per asset/window/strategy, fresh two-sided books, and hold to settlement. These are experiment defaults to freeze before the pilot. Use BTC/ETH/SOL together to measure correlated exposure. Keep size, cadence and entry policy fixed while comparing models.

**First milestone:** select any completed paper cycle and reproduce the immutable input, forecast or abstention, both price ceilings, the subsequent book, simulated fill or skip, and official settlement. Show where data went missing if any step failed.

## 2. Build the observation record and measure what exists

Start in `apps/dashboard/src/checkpoint.rs`, `outcomes.rs`, `assets.rs`, `order_book.rs`, `reference.rs` and `final_minute.rs`. Read `docs/market-replay.md`, `before-outcome.md` and `cost-controls.md`. Reuse asset, book and decimal types from the Rust workspace. Preserve all existing readers of `minute_ten_v1`. Add a versioned projection for legacy rows. Where a legacy row has only one receipt timestamp, use it conservatively for the combined observation and flag component timing as unknown.

| Priority | Capture or derive | Use and missing-data behavior |
|---|---|---|
| P0 contract | Asset, exact ticker/series, open, close, status, target, comparator, settlement rule version, supported tick grid, fee schedule and overrides effective at decision | Verify identity after rollover. Unknown rules prevent a directional price level. Unknown fees prevent an eligible paper order. |
| P0 opening book | Both native bid ladders with fixed-point prices and sizes, top ten initially. Record request start and receipt independently for book and metadata, source time/sequence if provided, source kind, stale/gap/crossed flags | Derive opposite asks by complement at the same quantity. Calculate YES midpoint, spread, top depth, depth at the proposed size and imbalance. Missing side gives an explicit unavailable midpoint. Keep market metadata and book non-atomic. [Kalshi order-book semantics](https://docs.kalshi.com/getting_started/orderbook_responses) |
| P0 execution evidence | A second book acquired after decision submission plus modeled routing delay, with request/receipt times, ticker, depth and expiry | Determines whether a paper limit could cross visible size. A minute candle or the decision snapshot cannot establish this. |
| P0 outcome | Official result and status, official expiration value if provided, first observed publication/receipt, correction version and fees effective for that market | Evaluator-only data. Pending, void and amended results have explicit ledger states. Use actual settlement treatment for voids. [Kalshi crypto settlement](https://help.kalshi.com/en/articles/13823838-crypto-markets) |
| P1 reference | If permitted, index identifier, observed CF value and age, signed distance to target in basis points, returns over complete prior intervals, realized volatility, source coverage and corrections | Supports a settlement-aware model. Check rights separately for retention, derived features, external model inference and distribution. Live visitor-dependent observations can have gaps. Launch book-only experiments if reference rights or coverage are missing. |
| P1 flow | Prospectively captured Kalshi trade IDs, price, size, direction when defined, and book updates with snapshot/sequence/gap markers | Test whether signed flow, spread change and depth replenishment improve predictions beyond price. Requires a separate volume, rights and cost review. Two snapshots cannot establish queue position. |
| P2 context | Existing CMC collection/availability times, lagged broad-market returns and aligned BTC/ETH/SOL features | Test incremental value one family at a time. Label rolling 24-hour volume by its actual horizon. Blockworks pilot display permission does not establish model/export rights. Coinbase stays on hold. |

Publish a compact feature dictionary with the packet. Suggested first definitions:

- **Distance to target (bp)** = 10,000 × (reference / target − 1). A positive sign means the reference is above the target. Include both observation times.
- **YES spread (¢)** = 100 × (derived YES ask − YES bid), using dollar prices from one coherent book. The midpoint baseline is their arithmetic mean in dollars. A crossed or one-sided book is ineligible.
- **Top-depth imbalance** = (YES bid size − derived YES ask size) / (sum of those sizes). It ranges from −1 to 1 and is unavailable at zero total size. Treat its predictive direction as an empirical question.
- **Required average gap (bp)** = 10,000 × (required remaining reference average / latest reference − 1), when the existing final-minute coverage checks pass. Keep it absent on an opening packet. It is a measure of the remaining hurdle.
- **Short-horizon volatility** = square root of the sum of squared consecutive log reference returns on a declared sampling grid over a declared trailing window. Return the grid, sample count and coverage. Test whether the feature helps at a given time-to-close before using it for probabilities.

Every input has source, units, schema, intended checkpoint, event time when supplied, receipt time, earliest time the runner could access it, provenance, quality and missing reason. Use UTC, decimal dollars for contract prices and explicit contract counts. Record visibility to the runner even if persistence trails receipt. A frozen packet may include only fields known by its cutoff. A historical candle downloaded tomorrow stays in the retrospective dataset. A later correction appends a revision with its own availability time.

**Ticket A — inventory and fixtures.** Implement a read-only coverage report on an explicitly selected date range using existing checkpoints/outcomes. Count expected slots (96 per UTC day per asset), eligible/open/settled markets, observed books, unmatched tickers, delayed receipts, missing sides and outcomes. Show observation age and receipt-delay percentiles. Build synthetic JSON fixtures covering rollover, late metadata and missing depth. Acceptance: rerunning the report gives the same totals against a frozen extract, gaps have denominators, and mutating a winner cannot change an input packet.

## 3. Run each market cycle independently of dashboard visitors

**Ticket B — bounded capture.** Generalize the private collector with an opening-checkpoint path and keep the existing minute-10 route compatible. One authenticated quarter-hour trigger discovers all three markets from the asset registry and validates actual open/close/status. Use a bounded retry for transient discovery within the deadline. A late trigger records the real delay. Never present an opening quote acquired 21 seconds late as the price at second zero.

**Ticket C — cycle runner.** Use a private scheduler to wake a trusted host. The host starts the MCP client, invokes the selected model or numerical baseline, validates its structured reply and records the deadline outcome. MCP provides the interface an agent calls. The scheduler and runner own wake-up, retries and persistence. Reuse a shared market capture for every strategy, including the no-agent baseline. Avoid a separate provider subscription per model.

Suggested first-pilot timing, measured against an actual UTC market open:

1. At +0s begin discovery. Freeze inputs when all required fields for an asset are available. Target a packet by +20s and measure the miss rate.
2. Accept the first valid decision by +40s. Save cutoff, packet-ready time, model start/end, persistence time and measured end-to-end delay. Preserve late output as diagnostic only.
3. For the initial comparison, designate a common execution observation requested at +45s for timely decisions. Freeze the decision and impose a routing-delay allowance before eligibility. A request may start only after both decision commitment and the allowance. Set a bounded receipt deadline, initially +55s. If unavailable, record an unpriced attempt. This estimates a policy that deliberately waits until +45s. A separate experiment may request the first book immediately after each model's own latency.
4. Use a separate bounded invocation or durable task when required. Return from the collector before its existing 50s work deadline and 60s service timeout. Do not sleep through a market or rely on background work after a Cloud Run response. Preflight scheduling jitter and conflicts with CMC/history jobs on the concurrency-one collector. If the measured timing is infeasible, revise the next experiment's timing explicitly.
5. Join official results only after the existing archive obtains them. Publish settlement with its actual availability time. Markets can close before the archive knows the winner.

Key the cycle by environment, actual ticker and open time, checkpoint and schema. Key each run additionally by strategy/experiment version, and each simulated order by run and intent number. Use conditional creates or transactions, expiring work leases, idempotent resume and explicit terminal statuses. Time-box retries. Recover the missing asset after a partial three-asset write. A duplicate trigger cannot rerun the model for a replacement opinion, submit a second intent or consume another paper allocation.

**Acceptance:** a fake-clock test replays early, late, duplicate, crashed and overlapping invocations. A late result or new ticker cannot consume a previous market's quote. Missing SOL leaves the other two assets usable. One actual supervised pilot records request and receipt times without a website visitor. Production scheduling is a later deployment decision.

## 4. Store packets so another model can reproduce a decision

**Ticket D — persistence.** For low-volume snapshots use the app's named Firestore database in `gitunderstand`. Proposed collections are `research_cycles_v1` (metadata/manifests), `research_packets_v1` (immutable normalized observations), and private run/intent/fill/settlement records. Keep features, later observations, and official labels in separate projections. Include input IDs, canonical serialization/hash, schema, feature code commit, fee/rule versions and provenance. A released dataset manifest identifies exact revisions. Store decimal strings or scaled integers. Set retention per source before enabling collection.

Keep private model runs and future account data inaccessible to the public dashboard identity. The existing database viewer has database-level permissions. Collection naming and browser route checks alone cannot provide identity isolation. Use local isolated storage for the first runner tests. Before remote deployment, verify actual identity boundaries and use dedicated app-private storage with separate IAM if necessary. Do not broaden shared project access. For approved high-rate event capture, batch compressed objects in a dedicated private GCS bucket with checksums, snapshot bases, sequence ranges and gap markers. Keep manifests in Firestore and use local DuckDB/Parquet for offline research. Add a warehouse only when measured queries justify it.

Publish a daily health report with expected/usable cycles, age and timing percentiles, invalid books, label lag, model cost, provider requests, bytes and projected monthly spend. With 96 cycles/day, opening and post-decision books for three assets add **576 asset-book observations/day** to the planned 288 minute-10 rows/day. That counts observations, not all database operations. Three separate model calls per cycle cost 288 calls/day/model; one batched call costs 96 if all three packets arrive in time. Compare cost and timeout coupling before choosing batching. Set hard run/token/read quotas and pause model calls on exhaustion. Produce a pilot bill estimate including provider, model and GCP costs before recurring activation. Keep the existing $40 monthly alert and resource limits.

**Acceptance:** restore a run by hash after restart. An inference identity cannot fetch labels or private account data via any exposed route. A public visitor cannot trigger capture or inference. Duplicate delivery leaves the ledger balanced.

## 5. Define the MCP contract once for small and large models

**Ticket E — read-only research MCP.** Implement a thin adapter over the same pure Rust feature service used in replay. Local development uses stdio. For remote integrations use an authenticated private Streamable HTTP endpoint, with a protocol revision and SDK supported by both ends. Test against the official Inspector and at least one real host. Kalshi already offers a connector for market exploration, portfolio reading and alerts. Our server adds frozen evidence, forecast comparison and paper replay. Do not attach a live market connector, web search or unrestricted clock/database tool to blinded replay agents. [MCP versioning](https://modelcontextprotocol.io/specification/versioning) · [MCP tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools) · [Kalshi MCP](https://docs.kalshi.com/getting_started/mcp)

| Interface | Input → output | Who can call |
|---|---|---|
| `get_cycle_packet` | Assigned run → immutable packet with asset/ticker, cutoff, rules, compact features, units, quality, evidence IDs and baseline probability if the experiment allows it | Forecast agent. The host binds run identity, cutoff and visibility policy. |
| `get_book_depth` | Packet ID + permitted side and bounded quantity → snapshot price ladder, quantity-weighted cost and age | Forecast agent, same frozen packet only. |
| `get_prior_performance` | Approved strategy + asset + bounded lookback → sample count, calibration and uncertainty from past labels actually available by cutoff | Optional experiment arm. Deny queries that can identify the current label. |
| `preview_paper_levels` | Run + validated forecast → deterministic conservative YES/NO price caps with fee assumptions, size and expiry | Paper host or authorized client. Mark hypothetical inputs explicitly. |
| `get_run_review` | Completed run → timeline, simulation and official result when known | Evaluator/operator on a separate authenticated surface. |

Use JSON Schema for inputs and structured outputs, enumerated assets/reasons, decimal units, maximum page/response/tool-call limits and stable error codes such as `STALE_BOOK`, `ASOF_DENIED` and `UNSUPPORTED_RULE`. Every response binds run, ticker, packet hash, cutoff and schema. Server-enforce scoping even if a client supplies another asset, timestamp or run ID. Expose methodology/schema as a versioned resource. For remote third-party clients, support the selected MCP authorization profile, scoped audience-bound tokens and separate read/review privileges. Never pass user tokens through to Kalshi. Treat provider strings as untrusted data.

Have the **trusted runner** write the first schema-valid structured prediction. An external agent needing submission later gets a separate paper-only, run-bound capability with expiry, unique nonce and identical validation. It can never pick its own fills. Initial reply contract:

```json
{
  "run_id": "assigned-by-host",
  "packet_hash": "assigned-by-host",
  "status": "forecast",
  "p_yes": 0.60,
  "evidence_ids": ["book_1", "reference_1"],
  "reason_codes": ["ABOVE_TARGET"]
}
```

Also accept `abstain` with a bounded reason and null probability. A model can issue a forecast while the price policy chooses no trade. Reject invalid probabilities, fabricated evidence, extra fields, mismatched hashes and expired replies. Log model ID/revision if exposed, prompt hash, tool transcript or permitted content hashes, config, input/output tokens, latency, cost and failures. Audit the original output verbatim where rights permit. Replaying a saved decision should be deterministic even when rerunning the language model is not.

**Acceptance:** two different host/model configurations consume the same fixture. Invalid output is recorded once. Attempts to advance the clock, read an outcome or invoke a write tool fail. No public endpoint lists private tools.

## 6. Calculate probabilities and buy levels in independently testable code

**Ticket F — forecasts.** First run a constant/historical-rate baseline and a contemporaneous midpoint baseline, then fit a regularized logistic model using midpoint, spread, time remaining, and depth imbalance. Add entitled reference distance and volatility as a separate challenger. A later settlement-aware challenger can resample historical short reference paths from earlier training windows, compute each simulated final-minute average using the contract rule, and estimate the fraction crossing target. Reject gaps and test calibration. During the final minute the existing required-remaining-average indicator is a threshold. Converting it into a probability requires a validated distribution for the remaining observations. [Kalshi crypto market rules](https://help.kalshi.com/en/articles/13823838-crypto-markets)

Compare the numerical model alone, a small agent on the identical packet, and an agent that can see the numerical model. Declare which baseline is visible. Give every variant the same deadline, cohort and tool limits. Use a separate ledger per strategy. Track whether an agent improves calibration or selection enough to cover added delay and inference cost. If it only explains a good numerical forecast, keep it as an explanation layer.

**Ticket G — paper price policy.** For a fully collateralized binary with dollar payout, let a previously fitted calibration method give a conservative probability interval [L,U] for the estimated YES probability. Estimate bounds from earlier validation data and disclose sample size/support. An agent's self-reported confidence is insufficient. Use L for YES and 1−U for NO. For side s, quantity q and candidate price x in dollars, require

```
q × (conservative_probability_s − x) − modeled_order_fees(q,x)
    − q × residual_execution_allowance >= q × minimum_required_edge
```

Search the contract's valid ticks for the highest qualifying buy price. Use applicable dated maker/taker rules and actual rounding at the order/fill granularity. Compute both sides independently. Depth walk at submission determines eligibility and quantity. Separate modeled slippage already in the execution price from any residual haircut. Unknown fee/tick rules, unsupported probability bounds or insufficient data give a reason and no eligible price. For the pilot take at most one side, using a frozen tie-break rule and risk limits. [Kalshi fee schedule](https://kalshi.com/fee-schedule)

**Synthetic acceptance example:** YES probability 60%, conservative interval [56%,64%]. Assume an illustrative combined fee/allowance of 2¢ per contract, required edge 1¢ and a 1¢ grid. YES ceiling is 53¢ and NO ceiling is 33¢. A fresh YES offer of 54¢ yields a pass on YES. A decision-time offer at 52¢ followed by 55¢ at execution yields zero simulated fills. These numbers are a fixture, not a claim about current quotes or fees.

## 7. Paper trade against evidence and give the user a replay

**Ticket H — ledger and paper simulator.** Start with marketable limit orders and an immediate-or-cancel simulation. Reserve a fixed virtual balance per strategy. After the decision, walk the eligible opposite-side depth from best to worst within the cap, enforcing displayed size, tick, fees and the timestamp policy. Record estimated price/quantity for each level, unfilled remainder, quote age and reference to the execution book. Keep distinct records for intent, simulated fill, cancel/expiry, settlement and correction. Book change between observation and placement is unknown. Label this a displayed-liquidity simulation and stress it by reducing visible size, worsening price and increasing delay. Within a strategy, shared displayed depth can be consumed only once.

A lower proposed limit can be logged as a watch level. With isolated checkpoints a touch during the unobserved interval is unknown. Add a resting-order model only after authorized trade/sequence capture, with queue-ahead assumptions and pessimistic/optimistic sensitivity. A candle low, midpoint or isolated traded price cannot establish a passive fill. Execute the first pilot with fixed exposure caps per market, per aligned BTC/ETH/SOL cycle and daily virtual loss. Count all skipped opportunities, timeouts and missing execution books. Unknown execution is distinct from a verified no-fill.

**Ticket I — Data Lab scorecard.** For each cycle show a timeline with packet time, prediction, buy-up-to levels for both sides, decision-time offer, later executable-size estimate, eventual official result and net paper P&L. Draw the model probability and market midpoint on one probability scale. Put executable offer, price ceiling and actual simulated price on a cents scale. Show spread, sample size, coverage and gap reasons as text. Show aggregate paper equity and drawdown beside calibration by probability band. Use the existing replay, themes and mobile layout. During blinded playback reveal each item only after its recorded availability, including settlement. Keep private run details behind appropriate access.

**Acceptance:** tests cover complement price and depth, partial fill, rounding, stale/crossed books, no future quote, late decision, insufficient virtual funds, a void and a corrected outcome. A YES and NO fixture settles to the correct balance after fees. The UI explains a correct forecast that lost money and a correct forecast that received no fill.

## 8. Define “beats the market” before seeing the results

**Ticket J — evaluator.** Freeze the experiment manifest with training dates, feature and fee versions, strategy, calibration, edge threshold, virtual balance, size, latency assumptions, primary metrics and stopping rule. Split whole UTC days in chronological order. Keep all three assets in each quarter-hour block together. Fit normalization, calibration and thresholds only on earlier folds, purge overlapping feature/label horizons and require that training labels were available at training time. Reserve an untouched final period and then run a forward paper period with hashes locked. A changed prompt or threshold starts a new experiment.

| Question | Primary comparison | What to report |
|---|---|---|
| Is the forecast more informative? | Paired Brier loss, mean of (p−outcome)², against the same-time midpoint. Lower is better. Also compare log loss with fixed clipping and reliability curves. | Matched eligible rows, all scheduled rows, per-asset and time-of-day slices, calibration counts and uncertainty. Score the numerical baseline on the same subset. Also score a deployed agent-plus-fallback policy on the full eligible cohort so abstention cannot manufacture a win. |
| Is the buy-level policy economically useful? | Net simulated P&L per scheduled eligible opportunity and per dollar committed versus abstaining and a predeclared simple market-based policy under identical fill/cost assumptions. | Filled size, no-fills, unknown executions, fees, cash drawdown, turnover, concentrated exposures, cost-adjusted P&L including agent cost, and sensitivity to latency/depth/fees. Report unknown coverage and adverse bounds rather than silently imputing it as zero. |
| Does the model earn its place? | Paired small agent vs. numerical baseline vs. agent-assisted baseline on the same markets. | Quality, coverage, schema violations, latency p50/p95, timeouts, cost per 1,000 decisions and incremental net dollars per 1,000 cycles. |

Suggested first evidence gate: collect a seven-day engineering pilot to measure coverage, costs and variability. Use it to choose an economically meaningful minimum improvement and required test length before looking at holdout results. Plan at least 30 subsequent prospective days across different conditions, extending if clustered uncertainty is wide. Gate per-hour targets and per-asset coverage explicitly, for example 95% usable opens per asset as an initial engineering objective. Report collection failures even on volatile days. Use paired block bootstrap by day or longer contiguous blocks if dependence persists. Prespecify the primary strategy/contrast and correct for multiple challengers if making multiple claims. Require a positive lower 95% bound on improvement for the claimed primary metric. For a trading pilot require positive conservative net P&L and improvement over the specified economic benchmark, acceptable predeclared drawdown, and survival under adverse costs. Label insufficient evidence as inconclusive. No fixed number of days establishes an edge by itself.

## 9. Add Kalshi write access in a later, independently approved project

**Ticket K — demo execution adapter.** Build only after the paper ledger, reconciliation tests and owner decision to start demo integration. Keep forecasting MCP read-only. Accept a signed, expiring order intent from a separately authenticated operator flow. Bind approval to environment, account, ticker, outcome side, size, maximum cost, price and expiry. One private executor translates that intent into exchange fields. Scope its server-side key and identity to the intended environment/account. Keep demo and production credentials, hosts and ledgers separate. [Kalshi demo environment](https://docs.kalshi.com/getting_started/demo_env)

1. Pin Kalshi's event-contract OpenAPI revision and verify signing and permitted key scopes for the chosen account in demo. Read system/market status, tick grid, balances, open orders and positions. A transactional risk check reserves available funds and caps total aligned crypto exposure, size, loss, outstanding orders and order rate. A human-approved price ceiling cannot be exceeded by a fresh quote check.
2. The currently documented V2 creation route is `POST /trade-api/v2/portfolio/events/orders`. The payload uses a single YES-price book. Buying YES at up to $0.53 maps to a bid at `0.5300`. Buying NO at up to $0.33 maps to an ask on that book at `0.6700`. Use fixed-point size, a stable `client_order_id`, explicit time in force and supported self-trade prevention. Verify both mappings and the resulting exposure in demo, including existing opposite positions. Start with price-limited immediate-or-cancel. Revalidate the pinned schema before implementation. [Create Order V2](https://docs.kalshi.com/api-reference/orders/create-order-v2) · [Order direction](https://docs.kalshi.com/getting_started/order_direction)
3. Persist the approved intent and reserve funds before sending an authenticated request. Track submitted, unknown, acknowledged, partial, filled, canceled, rejected and expired states. An acknowledgement is separate from a fill. Reconcile exchange order IDs, fill IDs, actual fees, positions and cash from authoritative portfolio reads/updates. Deduplicate fill events and handle fills racing a cancellation.
4. On transport timeout after submission, retain the reservation, reconcile by client/exchange identity and avoid blindly sending another order. Unknown states block further conflicting intents and surface to the operator. A kill switch disables submissions, attempts cancellation of known resting orders and reconciles residual positions. Specify behavior on restart, revoked credentials, rate limits and settlement corrections.
5. Demo acceptance includes duplicate intent, lost acknowledgement, partial fill, cancel/fill race, wrong environment, stale approval, crash before/after submit, reconcile restart and a kill-switch drill. Demo proves exchange integration. Run the economic experiment on prospective production market observations. A future live pilot needs separate owner authorization, explicit dollar limits, approved account/market scope and an operator who can stop it. No portfolio writes, RFQs, transfers or payments belong in the research MCP.

## Assigning the work

| Order | Give a coding model this slice | Evidence to bring back |
|---|---|---|
| 1 | A. Coverage report and versioned packet schema against existing minute-10 data | One frozen example per asset or a documented missing record, expected vs. observed counts, leakage tests |
| 2 | B and the capture portion of D. Local opening/post-decision capture with the storage interface | Mocked clock/restart/rollover tests, backwards compatibility, rights checklist by field, proposed cost per day |
| 3 | F and G. Midpoint, logistic baseline and decimal price ceilings | Reproducible offline experiment, fee/tick fixtures including YES 53¢ and NO 33¢, honest unsupported-data output |
| 4 | C and E. Run one local cycle with a numerical strategy, then connect one small model through MCP | Actual protocol discovery/tool call, scoped denials, immutable decision receipt, deadline/duplicate tests |
| 5 | H. Simulate size-limited orders, reserve cash and reconcile settlements | End-to-end fixture from open to balance, later-book evidence and unknown-fill cases |
| 6 | Finish D and activate a bounded prospective pilot only after reviewing budget, rights and private access | Real timestamps for all three assets, cold-start and schedule jitter, one-day cost/coverage report, rollback drill |
| 7 | I and J. Add Data Lab replay and run the frozen comparison | Mobile/desktop light/dark review, reproducible scorecard, uncertainty and a go/continue/stop decision |
| 8 | K. Isolated demo adapter as a separately authorized task | Version-pinned exchange contract tests, reconciliation and operator stop drill |

**Copyable first assignment**

> Read AGENTS.md, the project memory, handoff and README. Implement only Ticket A of this plan. Start with the existing minute-10 schema and official outcomes. Produce a bounded, read-only coverage report and versioned point-in-time packet fixtures for BTC, ETH and SOL. Demonstrate that a receipt 21 seconds after cutoff is unavailable at cutoff, changing the official winner cannot change the input hash, a missing book stays missing, and a legacy row can be read. Use synthetic fixtures in Git and record the input manifest for any permitted real-data check. Run the repository's required checks for touched code, update the handoff with exact results, and report coverage limitations. Keep deployment, new collection schedules and exchange access outside this ticket.

For each following assignment specify its letter, prerequisites, allowed files or service, input/output examples, acceptance tests and production scope. Use local fixtures or an emulator by default. Follow the repository's Rust, frontend and layout checks for the affected surfaces, and include authorization and stale-request tests for new routes. Any changes to trading scope, data licensing, recurring spend or cloud permissions require their own concrete decision. The first ticket needs neither a trading key nor additional GCP write access.
