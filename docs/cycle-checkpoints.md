# Opening and execution-candidate snapshots

Current automatic operation uses a prior-minute opening job and a separate quarter-hour follow-up job, with exact-window discovery and post-open metadata refresh. [Current timing contracts and verification](reliable-cycle-dispatch.md). The release notes below describe the original Ticket B implementation and its observed captures. New successful captures require a positive target and a valid two-sided research book. Older available records still require consumer validation.

Ticket B adds private `POST /collect-cycle` on the existing IAM-protected collector. Job `crypto-cycle-checkpoints` invokes it at UTC minutes 0/15/30/45, with the same scheduler identity, a 60-second attempt deadline and no automatic retries. The handler has a 50-second work deadline. It accepts starts only in seconds 0–54 of the actual current quarter hour and derives that opening server-side. Public dashboard visitors have no collection route.

One bounded invocation attempts both phases for BTC, ETH and SOL. Opening observations may start before +30s. The second phase waits until +45s and must start before +55s. Each phase captures three assets concurrently, with an eight-second opening budget and four-second follow-up budget per asset. Opening discovery permits five attempts, and follow-up discovery two, spaced one second apart when the correct active market is unavailable. Each attempt retains its request/receipt time, request success and matching-window status. The exact series, open, close and active/open state are checked before reading the book. A timeout, discovery failure, unavailable book or missed phase is stored explicitly. Provider response receipt after the phase deadline is late. These timing limits are pilot settings measured during verification, not scheduling guarantees.

The follow-up phase is **execution_candidate**, with null decision ID and decision commitment time. Ticket C will run a model and establish which candidate follows a committed decision plus routing allowance. No current record asserts a model decision or a fill. A late future decision cannot use an earlier candidate. The opening and follow-up prices can differ.

## Data contract

Records are immutable first attempts in the existing named Firestore database `crypto-prediction-lab`, project `gitunderstand`, collection `cycle_snapshots_v1`. IDs are `s<open_epoch>_<ASSET>_opening` and `s<open_epoch>_<ASSET>_execution_candidate`. They include schema version, intended/open/close time, overall request start and receipt, intended-time delay, phase, status/error, separate metadata and book request/receipt times, target/comparator, market identity/status and native top-ten YES/NO bids and sizes. Quantities and prices preserve decimal strings. An available capture may contain empty depth or a missing target, which downstream usability validation must reject. REST component times are local receipt times. Provider publication timestamps, tick/fee rules and guaranteed source freshness are unavailable in this schema. Requests are sequential and non-atomic.

Use no earlier than the later of actual component/overall receipt and Firestore document creation for durable research visibility. Never relabel an observation with its intended capture time. Late and failed records are preserved, rather than replaced by favorable later observations. The conditional-create transaction prevents overwrite across duplicate/overlapping requests. Per-phase reads skip existing records. Saved assets survive a later write failure. A duplicate may recover only absent asset/phase records within the live timing gate. A new invocation outside seconds 0–54 cannot backfill a past opening.

The previous minute-10 schema, route and schedule are retained. Existing replay continues to use minute-10 records. This release adds private storage only. No public display, MCP endpoint, prediction or execution is added by Ticket B.

## Operation and cost

Deploy the verified image to `crypto-history-collector` only, preserving identity, secrets, IAM, resources and the existing 60s service timeout. Run `zsh scripts/deploy-cycle-schedule.zsh` after deployment. Inspect an actual automatic scheduled cycle, read back all six documents, validate timestamps and tickers, check anonymous invocation rejection and duplicate preservation. Preserve existing history/CMC/minute-10 jobs. The collector's concurrency-one queue can delay this job or the CMC job at the same minute, so measure actual receipts and source gaps.

Planned new data is 576 small asset/phase documents per day, in addition to 288 minute-10 records. There are 96 bounded invocations/day, at most 50 request-seconds each (4,800 request-seconds/day), plus nominal six pre-read checks, six writes and twelve provider requests per cycle. Discovery retries can add up to fifteen provider requests per cycle. These counts exclude retries, duplicate calls, storage/indexing/egress and other jobs. No always-on process, paid feed or raised resource limit is introduced. The existing $40 monthly billing alert remains a target. Review actual usage before adding cadence or agents. Pause only `crypto-cycle-checkpoints` to stop the new recurring collection.

The active service settings and IAM should be compared before and after deployment. A partial capture remains visible, including a deadline that prevents saving the rest of a phase. Keep verification capture files under ignored `data/` or a temporary directory and publish only aggregate timing evidence.

## First scheduled observation

The first automatic 18:00 UTC cycle stored all six records. ETH and SOL opening books and all three +45s candidates were available. BTC opening discovery failed twice and was saved as unavailable at 18:00:18 UTC. That record remains immutable. This prompted the bounded opening retry extension and the delayed-discovery regression test. It does not supply a recovered historical BTC book. The bounded retry was verified on the next automatic cycle. The later 18:15 UTC full-cycle evidence below comes from revision `crypto-history-collector-00009-ddl`. The final timeout receipt guard is a subsequent release, covered by its own regression test and runtime checks.

## Successful scheduled verification

At 18:15 UTC (1:15 PM Eastern), all six records were available, with positive targets, two-sided uncrossed books and ten levels on each native bid side. Both phases matched the same actual ticker per asset. All component times were ordered, and each receipt preceded its immutable Firestore creation. ETH opening needed a second discovery attempt and succeeded within the opening deadline.

| Asset | Opening receipt UTC | Execution-candidate receipt UTC | Saved phases |
|---|---|---|---:|
| BTC | 2026-11-10T18:15:25.871359413Z | 2026-11-10T18:15:45.125302351Z | 2 |
| ETH | 2026-11-10T18:15:26.890947399Z | 2026-11-10T18:15:45.107507703Z | 2 |
| SOL | 2026-11-10T18:15:25.850799507Z | 2026-11-10T18:15:45.159615309Z | 2 |

The independent verifier ran at 18:15:49 UTC and invoked a duplicate inside the same eligible minute. The response was `captured` with zero new writes. Exact payloads, creation times and update times were unchanged for all six records. The first 18:00 UTC BTC opening failure was retained. Normal calendar-driven collection did not overwrite it.

Anonymous private POST returned 403, the public dashboard has no collection write path, and authenticated out-of-window invocation stored zero records. The existing minute-10 job was independently read back for BTC/ETH/SOL after the first collector update. Collector identity, secret mounts, CPU/memory, timeout, concurrency, min/max scale and IAM were compared with the original configuration. Existing history/CMC/minute-10 job configuration was retained. All 69 Rust tests, 58 frontend tests and 21 Python tests passed, as did formatting, clippy and GitHub Actions.

Final collector revision `crypto-history-collector-00010-dhx` uses build `60047978-e70b-4baa-bfc7-7e3018dcc3a1` and image digest `sha256:d324a2f35307dfc92842640c904099282a5eeecf4eaa8c952e1dd18f5ce7a73c`. It was deployed after the successful 18:15 cycle. The final change clears only an unfinished retry's receipt field and adds the timeout regression test. Authenticated health, anonymous denial, out-of-window no-write behavior, original-record preservation and configuration/IAM comparison passed on that revision.
