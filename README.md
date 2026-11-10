# Crypto Prediction Lab

A read-only Rust dashboard and client for BTC, ETH and SOL 15-minute Kalshi markets. Research live reference prices, contract quotes, liquidity and settled outcomes. It cannot place orders and makes no profitability claim. Not affiliated with Kalshi.

Public dashboard: https://crypto-prediction-lab-308289525742.us-central1.run.app/

New chat or contributor? Start with [project memory](docs/PROJECT_MEMORY.md), [latest handoff](docs/SESSION_HANDOFF.md) and [session guide](docs/SESSION_GUIDE.md). Repository working rules are in [AGENTS.md](AGENTS.md). Use the folder containing this Cargo workspace as your Codex project directory.

The application includes a live homepage, a shared single-market workspace and a combo research view. Features include light/dark themes, observed-price SVG plots, interactive charts, an always-visible order book, final-minute coverage and a Firestore settlement archive. Private combo RFQs and trading remain disabled.

[UI and data semantics](docs/focus-ui-design.md) · [History archive](docs/history-archive.md) · [Cost controls](docs/cost-controls.md) · [Data-engineering plan](docs/data-provider-engineering-plan.md)

CoinMarketCap adds a separately labeled market-context section with shared scheduled collection, normalized Firestore observations, and a collector-only API key. See [CMC context and quotas](docs/coinmarketcap-context.md). Coinbase remains on hold.

Direct CF Benchmarks and Coinbase integrations and durable high-frequency event capture are proposed in the data-engineering plan; they are not yet implemented.

## Quote and outcome study

The public dashboard compares archived YES asks with official results at minutes 5, 10 and 14. Choose an asset and price band to see the observed settlement rate, the break-even rate before fees and the eligible sample count. The first study covers November 7–9 UTC and is saved in the app's named Firestore database. [Chart definitions, storage and refresh commands](docs/quote-study.md)

## Data Lab

The new `/?view=data` workspace compares aligned BTC/ETH/SOL official outcomes, records coverage, and charts shared CMC returns and correlations with sample counts. Select an outcome cell to inspect the contract summaries, then choose an asset for its stored one-minute YES-ask OHLC. Exact values and missing minutes remain visible. Export observations as JSON for inspection; the export is not a point-in-time training dataset.

`GET /api/context/history` reads up to 288 saved CMC samples from the last 24 hours, with a five-minute shared cache. It does not poll CMC or write to storage. `GET /api/archive/window?asset=BTC&ticker=...` reads one validated archived market, returns normalized quote candles only, and caches at most 32 windows for five minutes. These routes use the existing configured archive identity. They are deployed on the public dashboard.

For a visual preview using the currently deployed public outcomes and short CMC history, without cloud credentials:

```sh
python3 scripts/preview-data-lab.py
```

Open http://127.0.0.1:3002/?view=data. This loopback helper serves local assets, the local quote study when present and read-only responses from the deployed dashboard, including the existing archive and cutoff inspectors.

Data Lab also includes a saved Blockworks BTC/ETH/SOL price-and-volume pilot and a “Before this outcome” inspector. Select an outcome cell to inspect completed intervals before a chosen cutoff, with retrospective downloads labeled separately from saved collection timestamps. CF reference and order-book history are marked unavailable when not archived. The one-time seven-day pilot is bounded; public refreshes cannot invoke provider collection, and exports exclude Blockworks. See [timing, deployment and data provenance](docs/before-outcome.md) and [pilot commands and credits](docs/blockworks-data-plan.md).

## Run

