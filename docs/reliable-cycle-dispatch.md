# Opening timing and exact contract discovery

The opening path now starts from a Scheduler trigger in the minute before each quarter hour. Early calls return a retryable 503 with a five-second delay. The job uses bounded five-second retries. A working handler accepts the call from 15 seconds before the boundary, discovers the exact upcoming contracts, and waits inside its bounded request until +5s. It captures only post-open metadata and books. The existing source receipt gate at +30s and durable decision commitment gate at +40s remain in force.

A separate private follow-up job runs at quarter-hour minutes. Calls before +5s retry. An accepted call waits until +45s, captures the later books and links the committed decisions. The follow-up also creates explicit deadline abstentions for any missing opening decisions. All handlers keep the existing 50s work budget inside the 60s service timeout. The Cloud Run resource settings and existing invocation identity remain the same.

## Why the old path missed inputs

A bounded read-only probe across the 21:30 UTC boundary found no matching current window in the old status-filtered open-market query at +5s or +15s for BTC, ETH or SOL. The window-specific query found the matching contracts before open, and direct reads matched them at +5s. BTC's numeric target was absent on the first direct read and present by +15s. The probe kept only identity/timing/readiness diagnostics locally.

The new client queries by series and the narrow expected close-time range, with no incompatible status filter, then refreshes the discovered ticker directly. Requests include `Cache-Control: no-cache`. Preflight saves only identifiers. Targets and depth used by the runner are read after open with actual component times. The source waits for the supported comparator, positive target, two-sided valid depth and uncrossed prices. It retries within a 24s maximum episode and the +28s source work boundary, retaining diagnostic attempts. A missing target or empty book cannot prematurely become a successful opening snapshot. Provider event time remains unknown.

## Scheduler contracts

- `crypto-cycle-checkpoints` uses UTC `14,29,44,59 * * * *` and private `POST /collect-opening`. It uses retry count four with a five-second backoff and a 100s retry duration. Scheduler can exceed the count until both configured limits are reached.
- `crypto-cycle-followup` uses UTC `0,15,30,45 * * * *` and private `POST /collect-followup`. It uses retry count two with the same backoff/duration. Both limits govern exhaustion.
- Both use the existing dedicated scheduler identity and a 60s attempt deadline. `scripts/deploy-cycle-schedule.zsh` applies only these two jobs. History, minute-10 and CMC schedules retain their configuration.

The original Scheduler schedule timestamp binds retries to the intended quarter. A stale retry cannot capture a later quarter. A missing timestamp falls back to the current nearby boundary for private operator use. Requests outside the admitted window stop without moving the cutoff. The first accepted dispatch is saved conditionally in `cycle_dispatch_v1/s<open>_<phase>` with receipt/schedule times and discovered IDs. It contains no future target/price observations.

All source snapshots, decisions and links keep their first-write identities. The manual `POST /collect-cycle` route retains its response counts and can still run both phases. Automated verification uses the two Scheduler jobs and performs no manual capture or warming.

The rejected Cloud Tasks design was removed before execution. This design uses Scheduler retry control and the existing invocation identity. No task queue, API enablement or identity-delegation grant is required.

## Cost and behavior

Early 503s are deliberate admission responses and are retried by Scheduler. They perform no collection or model writes. This adds one Scheduler job and bounded requests to the existing service. There is no always-on worker or raised CPU/memory/instance limit. The $40 monthly alert remains a target. The existing deterministic midpoint strategy and private storage remain in use. Service/provider outages can still cause explicit abstentions. The observation gates are unchanged.

Tests cover original-quarter binding through retries, early/late admission, future window lookup without the stale status filter, direct metadata refresh, delayed targets, empty-book retries, partial receipt timestamps and the existing cutoff/hash/commit/link contracts. The required checks passed with 82 Rust tests, 58 frontend tests and 21 Python tests, plus fmt, locked clippy and whitespace checks. Live cycle results and deployment details follow after observation.
