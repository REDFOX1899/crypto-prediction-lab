# Paper scorecards and economic replay

The Single market price map uses the freshest matching stream book or REST snapshot for the open contract. It requires a positive, uncrossed two-sided book received within 30 seconds. Enter a YES probability and the assumed cost per contract. With a $1 payout, the YES break-even probability equals the YES offer plus costs. NO has positive expected value when the YES probability falls below one minus the NO offer and costs. Ties pass. The opening example assumes 2¢ in costs. Visitors can change that assumption. Best-bid size balance runs from −100 to +100 and measures relative displayed size. It has no calibrated probability interpretation. Switching tickers clears the visitor's probability.

Data Lab compares the historical YES rate following a YES with the YES rate following a NO for the selected 24, 48 or 96 windows. Bars include the numerator and denominator. Gaps break adjacency. Official historical outcomes never enter the private forecast tools.

## Cutoff-safe private pricing (G)

The read-only MCP tool `get_paper_scorecard({})` reuses the assigned validated scope and frozen forecast. It computes one-contract watch prices and fee-adjusted break-even rates for both sides. Only a matching committed rule proof with supported quadratic taker configuration and a verified price/complement grid qualifies. Account-level rounding assumes non-direct member precision. Charges from individual brokers are unknown. The earlier hypothetical v1 tool is still independently versioned.

For each eligible tick, the model point scenario requires the side's estimated probability minus tick price and modeled fee to cover 1¢ required margin and 0.5¢ residual allowance. An eligible conservative order needs independently supported probability bounds at cutoff. The existing model supplies none. The bounded strategy therefore passes. Neither the per-band win rate nor the J1 Brier-loss interval supplies contract-specific bounds.

## Daily engineering study (D, H and I)

Reuse a downloaded J1 input. The operator exporter reads the exact decision generation and the private named database execution document for each slot. It uses the existing credentials of the operator. No service or recurring capture needs a new permission. Use new private output paths.

```sh
cargo build --locked -p dashboard
python3 scripts/export_paper_day.py /private/tmp/day/input.json /private/tmp/paper-bundle.json
DASHBOARD_ROLE=paper-replay EVALUATION_INPUT=/private/tmp/paper-bundle.json EVALUATION_OUTPUT=/private/tmp/paper-report.json target/debug/dashboard
python3 scripts/verify_paper_replay.py /private/tmp/paper-bundle.json /private/tmp/paper-report.json
```

The Rust replay projects the original generation back to the saved J1 fields, verifies rule and forecast proofs, and reuses the validated candidate-link timing logic. A cutoff earlier than durable commitment never exposes a decision in blinded MCP. The evaluator legitimately reads labels after close. Opening intent construction has no label or later-book parameter. The later snapshot starts after the original commitment and routing allowance. A future or missing execution document cannot establish a fill. File creation is exclusive with owner-only permissions. Replay is deterministic by scheduled opportunity. A new official revision yields a new report.

Each isolated strategy gets $288 per UTC day, with at most one non-recycled dollar per market, one contract and $3 per aligned cycle. Each slot walks the later opposite-side depth once within the price limit. A partial simulated fill cancels its remainder. Cash and realized equity track only observed settlement availability. A missing execution holds its dollar in reserve with adverse bounds. A nonbinary correction reverses a known prior provisional settlement and retains unresolved exposure pending authoritative treatment. A revised binary result changes the payout difference. Unobserved intermediate revisions cannot be reconstructed. Daily balances restart independently; these reports do not describe a continuous compounded portfolio.

The report compares five policies plus a zero-dollar abstention benchmark.

| Policy | Decision |
| --- | --- |
| Bounded | Pass while no validated as-of probability bounds exist |
| Model point estimate | Use saved calibrated YES and its complement with verified ticks and modeled fees |
| Midpoint | Apply exactly the same pricing hurdle to the opening midpoint |
| Market favorite | Take the side above 50% at the midpoint, limited to its opening offer and $1 total cost. Ties pass |
| Model under stress | Apply the model cap to half the later size, one worse tick and a minimum ten-second routing delay |

Each strategy's row carries the hypothetical side, limit, commitment time, later book identity, per-level quantity/fee, canceled remainder and official settlement when available. Aggregate diagnostics include realized-equity and cash paths, drawdown, unknown-inclusive P&L bounds, return per scheduled opportunity and per dollar spent. Per-asset coverage and nearest-rank p50/p95 delay metrics include opening receipts, durable decisions, execution receipts and official label availability. Billing data and provider request totals are outside this extract. External model calls are zero.

In Data Lab choose the private JSON file locally. At the decision stage inspect the probability and price bars. At +55 seconds inspect durable fills available then. Full-day review adds official results, simulated equity and calibration counts. Switching windows resets the reveal stage. Leaving Data Lab clears the selected file from the page. The file picker never uploads data. This is an operator display with local possession of the full report. Agent blinding is enforced by the private MCP service.

## Economic experiment (J)

The November 11 replay is retrospective engineering. The following **optional operator commands** register a separate future-period experiment and publish replayable evidence in the existing dedicated private bucket. Use the actual future UTC date and private paths. These commands are independent of J1 registration. No schedule is installed.

```sh
python3 scripts/paper_experiment.py register /private/tmp/paper-report.json YYYY-MM-DD --output /private/tmp/paper-registration.json
python3 scripts/paper_experiment.py archive /private/tmp/paper-bundle.json /private/tmp/paper-report.json --output /private/tmp/paper-day-receipt.json
python3 scripts/paper_experiment.py compare /private/tmp/paper-registration.json /private/tmp/paper-day-receipt.json --output /private/tmp/paper-comparison.json
```

Registration freezes the engine fingerprint, exact forecast experiment, capital, strategies, conservative comparison and fixed 30 consecutive UTC dates. Registration requires GCS creation before the first date. Input/report/manifest objects live under `paper_studies/v1/`, use create-only semantics and verify pinned bytes. Publication replays the bundle first. Compare accepts receipts for the same cohort, at most one revision per day, checks GCS generations and hashes, and lists missing days. For every asset/day at least 92 of 96 need timely forecasts, scored label coverage and evidence for both pricing and the later book. Unknown executions and unresolved settlements enter conservative bounds. The primary contrast uses the model scenario's lower net minus the favorite's upper net per scheduled window. The 2,000-resample interval groups whole UTC days across assets and assumes exchangeable day blocks. It is descriptive. Longer dependence, poor coverage and economic loss all limit interpretation.

A confirmatory result needs future observations and registration before collecting the cohort. The private tool has no submission or order method. Ticket K requires a separate owner decision about a demo account and remains outside the read-only application.
