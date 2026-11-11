# Ticket E: private cutoff research MCP

This is a first-party, read-only adapter over `decision::packet`, the same pure opening projection used by the private numerical runner. It exposes BTC, ETH and SOL independently. It does not run a language model, start recurring inference, change collection schedules or implement paper fills. MCP connects tools to a host; the host still owns model selection, tool permissions, forecast validation and deadlines.

## Identity and visibility

The trusted operator calls IAM-protected `POST /research/mcp-scope` on the **collector**, with exactly `asset` and `open_time` (an aligned window in the last 24 hours). Only an existing timely committed forecast qualifies. The issuer reconstructs the opening packet at the saved original host cutoff and checks equality/hash against the committed decision. Missing, changed, late or abstained records cannot become retrospective forecasts.

Issuance creates a random 256-bit capability, valid for ten minutes. Only its SHA-256 digest appears in the private GCS path `mcp/scopes/v1/<digest>.json`. The stored scope contains one allowlisted packet, its run ID/hash, fixed audience, issuance and expiry. It contains no decision output, execution candidate, label, result or review record. The authenticated operator response includes the capability and scope for host setup; keep it private. Reissuing a scope gives a new access credential to the same original packet/hash. Issuance is on demand; no extra job or model call is enabled.

The **separate** `crypto-research-mcp` Cloud Run service has a dedicated `crypto-research-mcp` identity. Its sole data grant is bucket object-viewer access conditioned on this exact MCP prefix. It cannot list the bucket or read `decisions/`, `links/`, Firestore, provider secrets or collection routes. The public dashboard has neither this route nor access to the research bucket. Project administrators retain their existing administrative privileges.

Cloud Run requires an IAM identity token for the MCP service audience and invocation permission. A second `X-Research-Capability` header assigns the packet. This is a private first-party IAM deployment, not a third-party OAuth connector. No token-minting/delegation roles, API keys, project-wide grants or user-token forwarding to Kalshi are introduced. External customer OAuth onboarding is separate work. Give the **trusted MCP host** these credentials, never place operator credentials or scope selection inside model tool arguments. A blinded experiment must also withhold web search, live connectors, unrestricted database/clock tools and local file access from its agent.

The remote server reconstructs the scope through the pure opening projection again. Packet equality rejects extra fields and recomputes book features; source identity, immutable availability at cutoff, component ordering, rule, depth and 30-second receipt-age checks are retained. The cutoff stays frozen even after settlement or process restart. Advancing time, swapping asset/run/hash, selecting another phase or reading an arbitrary resource fails closed.

Fee-aware conditional buy levels are now available through `preview_paper_levels`; [policy, assumptions and live verification](fee-aware-paper-levels.md). Existing packet and scope semantics are retained. Eligible order prices remain disabled because as-of fee/tick rules and calibration are not yet verified.

Prospective exchange fee/tick proofs are now available through `get_market_rules`; [capture and verification contract](verified-market-rules.md). Older decisions remain unavailable for rule evidence. The original book packet and conditional price-policy v1 retain their existing semantics.

## MCP contract

