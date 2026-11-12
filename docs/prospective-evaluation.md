# Ticket J1: prospective evaluation

The private collector can evaluate saved BTC, ETH and SOL opening forecasts against captured official results. It never fits, recalibrates or regenerates forecasts. Reports are on demand; existing collection schedules continue unchanged. No report/outcome tool is added to the blinded MCP and no public scorecard is published.

## Frozen experiment

Protocol `prospective_opening_eval_v1` fixes the deployed `opening_logistic_sigmoid_v1` model object, generation and original GCS creation time. Its manifest was registered create-only in the existing private research bucket at **2026-11-11 18:12:19.421 UTC**, generation `1794420739411830`:

`prospective/experiments/v1/97616a6481c3a6cc1435afb6198770a93874e37f4cd6779169d8247d49fc7a64.json`

- Engineering observations begin at 11 November 16:15 UTC, when the first verified live forecasts were committed. They were already observed before J1 registration and are explicitly engineering checks.
- The confirmatory period starts **12 November 2026 00:00 UTC**, after registration. Entire UTC days and all three aligned assets stay together. The model, metrics and support rules are locked before those future observations.
- Five strategies share the exact same scored rows: constant 0.5, frozen training historical YES rate, contemporaneous midpoint, saved raw logistic and saved sigmoid-calibrated logistic.
- Primary contrast: mean paired Brier loss, calibrated minus midpoint; negative is better. Secondary diagnostics are clipped log loss (1e-6), five fixed equal-width reliability bins, and threshold-0.5 classification accuracy (ties predict YES).
- The fixed summary support gate requires 30 complete confirmatory UTC days, each with at least 95% timely-forecast **and** scored coverage separately for each asset. A day has 96 expected markets/asset. Engineering/partial/low-coverage days cannot qualify.
- Once support exists, a deterministic 2,000-replicate UTC-day block percentile bootstrap (seed 17) reports a descriptive 95% interval for paired mean loss. Each sampled day carries all its assets/cycles; unequal scored counts retain row weighting. It assumes exchangeable day blocks. Longer serial dependence needs a separately frozen protocol.

This interval describes uncertainty in **average forecast-loss difference**, not a YES probability bound or an executable price. No result automatically certifies market edge, supplies calibration bounds or enables an order. A summary remains `insufficient_evidence` before support, then `diagnostic_review_only`. Daily reports cannot establish multi-day evidence by themselves. Model changes require a new registered experiment, not changes to this holdout.

## Source and time boundaries

`POST /research/evaluate-day` accepts only `{"day":"YYYY-MM-DD"}`. The server obtains a Firestore readTime and pins all three official-summary queries to that readTime. Queries select only summary and close time from the app's named `crypto-prediction-lab` database. GCS listing is bounded to the completed opening windows of that UTC day; media reads pin the listed generation and exclude objects created after readTime. Today is partial through its last closed quarter-hour. The initial engineering day begins at 16:15 rather than pretending earlier forecasts existed.

For each expected asset/window, the evaluator independently verifies original decision name/run/asset/strategy, original cutoff, production and authoritative commitment ordering, commitment before +40s, packet projection/hash, forecast proof/hash, model generation/creation time and saved probability arithmetic. It consumes **saved probability outputs**. A legacy decision without a committed forecast remains unavailable. No later book, rule field, candle, outcome or evaluator score enters forecast extraction.

Official result versions join by asset, ticker, open and close. Availability is conservatively `max(Firestore updateTime, summary captured_at)`, never first creation or an assumed provider publication time. Source version strings are preserved exactly. Only YES/NO score; void, empty, unsupported and pending/missing results are explicit non-scoring reasons. Current missing results are never filled from a previous report. Future versions are removed before report input hashing: appending unavailable versions cannot change an earlier report's input identity, scores or forecast identity.

The existing collector now detects a changed result even if the provider update timestamp is unchanged. Settled non-binary corrections invalidate the old archived YES/NO state and store an empty candle set without another candle request. Unchanged unsupported states are not rewritten every cycle. Existing public normalizers preserve non-binary gaps. The collector's existing two-day settled-market lookback remains; corrections outside its observed coverage are not guaranteed.

Every evaluation saves the observed official versions in immutable private input shards. A subsequent day report reads the previous report's label shards for comparison, counts source-version revisions separately from changed outcomes, and links the previous report. Older reports/inputs never change. Provider versions overwritten before an evaluation observed them cannot be reconstructed; this is not a complete provider revision history.

## Coverage and scores

Every completed scheduled asset/window remains in the denominator. Reasons distinguish missing decisions, late/invalid commitments, abstentions, absent forecast proofs, wrong model versions, invalid packet/proof, unavailable official results, non-binary outcomes and ticker mismatches. Timely forecast coverage and scored coverage are separate. Missing/unscored opportunities are not counted as correct, wrong, no-fill or zero loss.

All five strategies use the same valid forecast/result pairs. Each report contains pooled and per-asset Brier/log-loss/accuracy/reliability diagnostics, paired delta sums, support and gap reasons. Empty reliability bins and empty scores are null. Full-scheduled paired-loss bounds assign unknown opportunities the conservative range [-1,1], rather than imputing zero; these are coverage bounds, not statistical confidence intervals.

A summary reads the latest published immutable day report for each selected day. It records exact report generations and individual daily support. Missing reports, partial days, engineering days and inadequate coverage remain visible. Summary inputs cannot include a report published after its as-of time. The maximum lookback is 31 calendar days. J1 deliberately performs no hyperparameter selection, refitting, retrospective forecast attachment, simulated execution or P&L calculation.

## Private API and storage

The three routes exist only on the current IAM-protected collector:

