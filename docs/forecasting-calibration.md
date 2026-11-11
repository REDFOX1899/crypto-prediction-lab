# Original Ticket F: cutoff-safe numerical forecasts

`opening_logistic_sigmoid_v1` is a private, experimental numerical challenger for BTC, ETH and SOL opening decisions. It preserves `market_midpoint_v1` and its packet/hash/run ID. A new forecast proof is independently frozen in the same immutable decision. `get_forecast({})` reads that committed proof through the private MCP. An old decision cannot acquire a newly fitted model. No external AI provider, forecast submission, order, fill or public forecast panel is added.

## Available evidence and first result

The frozen extraction at **2026-11-11 15:38:42.761976 UTC** covers 10–11 November. It contains 228 opening decisions: BTC 70/76 usable forecasts, ETH 69/76, SOL 70/76. Respectively 69, 68 and 69 have matching official labels. These are 206 paired observations; 19 decisions abstained and three forecast windows lacked a label at extraction. The calendar range has 474 completed asset-windows; collection did not cover that whole range. Missing openings are not reconstructed from later candles.

The predeclared training/calibration/holdout split has **89/51/54 rows**, spanning **31/17/18 distinct cycles**. Holdout contains 18 observations per asset. Twelve joined rows fall outside eligible split intervals, including the gaps and unavailable-at-fit label versions. All assets in a quarter-hour share one split. These are time blocks across only two dates, not the original plan's whole-day final validation. That stronger evaluation needs more prospective days and a new frozen protocol.

Held-out Brier scores (lower is better):

| Asset | 50/50 | Historical YES rate | Market midpoint | Raw logistic | Sigmoid calibrated |
|---|---:|---:|---:|---:|---:|
| BTC | .250000 | .257053 | .254081 | .256166 | .256604 |
| ETH | .250000 | .250260 | .259935 | .259659 | .258557 |
| SOL | .250000 | .269206 | .235357 | .248887 | .251947 |
| Pooled | .250000 | .258840 | .249791 | .254904 | .255703 |

**This pilot does not beat the pooled market midpoint.** Calibrated-minus-midpoint paired Brier loss is +.005912. Pooled log losses are .693147/.711465/.692329/.703431/.705090 in the same strategy order. No thresholds or model settings were changed after these scores. Per-asset scores, five equal-width reliability bins, support and provenance hashes are in [the aggregate report](research-audits/forecasts-2026-11-11.json). Empty bins remain null. Related assets/cycles and small samples prevent claims of statistical significance, validated calibration or profitability.

## Fixed mathematics

The vector is intercept, logit(opening YES midpoint), spread in dollars, remaining-window fraction, native YES-vs-NO bid-size imbalance across the captured top ten levels, BTC indicator and ETH indicator. SOL supplies the reference intercept. Bid quantities are contracts, not traded volume. Midpoint is clipped to [1e-6, 1−1e-6] before logit. CF distance/volatility challengers remain unavailable because no entitled opening reference archive exists; CoinMarketCap is not substituted.

Normalization uses training means and population standard deviations only; standard deviations have a .01 floor and standardized features are clipped to [−10,10]. The pooled logistic model minimizes mean binary log loss plus .1/2 times the squared non-intercept coefficients. Deterministic full-batch gradient descent uses 800 steps of .1, zero initial coefficients and an unpenalized intercept. The feature implementation is shared by training and live inference.

A disjoint later block fits sigmoid `sigmoid(a + b*z)` to the raw logistic score z, using 800 steps of .05 and a .1/2 penalty on `a² + (b−1)²`. The slope is constrained to [.05,10]. It is fitted post-hoc calibration, not evidence that these probabilities are accurate. Historical YES baselines use per-asset training counts with Beta(1,1) smoothing. All five strategies are scored on identical held-out rows using Brier/log loss, fixed 1e-6 clipping for log loss and five-bin reliability counts/means. No held-out row selects model parameters, calibration parameters, baselines or normalization.

Minimum fit support is 24 training cycles and 12 calibration cycles, with each asset represented by at least eight/six rows, respectively, and both pooled outcome classes meeting the same limits. Insufficient support fails fitting instead of manufacturing a model. Test support below eight cycles is explicitly insufficient. These are engineering guards, not statistical-power guarantees. No actionable uncertainty interval is estimated: `probability_interval:null`, `calibration_status:fitted_diagnostic_only`, `paper_order_eligible:false`. The existing paper-level policy remains conditional.

## Cutoff and label boundaries

- Training rows open before **2026-11-11 05:30 UTC**; the current official label version must be available by **06:00 UTC**.
- Calibration rows open at/after **06:00**, before **10:30**; labels must be available by **11:00**.
- Test rows open at/after **11:00**. Their later labels are evaluator-only and available by the frozen extraction time.
- This predeclared 30-minute gap exceeds the 15-minute outcome horizon and keeps overlapping horizons out of adjoining splits. Dates are fixed in v1, not inferred from future label counts.

Export pins Firestore reads to one readTime and GCS reads to listed object generations born by that readTime. It reads only `decisions/v1/` plus official-label summaries, not execution candidates or candle payloads. The normalized decision input drops rules and unrelated fields. The strict Rust parser rejects unknown fields, duplicate rows, wrong identities, late commitments, incorrect hashes and altered projections. Opening projections are rebuilt through the same network-free allowlist used by the runner/MCP. Ordered requests, receipts and immutable durable source versions must precede the original cutoff; commitment must follow cutoff and precede +40 seconds.

