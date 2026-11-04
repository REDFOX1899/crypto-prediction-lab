# Crypto Prediction Lab

A learning-oriented, read-only Rust client for discovering Kalshi crypto prediction markets. This first milestone queries public market data; it cannot place orders and is not investment advice or a profitability claim. Not affiliated with Kalshi.

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
```

[Architecture](docs/architecture.md) | [Roadmap](docs/roadmap.md)

Market data is from [Kalshi's public API](https://docs.kalshi.com/api-reference/market/get-markets). Read each market's rules: Kalshi describes 15-minute markets as comparing averages from the minute before open and close, not a last exchange trade. Review Kalshi and reference-index terms before storing or redistributing their data. Historical access and API limits may change.

This project does not use real-money trading credentials or submit trades. Never commit secrets or personal data. Report security issues privately to the maintainer; do not post credentials in an issue.

## Local dashboard

```sh
cargo run -p dashboard
```

Open http://127.0.0.1:3000. The server listens only on your Mac and uses the same Rust client as the CLI. Without a key, it polls public BTC market and order-book snapshots every 10 seconds. An optional read-only key enables streaming. The chart begins collecting YES ask snapshots only while the page is open, resets when the market rolls over, and does not fill in missing history. Quotes and order-book levels come from separate requests and are not an atomic snapshot. An unavailable order book is reported separately. This is not an execution system.

Kalshi's order book reports YES and NO bids; it does not directly list asks. Quotes are indicative and do not guarantee fills. A registered account and API key are needed for Kalshi's authenticated WebSocket connection. Before publicly hosting market data, verify Kalshi and benchmark-provider redistribution terms, operating costs, and rate limits.

### Optional: live ticker stream

Create a production Ed25519 API key in Kalshi under Account & security → API Keys. Store the downloaded private key outside this repository with owner-only access. Never paste the key into chat or commit it. Set the key ID and private-key file path in your local shell, then start the dashboard:

```sh
read -r 'KALSHI_API_KEY_ID?Kalshi API key ID: '
export KALSHI_API_KEY_ID
export KALSHI_PRIVATE_KEY_PATH="$HOME/.config/crypto-prediction-lab/kalshi-private-key.pem"
cargo run -p dashboard
```

The example path must point to your own key file. The server signs the WebSocket handshake locally; the key never enters the browser. Production credentials are required for the production endpoint. When configured, a single shared stream provides BTC ticker quotes to local pages. The order book streams a full snapshot followed by ordered changes; a gap triggers reconnect and a fresh snapshot. The app falls back to a labeled public REST snapshot when streaming is unavailable. The app reconnects and rechecks market rollover; if disconnected, it falls back to public snapshots. The chart represents observed asks, not a complete history. Missing stream events cannot be recovered from the ticker stream. Never expose this local server or its credentials to the public without a separate deployment and security review.

### Reading the charts and depth

The BTC reference chart shows the CF Benchmarks BRTI index relayed by Kalshi and observations collected in the current page session. It is not the market's final settlement average. The dotted target line comes from the market's displayed target, when available. The YES contract chart is a separate series in cents. The reference feed uses the same read-only key on a separate connection; it can be unavailable independently of market quotes.

Order-book bars represent the quantity at each displayed price relative to the largest displayed quantity on that same side, not cumulative depth. A YES bid of 35¢ corresponds to a NO ask of 65¢ at the same quantity; likewise a NO bid of 40¢ implies a YES ask of 60¢. Quotes can change and do not include fees. Before publishing any of this data, verify Kalshi and CF Benchmarks data rights.

### Interactive charts

The dashboard includes TradingView Lightweight Charts 5.2.1, served locally. The chart supports scroll/pinch zoom, drag to pan, and a crosshair. Switch between a line and 1-minute candlesticks for either BRTI or the YES ask. Candles are assembled from values actually observed by the page: open is the first value in the minute, high and low are observed extremes, and close is the latest. The current candle and the first candle after opening or reconnecting may be partial. A YES-ask candle describes sampled quotes, not executed trades; BRTI candles describe reference-index observations, not Kalshi contract prices. Empty minutes are not filled. There is no historical backfill, so refresh clears the chart. WebSocket and browser update coalescing can omit ticks, so these are not authoritative exchange OHLC bars.

The bundled chart library is Apache-2.0 licensed; its license and notice are in `apps/dashboard/public/vendor/`. TradingView Lightweight Charts™ Copyright (с) 2025 TradingView, Inc. <https://www.tradingview.com/>.
