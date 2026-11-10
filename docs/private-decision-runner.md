# Private prospective decision runner

Ticket C adds a deterministic market-midpoint baseline for BTC, ETH and SOL 15-minute singles. Strategy `market_midpoint_v1` estimates a YES probability proxy as (best YES bid + derived YES ask) / 2. It is a benchmark for later model comparisons. Calibration, predictive edge, executable price ceilings and paper fills remain unvalidated or separate work.

## Cycle lifetime

The private `crypto-cycle-checkpoints` job now begins in the prior minute through `/collect-opening`. Bounded retries admit a handler that discovers the exact contracts and captures after +5s. Immediately after durable opening captures, the runner processes all three assets concurrently. It freezes its actual UTC cutoff before reading the assigned opening document. The pure projection validates that document and constructs an allowlist packet. The numerical strategy consumes the projected midpoint and performs no network calls. The host stores the first forecast or abstention before requesting any link to later data.

A separate quarter-hour `crypto-cycle-followup` job waits until +45s and saves execution candidates. The runner reads the committed decision and candidate, checks eligibility and stores an immutable link. Each decision phase has a five-second internal budget inside the existing 50s collection handler. Capture records stay independent. A timeout or storage failure leaves pending/missing run artifacts for private reconciliation. The HTTP capture response includes research phase results. Original collection writes remain conditional and the minute-10/archive jobs retain their behavior.

## Cutoff and identity

Packets require a version-1 opening capture for the exact registry asset, opening, 900-second closing window and phase. The source document ID must be in this application's named database. Both create and update times must be at or before the cutoff, and must be equal because only first-attempt immutable captures are supported. A changed document is rejected. Metadata/book request and receipt times must be ordered and the complete observation must have been durably available by cutoff. The target must be positive, comparator `greater_or_equal`, and ticker must match the registry series. Both sides must have positive sizes and valid unique prices, with at most ten levels and an uncrossed best bid/derived ask. Source receipt age above 30s is rejected under the initial pilot policy. Provider publication age remains unknown.

The packet exposes only asset/window/ticker, cutoff/provenance/timing, target/comparator, normalized depth, bid/ask/midpoint/spread and the source-age disclosure. Official results, expiration values, hindsight prices, future candidate data, arbitrary raw metadata and archived outcomes do not enter features or their SHA-256 hash. Future data and failed validation result in abstention with a bounded reason and null features/probability. The host supplies the cutoff. Requests cannot provide their own cutoff or probability.

Forecast creation must occur before +40 seconds. The durable Cloud Storage object's server `timeCreated` is the commitment used by link eligibility. A forecast committed at/after +40 is diagnostic and its effective status is late. It cannot qualify for paper testing. Producer time must not follow commitment. Only the first object is retained, including abstentions. Each run ID binds environment bucket, schema path, registry series, actual opening and fixed strategy version. Change the strategy version for a different prediction policy.

## Links and paper evidence

An eligible link requires a timely committed forecast, matching packet hash/probability/identity, the same opening and candidate ticker, a valid execution-candidate snapshot, and candidate request and book-request times at least **one second after durable decision commitment**. Future candidate availability is rejected. The candidate projection is validated at its first durable availability time, allowing later audit without falsely applying current wall-clock staleness to the old evidence. Missing candidates stay pending until +55, after which absence is terminal for that link. An unavailable first candidate, abstention, late decision, modified capture or identity error receives an explicit ineligibility reason.

Links record exact decision generation and commitment, original packet hash, routing allowance, candidate document ID and projected evidence. The opening and candidate source documents retain null decision fields because multiple strategies may later refer to them. Link objects provide the association without overwriting observations. A timing-eligible link supplies evidence to a later paper simulator. It does not establish an order, a fill, queue position or net profitability.

## Private storage and access

Decisions and links live in the dedicated regional private bucket `crypto-prediction-lab-research-308289525742`, separate from dashboard-readable Firestore. Uniform bucket-level access and public access prevention are enabled. The existing collector identity receives bucket-scoped `storage.objectCreator` and `storage.objectViewer`. Its successful path needs no overwrite/delete rights. Standard project administrator access also applies. The dashboard identity's project binding is the existing database-specific datastore viewer role and its existing secret grants. It receives no bucket role. No model provider credentials or paid inference calls are introduced.

Object paths are `decisions/v1/s<open>_<SERIES>_market_midpoint_v1.json` and `links/v1/s<open>_<SERIES>_market_midpoint_v1.json`. Writes use `ifGenerationMatch=0`. A precondition conflict reads the existing generation. Reads pin media to the generation from metadata so a metadata/body race cannot change evidence. Records are limited to 64 KiB. Bucket setup is in `scripts/setup-private-research.zsh`; it checks the project number before applying bucket roles. Raw private records and user credentials remain outside Git.

At full cadence this adds 288 decisions and 288 links/day, plus bounded GCS and Firestore reads. Normal collection uses roughly 24 GCS requests and six source-document reads per cycle for the runner, including the follow-up decision-existence check. Dispatch audit and capture bookkeeping add their own bounded operations. The normal record sizes are a few KiB, with a 64 KiB ceiling. Storage/operations, existing captures, retries and operator requests incur applicable charges. The existing $40 monthly alert is a target. Runtime CPU/memory/min/max scale and observation/decision deadlines are retained. The automatic timing repair separates the opening and follow-up schedules. See [current dispatch](reliable-cycle-dispatch.md). Unset `DECISION_BUCKET` on the collector to disable automatic decision work while retaining captures. No new continuously running service is required.

