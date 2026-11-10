//! Read-only access to Kalshi's public market endpoints.
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::Duration};
use thiserror::Error;

const API: &str = "https://external-api.kalshi.com/trade-api/v2";

#[derive(Debug, Error)]
pub enum Error {
    #[error("Kalshi request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Kalshi returned a repeated pagination cursor")]
    RepeatedCursor,
}

#[derive(Debug, Deserialize)]
struct SeriesResponse {
    series: Series,
}

#[derive(Debug, Deserialize)]
pub struct Series {
    pub category: String,
}

#[derive(Debug, Deserialize)]
pub struct ExchangeStatus {
    pub exchange_active: bool,
    pub trading_active: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Market {
    pub ticker: String,
    pub title: String,
    pub status: String,
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
    pub yes_bid_dollars: Option<Decimal>,
    pub yes_ask_dollars: Option<Decimal>,
    pub no_bid_dollars: Option<Decimal>,
    pub no_ask_dollars: Option<Decimal>,
    pub yes_sub_title: Option<String>,
    pub floor_strike: Option<Decimal>,
    pub strike_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrderBookResponse {
    orderbook_fp: OrderBook,
}

/// Kalshi returns aggregate YES and NO bid levels as [price, size] decimal strings.
#[derive(Debug, Deserialize, Serialize)]
pub struct OrderBook {
    pub yes_dollars: Vec<(Decimal, Decimal)>,
    pub no_dollars: Vec<(Decimal, Decimal)>,
}

#[derive(Debug, Deserialize)]
struct MarketResponse {
    market: Market,
}

#[derive(Debug, Deserialize)]
struct MarketPage {
    markets: Vec<Market>,
    #[serde(default)]
    cursor: Option<String>,
}

pub struct Client {
    http: reqwest::Client,
    base_url: String,
}

impl Client {
    pub fn new() -> Result<Self, Error> {
        Self::with_base_url(API)
    }

    /// Override the endpoint for integration tests or an explicitly chosen environment.
    pub fn with_base_url(base_url: &str) -> Result<Self, Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()?,
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }

    pub async fn series(&self, ticker: &str) -> Result<Series, Error> {
        // The CLI validates the ticker before placing it in this path.
        let response: SeriesResponse = self
            .http
            .get(format!("{}/series/{}", self.base_url, ticker))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response.series)
    }

