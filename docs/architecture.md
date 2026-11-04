# Architecture

One Rust application serves a shared market workspace for BTC, ETH and SOL. `assets.rs` is the crypto-only allowlist: it maps each asset to a Kalshi 15-minute series and its CF Benchmarks index. API query parameters deserialize into an `Asset` enum; unsupported assets fail with HTTP 400 before upstream requests.

A shared `Client` reuses HTTP connections. Each asset has independent ticker/order-book stream state, a reference stream, and a history cache. All dashboard tabs for the same asset consume the same stream state. `Arc` shares ownership safely; Tokio watch channels retain the latest display state with bounded memory. Each asset has its own rollover and reconnect handling. Data collection for durable replay would need a separate loss-aware path before UI coalescing.

The browser uses the server-provided registry for its selector. Switching assets closes the previous event streams and clears the chart and quote state. A selection generation guards asynchronous responses, preventing a delayed request from a previously selected asset from updating the current workspace.

Kalshi books contain YES and NO bids. Decimal-safe Rust values maintain aggregate price levels, and subscription sequence gaps trigger a reconnect and fresh snapshot. Snapshot fallbacks are labeled. Historical contract candles remain distinct from reference-index history and sampled live observations. History can be delayed or unavailable depending on account entitlement.

The server binds to loopback only (port 3000 by default; `DASHBOARD_PORT` is available for local testing). Credentials are read only by the server and never included in API responses or browser state. The application has no order or transfer operations. Review data rights, operating budget and deployment controls before public hosting.
