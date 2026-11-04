# Crypto Prediction Lab

A learning-oriented, read-only Rust client for discovering Kalshi crypto prediction markets. This first milestone queries public market data; it cannot place orders and is not investment advice or a profitability claim. Not affiliated with Kalshi.

## Run

Install stable Rust using [rustup](https://rustup.rs/), then:

```sh
cargo run -p market-cli -- KXBTC15M
```

Pass another verified crypto series ticker to inspect it. The caller is responsible for choosing a crypto series; the API itself is not restricted to crypto. If no currently active window exists, the CLI says so. Quotes can be missing or stale and do not guarantee a fill. Trading status is for the default exchange index.

## Verify

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

[Architecture](docs/architecture.md) | [Roadmap](docs/roadmap.md)

Market data is from [Kalshi's public API](https://docs.kalshi.com/api-reference/market/get-markets). Read each market's rules: Kalshi describes 15-minute markets as comparing averages from the minute before open and close, not a last exchange trade. Review Kalshi and reference-index terms before storing or redistributing their data. Historical access and API limits may change.

This project does not use real-money trading credentials or submit trades. Never commit secrets or personal data. Report security issues privately to the maintainer; do not post credentials in an issue.
