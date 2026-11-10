# Session handoff

Updated: 10 November 2026, America/New_York.

## Current work — quote study and Home redesign

The active branch is `codex/quote-outcome-desk`, based on `db41ac3` in the attached project checkout. The new UI and API are local. The study data was written to the existing named Firestore database in `gitunderstand`. The previous production release is recorded below.

- Home opens with an interactive settlement-rate and gross break-even chart. It covers three assets, three cutoffs and five quote bands. Each selection has exact counts, mean ask, percentage-point gap, UTC dates and coverage. The live watch adds a two-sided YES spread and retains its clock, reference chart and source freshness. The shared workspace navigation fits the header.
- November 7–9 UTC yielded 864 official binary outcomes. Minutes 5 and 10 each had 864 eligible quotes. Minute 14 had 221 BTC, 205 ETH and 217 SOL quotes. The fixed-minute candle is retrospective and quote size and fees were unavailable.
- Cloud verification used `python3 scripts/build_quote_study.py --end-day 2026-11-09 --publish --preview data/quote-study.json`. A transient HTTP 429 on the first publication left no manifest. With bounded backoff the atomic commit succeeded. Nine immutable partitions and one manifest were read back and compared with their expected contents. A later run against the same source data completed with **zero writes**. Anonymous Firestore GET returned 403. No IAM grants, subscriptions, scheduler changes or service deployment were needed.
- Run the local preview with `python3 scripts/preview-data-lab.py --port 3013`. It combines local UI and the verified aggregate with public read-only market feeds. The same aggregate is available in the native Rust server on loopback. Source captures and preview data are ignored by Git.
- Verification passed with `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace` (57 tests), `node --test apps/dashboard/tests/*.test.cjs` (56 tests), `python3 -m unittest discover -s scripts/tests -v` (13 tests), and `git diff --check`. The native public server returned the verified board, rejected POST/PATCH/PUT/DELETE and exposed no collection writes. The private collector returned 404 for the study route. Mock Firestore verification covered fixed-path reads, public field projection and cache reuse.
- Browser checks covered all 45 data selections, mobile/tablet/desktop widths of 320/390/768/1440 pixels, dark/light themes, workspace switching and the visible Single market book. Screenshots are under `/private/tmp/quote-study-*`.
- Review the local first screen before releasing the dashboard. The study is a manual three-day snapshot. Test the same preselected bands on later, untouched days and obtain prospective quotes, size and fees before evaluating executable returns. [Definitions and reproducible batch](quote-study.md)

## Previous deployed release

- Repository: `/Users/shantanu/Documents/ChatGPT/Crypto Prediction Lab`; https://github.com/REDFOX1899/crypto-prediction-lab. Do not switch to the older Desktop checkout.
- Branch `codex/blockworks-pilot`. Application commit **`a0da6c2`** is deployed and published to GitHub main, together with the deployment documentation. The owner explicitly approved the GitHub main push on 10 November 2026 after the earlier automatic approval rejection. GitHub main was verified as `f2e95bd` before publication; the push is a normal fast-forward, with no force or history rewrite.
- Public service `crypto-prediction-lab` now serves **100% `crypto-prediction-lab-00019-fab`**. Public URL: https://crypto-prediction-lab-308289525742.us-central1.run.app/?view=data.
- Successful Cloud Build: **`9160cf3d-fcd2-4550-9c2f-c14a3a468aa6`**. Image tag `before-outcome-20261110-r1`; pinned digest **`sha256:776f5d3005205291a53c5d2e3a3b517b18353042329d321842012ae01c6e9ab8`** in the dedicated dashboard Artifact Registry repository.
- Candidate was deployed with zero main-site traffic, passed real-data checks, then promoted. Temporary `before-outcome` tag was removed.
- Private collector remains **`crypto-history-collector-00005-msp`**. Existing IAM, service account, secret mounts, database, scheduler jobs, request billing, gen1, 0.25 CPU / 256 MiB, concurrency one, min zero/max one and 30-second dashboard timeout are unchanged. The only new runtime setting is `BLOCKWORKS_PUBLIC_DISPLAY=approved`. No new cloud resources, credentials, paid subscription or collector schedule were introduced.

## Authorized scope and completed work

The owner requested a price/volume history pilot plus “Before this outcome” in Data Lab, explicitly requested production deployment, and separately confirmed that the Blockworks agreement permits public BTC/ETH/SOL price/volume display. This confirmation is recorded in the pilot manifest. Model-use and long-term retention rights remain separate questions.