    pub async fn exchange_status(&self) -> Result<ExchangeStatus, Error> {
        Ok(self
            .http
            .get(format!("{}/exchange/status", self.base_url))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn order_book(&self, ticker: &str) -> Result<OrderBook, Error> {
        let response: OrderBookResponse = self
            .http
            .get(format!("{}/markets/{}/orderbook", self.base_url, ticker))
            .query(&[("depth", "10")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response.orderbook_fp)
    }

    /// Window-specific discovery includes unopened contracts and avoids a reused
    /// status-filtered list at rollover. Exact identity is validated by callers.
    pub async fn window_markets(&self, series: &str, open: i64) -> Result<Vec<Market>, Error> {
        let page: MarketPage = self
            .http
            .get(format!("{}/markets", self.base_url))
            .header("Cache-Control", "no-cache")
            .query(&[
                ("series_ticker", series.to_owned()),
                ("min_close_ts", (open + 899).to_string()),
                ("max_close_ts", (open + 901).to_string()),
                ("limit", "20".into()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(page.markets)
    }
    pub async fn market(&self, ticker: &str) -> Result<Market, Error> {
        let response: MarketResponse = self
            .http
            .get(format!("{}/markets/{ticker}", self.base_url))
            .header("Cache-Control", "no-cache")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response.market)
    }

    pub async fn open_markets(&self, series: &str) -> Result<Vec<Market>, Error> {
        let mut markets = Vec::new();
        let mut cursor = String::new();
        let mut seen = HashSet::new();
        loop {
            let page: MarketPage = self
                .http
                .get(format!("{}/markets", self.base_url))
                .query(&[
                    ("series_ticker", series),
                    ("status", "open"),
                    ("limit", "200"),
                    ("cursor", &cursor),
                ])
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            markets.extend(page.markets);
            match page.cursor.filter(|c| !c.is_empty()) {
                Some(next) if seen.insert(next.clone()) => cursor = next,
                Some(_) => return Err(Error::RepeatedCursor),
                None => return Ok(markets),
            }
        }
    }
}

/// Among active windows, choose the one closing soonest; ticker breaks ties deterministically.
pub fn current_market(markets: &[Market], now: DateTime<Utc>) -> Option<&Market> {
    markets
        .iter()
        .filter(|m| m.status == "active" && m.open_time <= now && now < m.close_time)
        .min_by(|a, b| {
            a.close_time
                .cmp(&b.close_time)
                .then_with(|| a.ticker.cmp(&b.ticker))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };
    const MARKET: &str = r#"{"ticker":"TEST","title":"Test","status":"active","open_time":"2026-01-01T00:00:00Z","close_time":"2026-01-01T00:15:00Z","yes_bid_dollars":"0.1230","yes_ask_dollars":null}"#;
    fn instant(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn parses_decimal_and_optional_prices_exactly() {
        let m: Market = serde_json::from_str(MARKET).unwrap();
        assert_eq!(m.yes_bid_dollars.unwrap().to_string(), "0.1230");
        assert!(m.no_ask_dollars.is_none());
        assert!(m.yes_ask_dollars.is_none());
        assert!(serde_json::from_str::<Market>(r#"{"ticker":"bad"}"#).is_err());
        assert!(serde_json::from_str::<Market>(&MARKET.replace("0.1230", "oops")).is_err());
    }

    #[test]
    fn selects_active_window_and_respects_boundaries() {
        let m: Market = serde_json::from_str(MARKET).unwrap();
        assert!(current_market(&[], instant("2026-01-01T00:00:00Z")).is_none());
        assert!(
            current_market(std::slice::from_ref(&m), instant("2026-01-01T00:00:00Z")).is_some()
        );
        assert!(
            current_market(std::slice::from_ref(&m), instant("2026-01-01T00:15:00Z")).is_none()
        );
        let later: Market = serde_json::from_str(
            &MARKET
                .replace("TEST", "LATER")
                .replace("00:15:00", "00:30:00"),
        )
        .unwrap();
        assert_eq!(
            current_market(&[later, m], instant("2026-01-01T00:05:00Z"))
                .unwrap()
                .ticker,
            "TEST"
        );
    }

    #[tokio::test]
    async fn parses_order_book_and_preserves_fractional_levels() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/markets/TEST/orderbook"))
            .and(query_param("depth", "10"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"orderbook_fp":{"yes_dollars":[["0.1250","3.50"]],"no_dollars":[]}}"#,
            ))
            .mount(&server)
            .await;
        let client = Client::with_base_url(&server.uri()).unwrap();
        let book = client.order_book("TEST").await.unwrap();
        assert_eq!(book.yes_dollars[0].0.to_string(), "0.1250");
        assert_eq!(book.yes_dollars[0].1.to_string(), "3.50");
        assert!(book.no_dollars.is_empty());
        assert!(client.order_book("MISSING").await.is_err());
    }

    #[tokio::test]
    async fn pages_and_propagates_http_errors() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/markets"))
            .and(query_param("cursor", ""))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(format!(r#"{{"markets":[{MARKET}],"cursor":"next"}}"#)),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/markets"))
            .and(query_param("cursor", "next"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"{"markets":[],"cursor":""}"#),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/exchange/status"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let client = Client::with_base_url(&server.uri()).unwrap();
        assert_eq!(client.open_markets("KXBTC15M").await.unwrap().len(), 1);
        assert!(client.exchange_status().await.is_err());
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path, query_param},
    };
    #[tokio::test]
    async fn discovers_exact_future_window_without_status_cache_and_refreshes_by_id() {
        let server = MockServer::start().await;
        let market = serde_json::json!({"ticker":"KXBTC15M-TEST","title":"Test","status":"initialized",
            "open_time":"2026-11-10T22:00:00Z","close_time":"2026-11-10T22:15:00Z"});
        Mock::given(method("GET"))
            .and(path("/markets"))
            .and(query_param("min_close_ts", "2699"))
            .and(query_param("max_close_ts", "2701"))
            .and(|r: &wiremock::Request| r.url.query_pairs().all(|(k, _)| k != "status"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"markets":[market]})),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/markets/KXBTC15M-TEST"))
            .and(header("cache-control", "no-cache"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"market":market})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = Client::with_base_url(&server.uri()).unwrap();
        assert_eq!(
            client.window_markets("KXBTC15M", 1800).await.unwrap().len(),
            1
        );
        assert_eq!(
            client.market("KXBTC15M-TEST").await.unwrap().status,
            "initialized"
        );
    }
}
