# Verified pre-cutoff market rule capture

This release captures exchange fee configuration and dynamic tick bands for BTC, ETH and SOL during the existing opening capture. It uses only Kalshi's public REST endpoints. No new schedule, identity, IAM role, paid provider or always-on worker is added. Existing opening/decision gates stay +30/+40 seconds.

## Sources and capture

The exact market metadata response supplies `event_ticker`, binary market type, notional dollars, `price_level_structure`, `price_ranges` and any fee-waiver expiry. Tick logic uses the actual decimal bands, not the structure name. After a usable two-sided opening book, the collector makes three parallel, bounded requests once per asset:

- `/series/{series}`: current fee type and multiplier.
- `/series/fee_changes?series_ticker=…&show_historical=true`: announced series fee changes.
- `/events/fee_changes?event_ticker=…&limit=100`: complete event-filtered override history, including clears and per-field fallback.

[Series metadata](https://docs.kalshi.com/api-reference/market/get-series), [series changes](https://docs.kalshi.com/api-reference/exchange/get-series-fee-changes), [event overrides](https://docs.kalshi.com/api-reference/events/get-event-fee-changes), [dynamic price grids](https://docs.kalshi.com/getting_started/fixed_point_migration).

Each source has actual request-start and receipt timestamps. Cache-Control is no-cache. Responses are limited to 16 KiB each, normalized histories to 32 rows and stored rule evidence to 8 KiB. A nonempty event cursor, excess history, unsupported identity or missing required fields makes rules unavailable. The collector does not treat a partial page as proof that no override exists. Extra provider fields, including outcomes/child markets/titles, are discarded before persistence.

The combined rule request has at most two seconds and respects the remaining opening work budget, reserving time to preserve a usable book. It never repeats because of an earlier empty or failed book. A slow/unavailable rule source is recorded as unavailable while the book remains usable. There is no attempt to fabricate or backfill earlier fee records. The follow-up capture performs no rule fetch.

## Storage and cutoff

`observation.market_rules` is saved inside the existing first-write-only opening document in named Firestore database `crypto-prediction-lab`, collection `cycle_snapshots_v1`, ID `s<open>_<ASSET>_opening`. The original schema/version and book projection remain compatible. There is no separate source write or new Firestore collection. Request/receipt times are not publication times; provider internal publication age remains unknown.

The runner fixes its cutoff after collection/persistence, before reading source documents. It builds the existing book packet unchanged, then a pure rule projection from that same immutable opening document. Both its creation and update times must be at or before cutoff and equal. Every component receipt must precede overall receipt, durable availability and cutoff. Market metadata times must match the assigned book packet exactly. The projection validates asset/ticker/event/series, $1 binary notional, fee type/multiplier ranges and the full tick grid. Saved bids and complement asks must lie on that grid.

The effective event override is the latest entry at or before cutoff, with independently cleared type/multiplier falling back to the series. Future-effective fee rows are removed before the agent-visible proof/hash. A series transition during the non-atomic observation interval, inconsistent current/history values, malformed/ambiguous history, an active unsupported waiver, invalid grid or late receipt gives an explicit unavailable reason. Metadata is an observation of the exchange configuration, not a claim that unobserved changes cannot occur.

The first committed private decision stores `market_rules` and `rules_hash` alongside the original `packet`/`packet_hash`, in the existing GCS decision object. The old book hash/probability benchmark does not change. Missing or invalid rules do not promote into verified evidence. Conditional writes preserve the first decision and source payload.

## Private MCP

`get_market_rules` accepts **only an empty object** and returns the rules committed with the assigned decision, their SHA-256 and the usual run/asset/ticker/cutoff/book-hash envelope. Scope issuance reconstructs the same pure projection at the original saved cutoff and compares it with the committed rule value/hash. New optional scope fields carry the frozen proof and hash; the MCP validates them again without database access. Future receipts, effective fee rows, altered derived values or extra outcome fields are denied, even after recomputing an attacker-supplied hash.

Old scopes and decisions remain supported. An earlier decision without committed rules returns `NO_RULES_COMMITTED_AT_CUTOFF`, null rules/hash and unavailable status. Scope issuance cannot attach newly fetched metadata to an old run. The MCP runtime still has only its conditional GCS MCP-prefix reader grant; it cannot read raw Firestore, decision/link objects, official results or provider APIs. Public visitors cannot discover the tools or invoke collection.

What is verified is the observed **exchange configuration**: fee type, multiplier, event override basis, notional and price bands. Account discounts, FCM charges, the member's rounding classification, calibrated probability bounds and future fill behavior are not verified. `preview_paper_levels` remains the reproducible v1 conditional calculator with its explicitly assumed fee/grid policy; this release exposes evidence for a later policy version rather than silently changing those historical assumptions. It creates no eligible orders, trades or fills.

## Cost and operation

At most nine extra public source requests per successful three-asset opening cycle: up to 864/day at 96 cycles. Rule fetches add no extra Firestore documents/writes; they add bounded bytes to existing opening records, committed decisions and on-demand scopes. Retries/duplicate infrastructure reads, build and storage charges still apply. The $40 monthly alert is a target, not a hard cap. Resource limits, schedules and paid-provider settings are retained.

`scripts/deploy-market-rules.zsh <immutable-app-image-digest>` updates only the two private images, MCP first for compatible scope rollout, then collector. Existing IAM, identities, mounts, environment, limits and schedules are retained. The public dashboard is not redeployed. Verify an automatic prospective cycle, read back source and commitment timestamps, verify all three assets over MCP and test duplicate preservation. Raw captures/tokens stay private; only aggregate evidence belongs in Git.

## Verification

Unit/integration tests cover actual public-source request normalization, event overrides/clears/partial inheritance, series transitions, incomplete histories, invalid grids, wrong identities, late/modified evidence, future-effective values and outcome mutation. They prove actual source capture can project into a committed cutoff packet, and slow rule sources preserve usable books. Existing MCP tests cover old scopes; new scope tests reject future/altered rule proofs. Independent SDK-host and deployed evidence will be recorded in the handoff after live verification.