- **One-time seven-day pilot:** 2,016 valid five-minute candles per BTC/ETH/SOL, **6,048 total**, zero rejected points. Requested provider timestamp range `[2026-11-03T05:10:00Z, 2026-11-10T05:10:00Z)`; downloaded `2026-11-10T05:15:07Z`. The pilot used three sequential API requests: accessible catalog, canonical IDs, batched history. No retries or recurring collection. Actual credit charge/balance is unverified.
- Ignored local file: `data/blockworks-seven-day-20261110.json` (891,993 bytes). The original one-day file remains ignored. No snapshot/capture or credential was committed or included in the build upload.
- `scripts/publish-blockworks-pilot.py` validates and atomically publishes three immutable normalized asset documents plus the latest permission manifest into the existing named Firestore database `crypto-prediction-lab`, collection `blockworks_pilots`. Current asset IDs are `p1794287707_BTC`, `p1794287707_ETH`, `p1794287707_SOL`; manifest `latest`. Existing operator identity performs these four writes. The dashboard uses its existing viewer identity and has no Blockworks key. Conditional writes prevent replacing immutable captures or racing a newer manifest.
- `apps/dashboard/src/blockworks.rs` supports one/seven-day explicit pilots and licensed saved-data display. Provider point_schema determines metric order; canonical assets, units, timestamps, bounds, OHLC and duplicates are validated. Public reads require the approval flag and approved manifest, cache for ten minutes, and display at most the final day of five-minute candles plus full-pilot counts. The seven-day capture is not returned wholesale through the chart route. Local snapshots still require loopback.
- The Blockworks Data Lab panel includes asset switching, coverage, close-price chart, relative volume, exact OHLC/volume disclosure, timestamps and source attribution. It states this is a saved pilot, not live collection. Observation exports still exclude Blockworks.
- New `GET /api/research/before?asset=BTC&ticker=<archived-ticker>&minute=10`: supported assets, matching ticker prefixes and cutoff values 0/5/10/14 only; unknown and duplicate parameters rejected. Uses recent completed archived windows, twelve-second deadline and shared caches. It cannot collect provider data or write to storage. POST is rejected.
- New Data Lab inspector opens from any outcome cell. Choose an asset and cutoff at open, +5m, +10m or one minute before close. Shows historical target text with provenance, eligible saved CMC sample, preceding price/volume intervals, 5m/15m/60m moves, mean absolute five-minute move, relative fifteen-minute volume and eligible completed minute YES ask candles.
- CF reference/distance and historical book depth/spread are explicitly unavailable because they were not archived. No aggregate price substitutes for CF. Outcome, expiration value and hindsight best-entry are excluded from before-response/calculations. Tests prove changing the winner/settlement cannot change features.
- New reads cancel on cutoff/asset/window/workspace changes; late replies must match request generation and exact selection. Browser cache is limited to 32 before-selections.
- **Existing pages preserved:** Home, Single market and Combo lab markup outside Data Lab, established frontend scripts and shared focus styles are unchanged. Source comparison permitted only Data Lab asset version changes. Production browser checks confirmed live Home quotes/reference values, Single market reference history and always-visible live book, and Combo lab current quotes.

## Timing and data limitations

- Blockworks and Kalshi history were retrieved retrospectively. The inspector filters completed intervals before cutoff but does not claim they were originally available then. Download times and unverified historical availability are visible.
- Provider timestamp + five minutes must be at/before the cutoff's last completed five-minute boundary. Exact bucket start/end convention remains unverified; the conservative rule is disclosed.
- Saved CMC collection/source timestamps must both precede cutoff and be within eleven minutes. First receipt was not separately archived, so no stronger point-in-time claim is made.
- Missing candles/volume are preserved; zero is distinct from missing. Multi-candle metrics require complete consecutive intervals. Volume denomination is still labeled unverified because response metadata omitted it.
- The pilot is static. Windows after its ending timestamp can have partial/missing Blockworks context. Browser refreshes only read saved data and cannot spend Blockworks credits. No model, forecasting accuracy or profitability claim is introduced.
- Coinbase remains on hold; direct CF licensing remains pending. Private RFQs, order execution, transfers and payments remain disabled.

## Exact verification

- `cargo fmt --check` passed.
- `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` passed.
- `cargo test --offline --locked --workspace` passed: **55 tests** (51 dashboard, four client), with local mock-server permission.
- `node --test apps/dashboard/tests/*.test.cjs` passed: **53 tests**, including exact cutoff selection, cancellation and ignored late responses.
- `python3 -m unittest discover -s scripts/tests -v` passed: **10 tests**, including publisher scope/conditional writes, stripped extra fields and missing-vs-zero volume.
- Publisher dry run validated four documents and a 965,688-byte request; actual atomic publication succeeded.
- Build upload allowlist was inspected: only Cargo files, Docker files, apps and crates; no local data, docs, secrets or credentials.
- Candidate and final public URL each passed **39 HTTP checks**: seven-day pilot metadata, bounded day display, all three assets at all four cutoffs, interval-end filtering, absent outcome/settlement/hindsight fields, CMC timestamp restrictions, invalid asset/ticker/cutoff/extra/duplicate parameters, rejected writes, existing pages/assets/history/feed routes.
- Browser verified desktop CSS width 1280 and mobile 390, light/dark, actual populated windows, all asset controls, cutoff changes and exact-value tables without document overflow or console errors. Final public inspector was visibly confirmed.
- Cloud service configuration and IAM bindings were compared before/after. Identity, mounts, resources, concurrency, timeout, scaling and existing environment were preserved. Collector revision was read back unchanged.
- `git diff --check` passed. Build and API checks contain no credentials.

## Evidence and next step

- Detailed semantics: `docs/before-outcome.md`. Earlier evaluation, credits and commands: `docs/blockworks-data-plan.md`.
- Public screenshot: `/private/tmp/before-outcome-production.jpg`.
- Temporary reports: `/private/tmp/crypto-before-candidate-checks.json`, `/private/tmp/crypto-before-live-checks.json`; each records 39 passing checks. Service/IAM snapshots and build metadata are also under `/private/tmp/crypto-before-*` and are not committed.
- Next: check the actual Blockworks credit charge before another fetch. Any recurring collection, expanded retention or model use requires its own concrete decision. No redeployment is needed for documentation-only updates.
- The owner’s explicit GitHub publication approval has been applied. Future documentation-only changes do not require redeploying the application. An earlier local preview at port 3005 may still show the old one-day-only configuration; use the public URL for this completed release.