The official Rust SDK is pinned to `rmcp 3.9.0`. It supports the current `2026-07-28` protocol and compatible earlier versions, including `2025-11-25`. HTTP is stateless Streamable HTTP at `/mcp`, with JSON responses, POST requests and GET 405 (no unsolicited event stream). No session or resumable-event identifiers are accepted. Local stdio uses the same handler and assigned scope, with newline-delimited JSON-RPC and clean stdout. [Official SDK](https://github.com/modelcontextprotocol/rust-sdk), [transport specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports), [current revision](https://modelcontextprotocol.io/docs/2026-07-28/learn/versioning).

| Interface | Accepted arguments | Result |
| --- | --- | --- |
| `get_cycle_packet` | Empty object only | Original immutable opening packet, source times/ID, target/rule, two native bid ladders and observed contract midpoint/spread |
| `get_book_depth` | Exact assigned `packet_hash`, side `YES` or `NO`, decimal-string `quantity` | Opposite-bid-derived ask ladder, covered/uncovered quantity, weighted ask, cost before fees and age at the frozen cutoff |
| `get_market_rules` | Empty object only | Exact committed exchange fee/tick proof and hash at the original cutoff, or explicit unavailable status |
| `preview_paper_levels` | Assigned packet hash, whole quantity 1–100, midpoint or explicit hypothetical forecast bounds | Conditional YES/NO ceilings, fee/allowance/depth comparison; no eligible order |
| `resources/read` | `research://methodology/v1` or `research://paper-policy/v1` | Versioned units, visibility policy, tool schemas and paper assumptions |

All data and methodology responses bind schema, run, asset, ticker, original cutoff and packet hash. Tool failures use a structured `data.error_code`, including `ASOF_DENIED`, `INVALID_QUANTITY` and `CALL_LIMIT`. Unknown routes/methods are absent or rejected. No `get_run_review`, outcome, performance, clock, collection or execution tool is exposed. The earlier Ticket E table is a proposal: packet/depth reads and conditional paper previews are implemented. Prospective exchange fee/tick evidence is captured separately; calibration, account fee applicability, prior-performance label availability and a fill ledger remain separate work.

Prices/quantities remain decimal strings. A purchase of YES uses `1 - NO bid`; NO uses `1 - YES bid`. Quantity is greater than zero and at most 10,000, with up to four decimal places. Calculations consume at most ten existing levels and report partial depth honestly. They exclude fees, queue position, slippage, future liquidity and fills. The packet includes the observed contract midpoint; this experiment is not blinded to prices and the midpoint is not a calibrated probability. CF reference history is unavailable here. An observed average never becomes an official settlement result.

Requests are limited to 8 KiB, scopes to 32 KiB, with packet/methodology responses verified under 32 KiB. Up to 128 unexpired capabilities are cached; each permits 32 data/discovery calls **per server process**, shared across stateless reconnects. Expiry is checked on every data call and survives restarts. The call counter resets after a process restart; it is an abuse bound, not a durable global spend quota. There is no paid inference or provider fetch behind these tools. A future paid model host must enforce persistent run/token/spend quotas separately. Remote lookup and handler work each have five-second limits; Cloud Run has a 15-second ceiling. All browser Origins and URL queries are rejected, and the SDK validates the configured Host allowlist. Cache-Control is `no-store`.

## Trusted host setup

Build with `cargo build --locked -p dashboard`. Prepare a real assigned packet without printing its contents or credentials:

```sh
python3 scripts/prepare_mcp_run.py --asset BTC --open-time <UTC-open-epoch> --output data/mcp-btc
```

Repeat for ETH/SOL. The helper writes private, owner-readable `scope.json` and `capability.json` files under the ignored data directory (or outside the repository). Do not pass the cloud CLI, operator identity or scope file as model tools.

For stdio, configure the host to launch the built `dashboard` executable with `DASHBOARD_ROLE=research-mcp-stdio` and `MCP_SCOPE_FILE` pointing at that scope file. There are no command arguments chosen by the agent. A ten-minute expiry requires a new operator-issued capability for later replay; original packet/cutoff/hash remain unchanged.

For remote use, configure `https://crypto-research-mcp-308289525742.us-central1.run.app/mcp` with the trusted host's service-audience IAM token in `Authorization: Bearer …` and the assigned capability in `X-Research-Capability`. Neither credential belongs in a URL, a transcript or Git. The host negotiates MCP and maps only the listed tools to the agent.

The official Inspector CLI and an independent host using the official TypeScript SDK are used for release verification; the Rust suite also hosts the same fixture over both protocol transports. Credentials and packet transcripts stay in temporary private files, and only aggregate pass/fail evidence goes into Git.

## Deployment and cost

`scripts/deploy-research-mcp.zsh <immutable-app-image-digest>` updates the existing collector image and deploys the separate private service. The dedicated identity receives only the conditional bucket-prefix reader grant. It has no collector invocation role or project role. Existing collector environment, resource limits, secrets, IAM and schedules are preserved. Public dashboard code/traffic are unchanged.

The new service uses request-based CPU, gen1, 0.25 vCPU, 256 MiB, concurrency one, min zero, max one, startup boost off and no paid provider. It is not an always-on collector. On-demand scope issuance adds a small immutable object and a source verification read, followed by bounded metadata/media reads on first access per instance. There is no recurring inference or automatic scope issuance. Build/storage/scheduler/hosting costs remain subject to the existing $40 monthly alert target, which is not a hard cap. Expired objects are inaccessible through MCP but remain in private storage for operator retention review; no broad deletion job is introduced.

## Ticket E release evidence (historical)

Deployed collector `crypto-history-collector-00013-2nv` and private MCP `crypto-research-mcp-00001-nhq` use build `01424721-7f5d-49a4-8c4f-bae5e4bb031f`, digest `sha256:663ea48a8df11626c560d02b1066d5c079c229c6f48dbefc1ee23c9a7b1eba45`, application source `acdcaf9`. GitHub production deployment `7701380357` is successful. The official Inspector read all three packets over both transports using the current protocol; an independent official SDK host passed 34 stdio and 45 remote checks per asset. Five invalid remote bundles were denied. The first automatic cycle after deployment produced three timely decisions and three eligible later links. The collector configuration/IAM and all application schedules were retained; project IAM was unchanged. [Aggregate proof](research-audits/private-mcp-2026-11-11.json). Exact checks, operational limits and private evidence locations are in `SESSION_HANDOFF.md`. Private tokens, complete packets, model inputs and cloud credentials never enter the report or repository.
