//! Public GET-only combo discovery. Never creates markets, RFQs, quotes or orders.
use crate::assets::Asset;
use chrono::{DateTime, Utc};
use kalshi_client::{Client, Market, OrderBook, current_market};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Yes,
    No,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub btc: Option<Side>,
    pub eth: Option<Side>,
    pub sol: Option<Side>,
}
impl Selection {
    fn picks(&self) -> Vec<(Asset, Side)> {
        [
            (Asset::Btc, self.btc),
            (Asset::Eth, self.eth),
            (Asset::Sol, self.sol),
        ]
        .into_iter()
        .filter_map(|(a, s)| s.map(|s| (a, s)))
        .collect()
    }
    pub fn valid(&self) -> bool {
        self.picks().len() >= 2
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Leg {
    pub market_ticker: String,
    pub side: Side,
}
#[derive(Clone, Deserialize)]
struct ComboMarket {
    ticker: String,
    status: String,
    open_time: DateTime<Utc>,
    close_time: DateTime<Utc>,
    #[serde(default)]
    mve_selected_legs: Vec<Leg>,
}
#[derive(Deserialize)]
struct Page {
    markets: Vec<ComboMarket>,
    #[serde(default)]
    cursor: Option<String>,
}
#[derive(Clone, Serialize)]
pub struct Quote {
    pub status: &'static str,
    pub checked_at: DateTime<Utc>,
    pub message: &'static str,
    pub window_close: Option<DateTime<Utc>>,
    pub ticker: Option<String>,
    pub legs: Vec<Leg>,
    pub bid: Option<Decimal>,
    pub ask: Option<Decimal>,
    pub ask_size: Option<Decimal>,
    pub yes_bids: Vec<(Decimal, Decimal)>,
    pub yes_asks: Vec<(Decimal, Decimal)>,
    pub discovery_complete: bool,
    pub markets_found: usize,
    pub markets_checked: usize,
    pub book_errors: usize,
}
impl Quote {
    fn status(status: &'static str, message: &'static str) -> Self {
        Self {
            status,
            message,
            checked_at: Utc::now(),
            window_close: None,
            ticker: None,
            legs: vec![],
            bid: None,
            ask: None,
            ask_size: None,
            yes_bids: vec![],
            yes_asks: vec![],
            discovery_complete: false,
            markets_found: 0,
            markets_checked: 0,
            book_errors: 0,
        }
    }
}
struct Catalog {
    at: Instant,
    close: DateTime<Utc>,
    markets: HashMap<Asset, Market>,
    combos: Vec<ComboMarket>,
    complete: bool,
}
struct Cache {
    failed_at: Option<Instant>,
    catalog: Option<Catalog>,
    quotes: HashMap<String, (Instant, Quote)>,
}
pub struct Service {
    client: Arc<Client>,
    http: reqwest::Client,
    base: String,
    cache: Mutex<Cache>,
}
impl Service {
    pub fn new(client: Arc<Client>) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()?,
            base: "https://external-api.kalshi.com/trade-api/v2".into(),
            cache: Mutex::new(Cache {
                failed_at: None,
                catalog: None,
                quotes: HashMap::new(),
            }),
        })
    }
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, ()> {
        let mut response = self
            .http
            .get(format!("{}{path}", self.base))
            .query(query)
            .send()
            .await
            .map_err(|_| ())?
            .error_for_status()
            .map_err(|_| ())?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
            if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
                return Err(());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| ())
    }
    async fn discover(&self, now: DateTime<Utc>) -> Result<Catalog, ()> {
        let mut markets = HashMap::new();
        // Single-flight catalog shared by every selection and visitor. Three fixed asset queries only.
        for asset in Asset::ALL {
            let ms = self
                .client
                .open_markets(asset.info().series)
                .await
                .map_err(|_| ())?;
            if let Some(m) = current_market(&ms, now)
                .filter(|m| (m.close_time - m.open_time).num_seconds() == 900)
            {
                markets.insert(asset, m.clone());
            }
        }
        let close = markets.values().map(|m| m.close_time).min().ok_or(())?;
        let mut cursor = String::new();
        let mut seen = HashSet::new();
        let mut combos = vec![];
        let mut complete = false;
        for _ in 0..4 {
            let page: Page = self
                .get(
                    "/markets",
                    &[
                        ("mve_filter", "only".into()),
                        ("min_close_ts", now.timestamp().to_string()),
                        ("max_close_ts", (close.timestamp() + 1).to_string()),
                        ("limit", "1000".into()),
                        ("cursor", cursor),
                    ],
                )
                .await?;
            combos.extend(
                page.markets
                    .into_iter()
                    .filter(|m| m.mve_selected_legs.len() >= 2 && m.mve_selected_legs.len() <= 3),
            );
            match page.cursor.filter(|c| !c.is_empty()) {
                Some(next) if seen.insert(next.clone()) => cursor = next,
                Some(_) => return Err(()),
                None => {
                    complete = true;
                    break;
                }
            }
        }
        Ok(Catalog {
            at: Instant::now(),
            close,
            markets,
            combos,
            complete,
        })
    }
    pub async fn quote(&self, selection: &Selection) -> Quote {
        let picks = selection.picks();
        if picks.len() < 2 {
            return Quote::status("choose_picks", "Choose directions for at least two assets.");
        }
        let now = Utc::now();
        let mut cache = self.cache.lock().await;
        if cache
            .failed_at
            .is_some_and(|at| at.elapsed() < Duration::from_secs(30))
        {
            return Quote::status(
                "unavailable",
                "Kalshi discovery is temporarily unavailable. Retrying after a short cooldown.",
            );
        }
        if cache
            .catalog
            .as_ref()
            .is_none_or(|c| c.close <= now || c.at.elapsed() > Duration::from_secs(30))
        {
            match self.discover(now).await {
                Ok(c) => {
                    cache.failed_at = None;
                    cache.catalog = Some(c);
                    cache.quotes.clear()
                }
                Err(()) => {
                    cache.failed_at = Some(Instant::now());
                    return Quote::status(
                        "unavailable",
                        "Kalshi discovery is unavailable. No current quote can be verified.",
                    );
                }
            }
        }
        let catalog = cache.catalog.as_ref().expect("catalog set");
        let Some(first) = catalog.markets.get(&picks[0].0) else {
            return Quote::status("waiting_window", "Waiting for active 15-minute markets.");
        };
        if picks.iter().any(|(a, _)| {
            catalog.markets.get(a).is_none_or(|m| {
                m.open_time != first.open_time
                    || m.close_time != first.close_time
                    || m.open_time > now
                    || m.close_time <= now
            })
        }) {
            return Quote::status(
                "waiting_window",
                "The selected assets do not share an active 15-minute window.",
            );
        }
        let legs: Vec<Leg> = picks
            .iter()
            .map(|(a, s)| Leg {
                market_ticker: catalog.markets[a].ticker.clone(),
                side: *s,
            })
            .collect();
        let key = serde_json::to_string(&legs).expect("serializable legs");
        if let Some((_, q)) = cache
            .quotes
            .get(&key)
            .filter(|(at, _)| at.elapsed() < Duration::from_secs(5))
        {
            return q.clone();
        }
        let close = first.close_time;
        let complete = catalog.complete;
        let mut candidates: Vec<_> = catalog
            .combos
            .iter()
            .filter(|m| matches_exact(m, &legs, now, close))
            .cloned()
            .collect();
        candidates.sort_by(|a, b| a.ticker.cmp(&b.ticker));
        let mut q = Quote::status(
            if complete {
                "not_found"
            } else {
                "search_limited"
            },
            if complete {
                "No matching existing combo market was found. We do not create one or request quotes."
            } else {
                "The bounded market search is incomplete. No matching market was found in the checked pages."
            },
        );
        q.window_close = Some(close);
        q.legs = legs;
        q.discovery_complete = complete;
        q.markets_found = candidates.len();
        let mut chosen: Option<Quote> = None;
        let mut checked = 0;
        let mut errors = 0;
        for m in candidates.iter().take(3) {
            // Path is provider data, never user input. Reject unexpected path characters.
            if m.ticker.is_empty()
                || !m
                    .ticker
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                errors += 1;
                continue;
            }
            #[derive(Deserialize)]
            struct BookResponse {
                orderbook_fp: OrderBook,
            }
            match self
                .get::<BookResponse>(
                    &format!("/markets/{}/orderbook", m.ticker),
                    &[("depth", "10".into())],
                )
                .await
            {
                Ok(book) => {
                    checked += 1;
                    let mut candidate = q.clone();
                    candidate.ticker = Some(m.ticker.clone());
                    let (bids, asks) = levels(book.orderbook_fp);
                    candidate.bid = bids.first().map(|l| l.0);
                    candidate.ask = asks.first().map(|l| l.0);
                    candidate.ask_size = asks.first().map(|l| l.1);
                    candidate.yes_bids = bids;
                    candidate.yes_asks = asks;
                    let better = chosen
                        .as_ref()
                        .is_none_or(|old| match (candidate.ask, old.ask) {
                            (Some(a), Some(b)) => a < b,
                            (Some(_), None) => true,
                            (None, Some(_)) => false,
                            (None, None) => candidate.bid > old.bid,
                        });
                    if better {
                        chosen = Some(candidate)
                    }
                }
                Err(()) => errors += 1,
            }
        }
        if let Some(candidate) = chosen {
            q = candidate;
            q.status = if q.ask.is_some() {
                "available"
            } else if q.bid.is_some() {
                "bid_only"
            } else {
                "empty"
            };
            q.message = match q.status {
                "available" => {
                    "Public combo book snapshot. Best ask among the checked markets; fees excluded. Fills are not guaranteed."
                }
                "bid_only" => "A bid is visible, but no ask is available in the checked books.",
                _ => {
                    "Matching market found. The checked public books are empty; no quote is available."
                }
            };
        } else if !candidates.is_empty() {
            q.status = "unavailable";
            q.message = "Matching markets found, but their order books could not be verified.";
        }
        q.markets_checked = checked;
        q.book_errors = errors;
        // Do not publish a quote after its source contract window has ended.
        if Utc::now() >= close {
            return Quote::status(
                "waiting_window",
                "This 15-minute window has ended. Waiting for the next window.",
            );
        }
        q.checked_at = Utc::now();
        cache.quotes.insert(key, (Instant::now(), q.clone()));
        q
    }
}
fn matches_exact(m: &ComboMarket, legs: &[Leg], now: DateTime<Utc>, close: DateTime<Utc>) -> bool {
    m.status == "active"
        && m.open_time <= now
        && now < m.close_time
        && m.close_time == close
        && m.mve_selected_legs.len() == legs.len()
        && legs
            .iter()
            .all(|l| m.mve_selected_legs.iter().filter(|x| *x == l).count() == 1)
}
type Levels = Vec<(Decimal, Decimal)>;
fn levels(book: OrderBook) -> (Levels, Levels) {
    let valid =
        |(p, s): &(Decimal, Decimal)| *p > Decimal::ZERO && *p < Decimal::ONE && *s > Decimal::ZERO;
    // Aggregate duplicate levels and retain decimal quantities; no inferred size beyond displayed depth.
    let mut bids = BTreeMap::new();
    let mut asks = BTreeMap::new();
    for (p, s) in book.yes_dollars.into_iter().filter(valid) {
        *bids.entry(p).or_insert(Decimal::ZERO) += s;
    }
    for (p, s) in book.no_dollars.into_iter().filter(valid) {
        *asks.entry(Decimal::ONE - p).or_insert(Decimal::ZERO) += s;
    }
    (
        bids.into_iter().rev().take(10).collect(),
        asks.into_iter().take(10).collect(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_exact_legs_and_expiry() {
        let now: DateTime<Utc> = "2026-11-08T20:05:00Z".parse().unwrap();
        let close = "2026-11-08T20:15:00Z".parse().unwrap();
        let legs = vec![
            Leg {
                market_ticker: "BTC".into(),
                side: Side::Yes,
            },
            Leg {
                market_ticker: "ETH".into(),
                side: Side::No,
            },
        ];
        let mut m = ComboMarket {
            ticker: "COMBO".into(),
            status: "active".into(),
            open_time: now,
            close_time: close,
            mve_selected_legs: legs.clone(),
        };
        assert!(matches_exact(&m, &legs, now, close));
        m.mve_selected_legs[1].side = Side::Yes;
        assert!(!matches_exact(&m, &legs, now, close));
        m.mve_selected_legs = legs.clone();
        m.mve_selected_legs.push(legs[0].clone());
        assert!(!matches_exact(&m, &legs, now, close));
        m.mve_selected_legs = legs.clone();
        assert!(!matches_exact(&m, &legs, close, close));
        m.close_time += chrono::Duration::minutes(15);
        assert!(!matches_exact(&m, &legs, now, close));
    }
    #[test]
    fn rejects_arbitrary_assets_tickers_and_duplicate_parameters() {
        assert!(
            serde_json::from_str::<Selection>(r#"{"btc":"yes","eth":"no"}"#)
                .unwrap()
                .valid()
        );
        for s in [
            r#"{"btc":"up","eth":"yes"}"#,
            r#"{"ticker":"anything"}"#,
            r#"{"btc":"yes","btc":"no"}"#,
        ] {
            assert!(serde_json::from_str::<Selection>(s).is_err())
        }
        assert!(!Selection::default().valid());
    }
    #[test]
    fn complement_prices_preserve_size_and_missing_sides() {
        let d = |s: &str| s.parse::<Decimal>().unwrap();
        let (b, a) = levels(OrderBook {
            yes_dollars: vec![(d("0.12"), d("2.5")), (d("0"), d("10"))],
            no_dollars: vec![(d("0.75"), d("3.5")), (d("0.75"), d("1")), (d("1"), d("9"))],
        });
        assert_eq!(b, vec![(d("0.12"), d("2.5"))]);
        assert_eq!(a, vec![(d("0.25"), d("4.5"))]);
        let (b, a) = levels(OrderBook {
            yes_dollars: vec![],
            no_dollars: vec![],
        });
        assert!(b.is_empty() && a.is_empty());
    }
    async fn fixture(book: serde_json::Value, code: u16) -> (Service, wiremock::MockServer) {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path, query_param},
        };
        let server = MockServer::start().await;
        let now = Utc::now();
        let close = now + chrono::Duration::minutes(10);
        let open = close - chrono::Duration::minutes(15);
        let base = |ticker: &str| serde_json::json!({"ticker":ticker,"title":ticker,"status":"active","open_time":open,"close_time":close});
        for (asset, ticker) in [
            ("KXBTC15M", "BTC"),
            ("KXETH15M", "ETH"),
            ("KXSOL15M", "SOL"),
        ] {
            Mock::given(method("GET"))
                .and(path("/markets"))
                .and(query_param("series_ticker", asset))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"markets":[base(ticker)],"cursor":""})),
                )
                .expect(1)
                .mount(&server)
                .await;
        }
        let mut combo = base("COMBO");
        combo["mve_selected_legs"] = serde_json::json!([{"market_ticker":"ETH","side":"no"},{"market_ticker":"BTC","side":"yes"}]);
        Mock::given(method("GET"))
            .and(path("/markets"))
            .and(query_param("mve_filter", "only"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"markets":[combo],"cursor":""})),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/markets/COMBO/orderbook"))
            .respond_with(ResponseTemplate::new(code).set_body_json(book))
            .expect(1)
            .mount(&server)
            .await;
        let mut service =
            Service::new(Arc::new(Client::with_base_url(&server.uri()).unwrap())).unwrap();
        service.base = server.uri();
        (service, server)
    }
    #[tokio::test]
    async fn reads_exact_public_book_and_coalesces_requests() {
        let(service,server)=fixture(serde_json::json!({"orderbook_fp":{"yes_dollars":[["0.20","3.5"]],"no_dollars":[["0.70","2.25"]]}}),200).await;
        let picks = Selection {
            btc: Some(Side::Yes),
            eth: Some(Side::No),
            sol: None,
        };
        let q = service.quote(&picks).await;
        assert_eq!(q.status, "available");
        assert_eq!(q.ask, Some("0.30".parse().unwrap()));
        assert_eq!(q.ask_size, Some("2.25".parse().unwrap()));
        assert_eq!(service.quote(&picks).await.checked_at, q.checked_at);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 5);
        assert!(requests.iter().all(|r| r.method == "GET"));
        let q = service
            .quote(&Selection {
                btc: Some(Side::No),
                eth: Some(Side::No),
                sol: None,
            })
            .await;
        assert_eq!(q.status, "not_found");
        assert!(q.ask.is_none());
    }
    #[tokio::test]
    async fn empty_book_and_failed_book_are_distinct() {
        for (code, expected) in [(200, "empty"), (503, "unavailable")] {
            let (service, _server) = fixture(
                serde_json::json!({"orderbook_fp":{"yes_dollars":[],"no_dollars":[]}}),
                code,
            )
            .await;
            let q = service
                .quote(&Selection {
                    btc: Some(Side::Yes),
                    eth: Some(Side::No),
                    sol: None,
                })
                .await;
            assert_eq!(q.status, expected);
            assert!(q.ask.is_none());
        }
    }
    #[tokio::test]
    async fn bid_only_does_not_invent_an_ask() {
        let (service, _server) = fixture(
            serde_json::json!({"orderbook_fp":{"yes_dollars":[["0.20","2"]],"no_dollars":[]}}),
            200,
        )
        .await;
        let q = service
            .quote(&Selection {
                btc: Some(Side::Yes),
                eth: Some(Side::No),
                sol: None,
            })
            .await;
        assert_eq!(q.status, "bid_only");
        assert!(q.ask.is_none());
    }
}
