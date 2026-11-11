# Fee-aware conditional paper buy levels

The owner requested this as Ticket F. The older research proposal names forecasts F and the price policy G; this release implements the requested **price-policy slice**, not a fitted forecast or simulator. `preview_paper_levels` is a private, read-only MCP tool for BTC, ETH and SOL. It uses only the host-assigned cutoff-safe opening packet. It never reads a later book, outcome, live provider, database or unrestricted clock.

## What the numbers mean

The tool returns conditional YES and NO buy-up-to prices, each independently computed on an **assumed** one-cent grid. It also shows the original opening offer, visible depth below the ceiling, modeled fees, conservative fee reserve, residual allowance and hypothetical net edge. Missing depth remains missing. A lower watch level does not establish that an order could fill later.

There are no captured fee/tick rules in packet schema v1 and no validated probability calibration yet. Therefore **every result has `paper_order_eligible: false` and `eligible_buy_price_usd: null`**. Numeric `buy_up_to_usd` values belong to a disclosed hypothetical scenario. The request cannot promote them to calibrated forecasts or eligible orders. The adapter does not write a prediction, intent, balance, fill or profit record. Actual rule capture, as-of calibration evidence and the paper ledger are separate work.

Two scenario kinds are supported:

- `midpoint_benchmark` uses only the saved observed midpoint, with equal lower/upper values. That equality is a scenario assumption, not a confidence interval or calibration claim.
- `hypothetical` accepts decimal-string `p_yes`, `lower`, `upper`, with `0 <= lower <= p_yes <= upper <= 1`. Values entered after the fact never become claimed pre-cutoff forecasts. An agent's self-reported bounds remain unvalidated.

For YES use `lower`. For NO use `1 - upper`. The supported quantity is 1–100 **whole contracts**. Risk constraints and policy parameters are fixed by code, not agent arguments. Fee, asset, run, cutoff, phase, calibration and policy overrides are rejected.

## Versioned assumptions and exact arithmetic

