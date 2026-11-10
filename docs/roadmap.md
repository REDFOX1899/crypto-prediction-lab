# Roadmap

Updated 9 November 2026. See [project memory](PROJECT_MEMORY.md) for the current implementation; the initial CLI-only roadmap is superseded.

## Already built

- Rust client/CLI and a public dashboard with Home, Single market and Combo lab.
- BTC/ETH/SOL 15-minute asset selection, live ticker/book/reference observations, target distance and final-minute coverage.
- Interactive line/candlestick charts, an always-visible order book, source freshness and light/dark themes.
- Private scheduled settlement archival and public outcome history in a dedicated Firestore database.
- Scheduled CoinMarketCap collection with shared cached context, quota controls and sampled history.

## Suggested next features — not yet selected or approved

1. **Join broader context to completed windows.** Show the latest CMC observation actually available at each window's open/close. Preserve source timestamps and missing coverage; do not use later observations or claim retrospective predictive accuracy.
2. **Add honest replay of recorded observations.** Let users explore a completed 15-minute window, starting with the data already retained. Label minute candles, five-minute samples, missing records and hindsight analysis distinctly. Full tick/order-book replay needs a separate permitted, durable recording pipeline.
3. **Improve operational visibility and user feedback.** Expose understandable source-health states, verify missed collection recovery, and gather feedback from a small set of users before increasing infrastructure or adding subscriptions.

Coinbase remains on hold. Direct CF access depends on provider access/licensing. Nautilus research adapters and paper simulation remain longer-term proposals; no real-money execution or payments are enabled.