Official contracts are mutable Firestore documents. A label is conservatively available at **max(current document updateTime, summary captured_at)**. First creation and provider event times cannot prove the current corrected result was available earlier. Exact source/version/result hashes bind joins by asset, ticker and window. Earlier overwritten label versions are not reconstructed. Labels are separate from features, and later official values never enter live inference.

The model contains only coefficients, normalization, calibration, earlier aggregate support/rates and train/calibration manifest hashes. Held-out labels/features and evaluation scores do not enter the artifact identity. Mutation tests prove that changing held-out labels changes scores but preserves the model byte/hash identity, and delayed fitting labels are equivalent to excluding those rows.

## Private runtime and MCP

The model is saved create-only under `forecast_models/v1/<sha256>.json` in the existing private research bucket. `FORECAST_MODEL_OBJECT` on the collector selects that exact object. Startup performs one bounded, generation-pinned preload; failure leaves explicit unavailable forecasts while source capture and midpoint decisions continue. Each forecast requires the model's authoritative GCS creation time to be at/after calibration and at/before the packet's original cutoff. The raw and calibrated predictions can be reproduced from the proof; no network or outcome handle enters inference.

Each timely new decision freezes raw/calibrated probability outputs inside `forecast_proof`, together with `forecast_hash`. Reads return those saved outputs and independently validate them against the frozen parameters (1e-12 numerical tolerance), rather than silently replacing them with a later implementation's predictions. Issuing an MCP scope validates that saved proof at the original cutoff; it does not run today's model on an old packet. Scope validation rechecks model identity, label-availability limits, model commitment, packet identity and proof hash. `get_forecast` returns probabilities, model version, baseline comparisons and aggregate support, without coefficients, raw training records or evaluation labels. Optional fields preserve legacy scopes, which return `NO_FORECAST_COMMITTED_AT_CUTOFF`. Caller asset/model/cutoff/phase overrides fail closed. Methodology: `research://forecast-policy/v1`.

The existing MCP identity still reads only its scope prefix; no model-bucket, database or outcome access is granted to it. Only the collector loads the model. IAM, source schedules and resource limits remain unchanged. No automatic retraining, additional scheduler, provider subscription or always-on process is introduced. Per-process cold starts add at most one model metadata/media preload (seven-second total bound); each new decision stores a small model proof. Storage/request costs remain subject to the existing $40 alert target, not a guaranteed cap.

## Reproduction and publication

Use private paths, authenticated operator access and the named database. The batch fitter has no HTTP endpoint and no collection/network work:

```sh
python3 scripts/export_forecast_data.py --start 2026-11-10 --end-exclusive 2026-11-12 --output /private/tmp/new-frozen-extract.json
DASHBOARD_ROLE=forecast-fit FORECAST_INPUT=/private/tmp/new-frozen-extract.json FORECAST_OUTPUT_DIR=/private/tmp/new-fit cargo run --locked -p dashboard
python3 scripts/verify_forecast_fit.py /private/tmp/new-frozen-extract.json /private/tmp/new-fit
python3 scripts/publish_forecast_model.py /private/tmp/new-fit --receipt /private/tmp/new-publication.json
```

After verifying a fitted experiment, preserve its exact private dataset and report in GCP as well:

```sh
python3 scripts/archive_forecast_experiment.py /private/tmp/new-frozen-extract.json /private/tmp/new-fit --model-receipt /private/tmp/new-publication.json --output /private/tmp/new-experiment-receipt.json
```

This writes content-addressed, generation-verified create-only objects under `forecast_datasets/v1/`, `forecast_evaluations/v1/` and `forecast_experiments/v1/` in the existing private research bucket. The experiment manifest links dataset, evaluation and exact model generation without putting held-out labels or their hash into the model. The frozen dataset can be retrieved by the operator for exact reproduction even after mutable Firestore labels change. These prefixes are outside the MCP identity's scope-only grant and never loaded by the live runner. The first normalized dataset is 538,035 bytes; no new store, recurring work or public data exposure is added.

Export/output files are private and refuse overwrite. The publisher verifies hash and exact returned generation, uses `ifGenerationMatch=0`, and never updates IAM. Re-extracting later mutable label versions can legitimately reduce past fitting support; use the original frozen extract for exact reproduction. A new protocol/model version needs a separately frozen experiment, not tuning on these 54 held-out observations.

Deployment and actual live checks are recorded in `SESSION_HANDOFF.md`. Next: accumulate multiple untouched UTC days and prospective forecasts, then freeze a whole-day walk-forward evaluation and uncertainty method. Reference challengers, agent comparisons, a verified-fee paper policy and the paper fill ledger remain separate work. No result here authorizes real trading.

Primary method references: [scikit-learn calibration guide](https://scikit-learn.org/stable/modules/calibration.html), [chronological splitting](https://scikit-learn.org/stable/modules/generated/sklearn.model_selection.TimeSeriesSplit.html). The runtime implements the explicitly frozen Rust optimizer above; it does not call sklearn.