Install stable Rust using [rustup](https://rustup.rs/), then:

```sh
cargo run -p market-cli -- KXBTC15M
```

Pass another verified crypto series ticker to inspect it. The CLI checks the series category with Kalshi and accepts crypto series only. If no currently active window exists, the CLI says so. Quotes can be missing or stale and do not guarantee a fill. Trading status is for the default exchange index.

## Verify

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
node --test apps/dashboard/tests/*.test.cjs
```

[Architecture](docs/architecture.md) | [Roadmap](docs/roadmap.md)

Market data is from [Kalshi's public API](https://docs.kalshi.com/api-reference/market/get-markets). Read each market's rules: Kalshi describes 15-minute markets as comparing averages from the minute before open and close, not a last exchange trade. Review Kalshi and reference-index terms before storing or redistributing their data. Historical access and API limits may change.

This project does not use real-money trading credentials or submit trades. Never commit secrets or personal data. Report security issues privately to the maintainer; do not post credentials in an issue.

## Local dashboard

```sh
cargo run -p dashboard
```

Open http://127.0.0.1:3000. The server listens only on your Mac and uses the same Rust client as the CLI. Without a key, the detailed workspace uses public market and order-book snapshots for the selected BTC, ETH or SOL contract. An optional read-only key enables streaming. The chart begins collecting YES ask snapshots only while the page is open, resets when the market rolls over, and does not fill in missing history. Quotes and order-book levels come from separate requests and are not an atomic snapshot. An unavailable order book is reported separately. This is not an execution system.

Kalshi's order book reports YES and NO bids; it does not directly list asks. Quotes are indicative and do not guarantee fills. A registered account and API key are needed for Kalshi's authenticated WebSocket connection. Before publicly hosting market data, verify Kalshi and benchmark-provider redistribution terms, operating costs, and rate limits.

### Optional: live ticker stream

Create a production Ed25519 API key in Kalshi under Account & security → API Keys. Store the downloaded private key outside this repository with owner-only access. Never paste the key into chat or commit it. Set the key ID and private-key file path in your local shell, then start the dashboard:

```sh
read -r 'KALSHI_API_KEY_ID?Kalshi API key ID: '
export KALSHI_API_KEY_ID
export KALSHI_PRIVATE_KEY_PATH="$HOME/.config/crypto-prediction-lab/kalshi-private-key.pem"
cargo run -p dashboard
```

The example path must point to your own key file. The server signs the WebSocket handshake locally; the key never enters the browser. Production credentials are required for the production endpoint. When configured, per-asset streams shared by visitors provide ticker and order-book updates for BTC, ETH and SOL. The order book streams a full snapshot followed by ordered changes; a gap triggers reconnect and a fresh snapshot. The app falls back to a labeled public REST snapshot when streaming is unavailable. The app reconnects and rechecks market rollover; if disconnected, it falls back to public snapshots. The chart represents observed asks, not a complete history. Missing stream events cannot be recovered from the ticker stream. Never expose this local server or its credentials to the public without a separate deployment and security review.

### Reading the charts and depth

The BTC reference chart shows CF Benchmarks BRTI history, when the account is entitled, together with live values relayed by Kalshi. It is not the market's final settlement average. The dotted target line comes from the market's displayed target, when available. The YES contract chart is a separate series in cents using Kalshi YES-ask candles and live sampled asks. The reference feed uses the same read-only key on a separate connection; it can be unavailable independently of market quotes.

Order-book bars represent the quantity at each displayed price relative to the largest displayed quantity on that same side, not cumulative depth. A YES bid of 35¢ corresponds to a NO ask of 65¢ at the same quantity; likewise a NO bid of 40¢ implies a YES ask of 60¢. Quotes can change and do not include fees. Before publishing any of this data, verify Kalshi and CF Benchmarks data rights.

### Interactive charts

The dashboard includes TradingView Lightweight Charts 5.2.1, served locally. The chart supports scroll/pinch zoom, drag to pan, and a crosshair. Switch between a line and 1-minute candlesticks for either BRTI or the YES ask. Candles are assembled from values actually observed by the page: open is the first value in the minute, high and low are observed extremes, and close is the latest. The current candle and the first candle after opening or reconnecting may be partial. A YES-ask candle describes sampled quotes, not executed trades; BRTI candles describe reference-index observations, not Kalshi contract prices. Empty minutes are not filled. The dashboard requests completed YES-ask candles for the active market from Kalshi, and requests the two most recent completed BRTI hours from the CF Benchmarks passthrough when entitled. CF history may lag, be incomplete, or unavailable; gaps are left blank and shown in the status. Live observations are collected after opening the page and are not persisted. WebSocket and browser update coalescing can omit ticks, so these are not authoritative exchange OHLC bars.

The bundled chart library is Apache-2.0 licensed; its license and notice are in `apps/dashboard/public/vendor/`. TradingView Lightweight Charts™ Copyright (с) 2025 TradingView, Inc. <https://www.tradingview.com/>.

### Historical data details

BRTI history comes from Kalshi’s authenticated CF Benchmarks passthrough (`/cfbenchmarks/history/values`), requesting two completed UTC-hour windows and aggregating returned index ticks into one-minute OHLC bars. Data can be delayed or unavailable, and this endpoint may require account entitlement. The server bounds response size and caches results for five minutes. It never sends key material to the browser. The YES chart uses Kalshi’s public 1-minute YES-ask candlesticks for the current market only, followed by locally observed live quote values. Past candles end at their stated time; the chart places them at the minute start. Null, invalid and unfinished provider candles are omitted. The current live candle may be incomplete. Settled contracts and available minute candles are archived separately; complete tick-level replay and backtesting remain future work. Verify data rights before storage or public redistribution.

### Shared asset workspace

The selector enables BTC, ETH and SOL 15-minute markets. One registry supplies the supported symbols, market series and reference indices. Select an asset to change the shared chart, live quotes, order book and history. The selected symbol is kept in the page URL, for example `/?asset=ETH`. Unsupported API assets are rejected. History and live reference access can differ by asset and account entitlement. Per-asset workers and caches are shared by dashboard tabs. Browser requests from an older selection are ignored after switching.

For a second local preview server: `DASHBOARD_PORT=3001 cargo run -p dashboard`.

## Public hosting

The deployed public pilot uses shared two-second browser polling; local mode supports SSE. Reference and market connections remain server-side. A separate private scheduled collector writes settled-contract history. Deployment scripts refer to dedicated Secret Manager resources and contain no private key material. See [deployment](docs/public-deployment.md), [production pilot](docs/production-pilot.md) and [combo semantics](docs/combo-lab.md).

The current live feed buffers are bounded in memory. The archive is not a complete tick/order-book capture, and the observed final-minute average is not an official settlement result. Existing hosting limits and budget alerts reduce cost without guaranteeing an absolute monthly bill cap.
