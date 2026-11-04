use anyhow::{Context, Result, bail};
use chrono::Utc;
use kalshi_client::{Client, current_market};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let series = args.next().unwrap_or_else(|| "KXBTC15M".into());
    if args.next().is_some()
        || !series.starts_with("KX")
        || !series
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        bail!("Usage: market-cli [SERIES_TICKER] (uppercase letters and digits)");
    }
    let client = Client::new()?;
    let status = client
        .exchange_status()
        .await
        .context("Could not fetch exchange status")?;
    let markets = client
        .open_markets(&series)
        .await
        .context("Could not fetch markets")?;
    let now = Utc::now();
    println!(
        "Kalshi exchange active: {} | trading active: {} (default exchange index)",
        status.exchange_active, status.trading_active
    );
    if let Some(market) = current_market(&markets, now) {
        let price = |p: Option<rust_decimal::Decimal>| {
            p.map(|p| format!("${p}"))
                .unwrap_or_else(|| "unavailable".into())
        };
        println!(
            "Series: {series}\nMarket: {}\n{}\nStatus: {}",
            market.ticker, market.title, market.status
        );
        if let Some(target) = &market.yes_sub_title {
            println!("YES: {target}");
        }
        println!(
            "YES bid: {} | ask: {}",
            price(market.yes_bid_dollars),
            price(market.yes_ask_dollars)
        );
        println!(
            " NO bid: {} | ask: {}",
            price(market.no_bid_dollars),
            price(market.no_ask_dollars)
        );
        println!(
            "Closes: {} ({} seconds remaining)",
            market.close_time,
            (market.close_time - now).num_seconds()
        );
        println!("Quotes are snapshots, not guaranteed fills.");
    } else {
        println!(
            "No currently active market found for {series} ({} open markets returned).",
            markets.len()
        );
    }
    Ok(())
}