| Route | Input | Result |
|---|---|---|
| `POST /research/evaluate-day` | Exact `day` body | Newly saved immutable daily report and input references |
| `GET /research/evaluation?day=...` | Exact day query | Latest published immutable report, or `not_evaluated` |
| `POST /research/evaluate-summary` | Exact `day` body, interpreted as through-day | Saved summary of at most 31 daily reports |

Unknown fields, caller cutoffs/as-ofs, assets, models, outcomes, buckets and source selectors are rejected. Allowed days begin at the engineering date, end today and are at most 31 days old. Request bodies are limited to 8 KiB. Report writes have a 40-second handler bound; saved-report reads have eight seconds. The existing collector service ceiling stays 60 seconds.

Heavy report work is admitted only at quarter-hour offsets **03:30–07:59** or **11:30–11:59**, leaving room for existing opening, follow-up, history and minute-ten captures. Outside these bands it returns 503 with Retry-After 45; this is not a collection failure. There is no new scheduler, recurring report job, always-on worker, provider subscription or resource-limit increase.

Objects are create-only, content-addressed and bounded to 64 KiB each in `crypto-prediction-lab-research-308289525742`:

- `prospective/experiments/v1/`: registered protocol.
- `prospective/inputs/v1/<experiment>/`: separate header, decision, current-label, previous-label and scored-row shards (48 KiB target).
- `prospective/manifests/v1/<experiment>/`: input reconstruction references and hashes.
- `prospective/reports/v1/<experiment>/days/<date>/`: immutable daily report revisions.
- `prospective/summaries/v1/<experiment>/`: immutable summaries with exact day-report references.

Only the app collector/operator's existing permissions are used. The MCP identity's scope-only bucket grant cannot read these prefixes or the database; it has no collector invocation permission. The public dashboard has no evaluator route. No raw inputs, scope capabilities, credentials or private captures belong in Git. Incomplete input parts after a timed-out call are not a report; retries produce new as-of snapshots without overwriting earlier objects.

A full-day run has at most 288 decision media reads, four bounded decision-list pages, three bounded official-summary queries plus one read anchor, prior-report lookup and bounded prior-label reads, followed by small input/report writes. Source decisions already contain the probabilities. Recent partial days cost less. Summary work reads up to 31 day-report inventories/selected reports. Costs remain subject to the existing $40 budget-alert target, not a hard cap; this release adds no automatic report cadence.

## Operator workflow

Use the trusted operator identity, never an agent's tool arguments. Pick an admitted window:

```sh
python3 scripts/run_prospective_evaluation.py --day 2026-11-11 --output /private/tmp/evaluation-first --download-input
python3 scripts/verify_prospective_evaluation.py /private/tmp/evaluation-first
DASHBOARD_ROLE=evaluation-replay EVALUATION_INPUT=/private/tmp/evaluation-first/input.json EVALUATION_OUTPUT=/private/tmp/evaluation-replayed.json cargo run --locked -p dashboard
python3 scripts/run_prospective_evaluation.py --day 2026-11-11 --mode summary --output /private/tmp/evaluation-summary
python3 scripts/run_prospective_evaluation.py --day 2026-11-11 --mode get --output /private/tmp/evaluation-readback
```

The client saves owner-readable files outside source control, downloads exact object generations, and verifies the raw stored SHA-256 bytes (including Rust's JSON float representation). The input manifest reconstructs every normalized source and label version. Replaying that archived input reproduces the core report; the outer saved report additionally has its GCS input-manifest reference. The independent Python checker verifies actual cutoff/model availability, saved prediction arithmetic, all 20 asset/strategy score pairs, 100 reliability-bin counts and adverse missing-data bounds.

`DASHBOARD_ROLE=evaluation-manifest` writes the fixed registration JSON; `scripts/publish_evaluation_manifest.py` verifies the pinned model metadata and publishes it create-only before the confirmatory start. `scripts/deploy-prospective-evaluation.zsh` updates only the existing collector image and `EVALUATION_EXPERIMENT_OBJECT`. MCP and public services need no deployment for operator evaluation.

Collector revision `crypto-history-collector-00016-j4q` is deployed. The first live report scored 27/27 observations (nine per asset); both saved report versions replayed exactly and live MCP/authorization checks passed. The summary correctly has zero complete confirmatory days and no uncertainty interval. [Aggregate verification evidence](research-audits/prospective-evaluation-2026-11-11.json) and exact checks/deployment/results are recorded in `SESSION_HANDOFF.md`. Next: accumulate untouched confirmatory days and produce daily reports without tuning on their scores. Consider a report schedule only after measuring its operating cost and capture impact. Verified-rule paper pricing and a fill ledger remain separate work.

## Public Data Lab snapshot

The owner requested public live-test performance on 11 November 2026. Data Lab's forecast section now defaults to the prospective daily snapshot, with the historical pilot retained in the Test selector. Publication uses `python3 scripts/publish_live_forecast.py /private/tmp/<verified-daily-report>`. The helper independently verifies the private report and exports only aggregate strategy metrics, coverage, missing-reason counts and the as-of time into the existing public JSON. Raw inputs, model artifacts, decisions and labels remain private. The existing capture schedules continue to commit new forecasts; publishing updated public aggregates is an operator action and requires the dashboard asset release.

The first public confirmatory snapshot is the partial UTC day 12 November, as of 02:11:44.454605 UTC. It contains 24 scored contracts across eight closing times, eight per asset, with all 24 forecasts timely and no unscored opportunities. Pooled calibrated-model Brier is 0.23295693046233823 versus midpoint 0.22354062500000005 and constant 0.25. This early sample favors the midpoint. The model is frozen and this report does not establish profitable selection or multi-day support.