## Private operator interfaces

Cloud Run IAM protects these collector-only routes. The public dashboard has no matching routes.

- `POST /run-decisions` accepts `{}` for the current cycle or `{"open_time": <UTC epoch>}` for a selected aligned window in the last 24 hours. It resumes first-decision creation and link reconciliation. A missed decision window creates an abstention, never a historical forecast. Repeat requests preserve existing generations. Unknown fields, future/misaligned windows and unsupported assets are rejected. There is a 15s work budget.
- `GET /research/decision?asset=BTC&open_time=<UTC epoch>` returns the private decision/link envelopes, durable commitment times and generations, with `Cache-Control: no-store` and a 10s work budget. ETH and SOL use the same registry validation. This is an operator review surface. Future blinded MCP clients must use the scoped projection interface rather than operator review privileges.

For local operator access, use an authenticated Cloud Run proxy bound to localhost. Keep credentials in the normal Google Cloud login flow.

```sh
gcloud run services proxy crypto-history-collector \
  --project=gitunderstand --region=us-central1 --port=3007
```

In another shell, `curl -H 'Content-Type: application/json' --data '{}' http://127.0.0.1:3007/run-decisions` resumes the current cycle. Confirm the current opening and deadline before interpreting its output.

## Verification

77 Rust tests, 58 frontend tests and 21 Python tests passed, including cutoff/immutable-version guards, future-field/outcome independence across BTC/ETH/SOL, bad identity/book/rule/age rejection, late/tampered/earlier-candidate link rejection, generation-pinned reads, conditional first-write preservation, record size limits and caller-cutoff denial. Formatting, locked clippy and whitespace checks passed. Tests use synthetic fixtures and local mock servers. No frontend layout changed.

Live deployment and scheduled-cycle evidence are recorded in the session handoff after observation. A private deadline invocation already confirmed three abstentions and three ineligible links. Anonymous private routes and caller-cutoff overrides were rejected. A direct dashboard-service impersonation test was unavailable under existing operator permissions. Bucket and project IAM were inspected without adding token-mint permissions or enabling a project diagnostic API.

The first scheduled 21:00 UTC cycle was dispatched at 21:00:38 UTC according to Scheduler execution logs. All opening captures were outside the +30s gate. The runner saved three `snapshot_unavailable` abstentions, and all links were ineligible with `decision_abstained`. These records remain immutable. Minute-level scheduling/cold starts do not provide a second-zero SLA. This pilot reports those misses; resource limits and deadline rules were preserved. A subsequent supervised prospective cycle exercises the positive linkage path.

## Supervised prospective verification at 21:15 UTC

A supervised private capture request at +18s exercised the existing collector path on revision `crypto-history-collector-00011-hzp`. The first-attempt rules remained in force. All three assets have private decision and link objects. BTC and ETH each exhausted the bounded opening discovery attempts without a matching market/window; their forecasts abstained with null packets/probabilities, and the later books were ineligible. SOL had a valid opening book and a timing-eligible later link. This is positive-path evidence from a supervised live cycle, alongside the earlier scheduler-delayed negative-path test. It is not a claim that every scheduled cycle delivers a forecast.

| Asset | Decision cutoff UTC | Durable commitment UTC | Outcome |
|---|---|---|---|
| BTC | 21:15:22.724982720 | 21:15:22.931 | Abstained, opening unavailable |
| ETH | 21:15:22.724188955 | 21:15:22.928 | Abstained, opening unavailable |
| SOL | 21:15:22.734399265 | 21:15:22.925 | Forecast linked to later candidate |

SOL's opening source became durable at 21:15:22.675527 UTC, before its cutoff. The candidate request started at 21:15:45.039655011 and completed at 21:15:45.084514693 UTC, after commitment plus the one-second allowance. An independent verifier recomputed the input hash and midpoint, checked every input availability time, commitment/deadline, matching ticker and candidate ordering. It checked that abstentions expose no packet, probability or candidate, and repeated `POST /run-decisions` preserved all six object generations, creation times and payloads. The proof file is local `/private/tmp/ticket-c-live-proof.json`; raw records stay private.

Build `c0d3ccb9-8292-4ca5-89c9-4cefa94d1039` produced digest `sha256:d324dcd6fbddfeddc14a657d5fb16868a371a87eb30fba2ca0231620588ce900`. The collector serves 100% traffic on revision 00011. Source implementation is commit `38cc442`. GitHub Actions passed. Runtime identity, mounts, CPU/memory, concurrency, timeout, scaling and invocation IAM were compared with the prior release. The expected environment addition is `DECISION_BUCKET`. The public dashboard was not redeployed.

The remaining operational limitation is opening coverage under minute-level scheduler delivery and market-discovery delays. These failures are recorded as abstentions and must be counted in future performance reports. No predictive edge or fill rate has been established.
