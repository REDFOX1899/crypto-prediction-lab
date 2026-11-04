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

Open http://127.0.0.1:3000. The server listens only on your Mac and uses the same Rust client as the CLI. It polls public BTC market and order-book snapshots every 10 seconds; no Kalshi account or API key is needed. The chart begins collecting YES ask snapshots only while the page is open, resets when the market rolls over, and does not fill in missing history. Quotes and order-book levels come from separate requests and are not an atomic snapshot. An unavailable order book is reported separately. This is not a live WebSocket feed or an execution system.

Kalshi's order book reports YES and NO bids; it does not directly list asks. Quotes are indicative and do not guarantee fills. A registered account and API key are needed for Kalshi's authenticated WebSocket connection. Before publicly hosting market data, verify Kalshi and benchmark-provider redistribution terms, operating costs, and rate limits.

### Optional: live ticker stream

Create a production Ed25519 API key in Kalshi under Account & security → API Keys. Store the downloaded private key outside this repository with owner-only access. Never paste the key into chat or commit it. Set the key ID and private-key file path in your local shell, then start the dashboard:

```sh
read -r 'KALSHI_API_KEY_ID?Kalshi API key ID: '
export KALSHI_API_KEY_ID
export KALSHI_PRIVATE_KEY_PATH="$HOME/.config/crypto-prediction-lab/kalshi-private-key.pem"
cargo run -p dashboard
```

The example path must point to your own key file. The server signs the WebSocket handshake locally; the key never enters the browser. Production credentials are required for the production endpoint. When configured, a single shared stream provides BTC ticker quotes to local pages. The order book remains a public REST snapshot, not a streamed order book. The app reconnects and rechecks market rollover; if disconnected, it falls back to public snapshots. The chart represents observed asks, not a complete history. Missing stream events cannot be recovered from the ticker stream. Never expose this local server or its credentials to the public without a separate deployment and security review.