`paper_buy_levels_v1` assumes the general quadratic taker coefficient 0.07, multiplier one and no settlement fee, with a non-direct-member cent balance precision. These are mathematical assumptions, not verified contract-specific historical metadata. Discounts, maker fees, FCM surcharges, exits and overrides are outside the scenario. The current general formula comes from the [official fee schedule, effective July 7, 2026](https://kalshi.com/docs/kalshi-fee-schedule.pdf). The [current rounding documentation](https://docs.kalshi.com/getting_started/fee_rounding) distinguishes trade-fee precision, balance precision and per-order accumulated rebates. We do not apply an older flat cent-rounding description as an exact universal account rule.

The modeled fee implementation rounds each hypothetical fill's model fee upward to six decimal dollar places, aligns the buy balance change downward to the assumed cent precision, accumulates rounding overpayment and rebates whole precision increments without making that fill's net fee negative. One displayed price level is one modeled fill in a single hypothetical order. The output also shows a separate stress assumption: independent one-contract orders rounded to cents without rebates. Actual matching/fill fragmentation is unknown.

For requested whole quantity `q`, grid price `x` and conservative scenario probability `r`:

```text
stress_fee(q,x) = q * ceil_cent(ceil_6dp(0.07 * x * (1-x)))
q * (r - x) - stress_fee(q,x) - q * 0.005 >= q * 0.01
```

The engine searches **every** allowed price from 1¢ through 99¢ and returns the highest qualifying price, or null with `NO_POSITIVE_PRICE`. It uses checked input ranges and bounded Decimal arithmetic, with no floating-point price decisions. The fixed minimum edge is 1¢/contract, and the separate residual execution allowance is 0.5¢/contract. That allowance is an explicit stress assumption, not a measured slippage estimate. Prices already encountered in the depth walk are not charged again as slippage.

At the frozen opening book, YES asks derive from `1 - NO bid`, and NO asks from `1 - YES bid`. The walk preserves level sizes and quantities, reports uncovered remainder, and requires the full requested size for a hypothetical side selection. The larger of modeled level-fill fees and unit-order stress fees is reserved. If the actual modeled fee would defeat the required edge, the side is rejected even if its watch ceiling exists. Supported depth quantities have at most two decimal places; unsupported subcent prices or size precision are reported without rounding them into invented depth. Modeled entry cost plus reserved fees must stay at or below $100. At most one side is conditionally selected, by larger conservative total net, with YES on an exact tie. These checks never change the ineligible status above.

A hypothetical intent expiry is derived only from the frozen cutoff: cutoff + one second, capped at open + 40 seconds. A later replay retains that original expiry. The timestamp does not authorize an order and does not renew on tool calls.

## Synthetic examples

With an assumed YES probability of 60% and bounds [56%,64%], the current policy produces YES **52¢** and NO **32¢** ceilings for one contract. At 52¢ the stressed entry fee is 2¢ and the residual allowance is 0.5¢; the remaining scenario edge is at least 1¢. These are fixture values, not current market quotes or validated probabilities.

The older proposal's 53¢/33¢ acceptance example used a combined 2¢ fee/allowance with no extra residual haircut. The pure engine reproduces those ceilings when that fixture's residual is zero; the deployed policy keeps its separate 0.5¢ allowance and therefore rounds down another cent. Policy changes require a new version/hash, rather than silently reinterpreting saved results.

## Private MCP contract

Request:

```json
{
  "packet_hash": "<host-assigned SHA-256>",
  "quantity": "1",
  "forecast": {
    "kind": "hypothetical",
    "p_yes": "0.60",
    "lower": "0.56",
    "upper": "0.64"
  }
}
```

For the observed-midpoint scenario replace the forecast object with `{"kind":"midpoint_benchmark"}`. Extra fields are rejected, including parameters nested inside that benchmark variant. Probabilities require ordinary decimal strings with up to six fractional digits; exponents, negative values, non-finite values and inconsistent bounds fail closed. Request quantity is an integer string and cannot exceed 100.

The existing MCP envelope binds run, asset, ticker, cutoff, packet hash and schema. `data` adds the policy version/hash and a scenario hash binding packet hash + policy hash + canonical scenario. The `research://paper-policy/v1` resource returns assumptions and units. The existing methodology resource remains available. Scope validation still reconstructs the original pure opening projection before any calculation, and the trusted host must continue withholding outside data/review/clock/database tools from a blinded agent.

Expected domain errors include `ASOF_DENIED`, `INVALID_FORECAST`, `INVALID_QUANTITY` and `INVALID_BOOK`. Each side has explicit no-level/depth reasons such as `NO_POSITIVE_PRICE`, `OFFER_ABOVE_CAP`, `INSUFFICIENT_DEPTH_UNDER_CAP`, `UNSUPPORTED_ASSUMED_GRID_OR_SIZE_PRECISION` and `NET_EDGE_BELOW_MINIMUM`. Request time cannot advance the evidence cutoff. The mathematical assumptions are not claimed to have been market observations available at a historical cutoff.

## Verification and deployment

The Rust tests reproduce the official rounding example, exercise accumulator rebates and compare all 1,001 probability steps with an independent integer-microdollar exhaustive-tick oracle. They cover complements, partial depth, unsupported precision, zero/one bounds, sizes, malformed requests, extra fields and hash mismatches. Mutation tests prove outcomes and later observations cannot change projected inputs or levels, and late durable availability is denied. The official Rust SDK also invokes the tool and attempts forbidden scope/policy overrides.

`scripts/verify_paper_levels.mjs` is an independent host using the official TypeScript SDK. It checks the pure math with a BigInt fee/tick oracle, policy/scenario hashes, unchanged packet cutoff, repeated-result equality, both scenario modes, disabled eligibility and adversarial requests. Use a separate freshly issued capability for this verifier so it does not consume another host's 32-call allowance. Raw scopes/transcripts remain private. The official Inspector also checks protocol discovery/schema and actual tool calls.

`scripts/deploy-paper-levels.zsh <immutable-app-image-digest>` updates **only the private MCP image**. No IAM, runtime identity, resources, schedules, collector image, public dashboard, provider or model subscription changes are required. The existing $40 monthly alert remains a target, not a hard cap. There is no recurring model invocation, new collection cadence or always-on worker. Release evidence is added to the session handoff after the actual deployment checks finish.
