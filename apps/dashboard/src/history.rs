//! Read-only, bounded history requests. Credentials stay on the server.
use crate::stream::Credentials;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const API: &str = "https://external-api.kalshi.com/trade-api/v2";
const CF_PATH: &str = "/trade-api/v2/cfbenchmarks/history/values";
const MAX_BYTES: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Candle {
    pub time: i64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
}
#[derive(Clone, Serialize)]
pub struct History {
    pub status: &'static str,
    pub candles: Vec<Candle>,
}
#[derive(Serialize)]
pub struct MarketHistory {
    pub ticker: String,
    pub status: &'static str,
    pub candles: Vec<Candle>,
}
#[derive(Deserialize)]
struct CfEnvelope {
    data: CfData,
}
#[derive(Deserialize)]
struct CfData {
    payload: Option<Vec<Tick>>,
    error: Option<String>,
}
#[derive(Deserialize)]
struct Tick {
    time: i64,
    value: Decimal,
}
#[derive(Deserialize)]
struct MarketEnvelope {
    ticker: String,
    candlesticks: Vec<MarketCandle>,
}
#[derive(Deserialize)]
struct MarketCandle {
    end_period_ts: i64,
    yes_ask: Option<Quote>,
}
#[derive(Deserialize)]
struct Quote {
    open_dollars: Option<Decimal>,
    high_dollars: Option<Decimal>,
    low_dollars: Option<Decimal>,
    close_dollars: Option<Decimal>,
}

fn aggregate(mut ticks: Vec<Tick>, start_ms: i64, end_ms: i64) -> Vec<Candle> {
    ticks.sort_by_key(|t| t.time);
    let mut minutes: BTreeMap<i64, Candle> = BTreeMap::new();
    for tick in ticks {
        if tick.time < start_ms || tick.time >= end_ms || tick.value <= Decimal::ZERO {
            continue;
        }
        let minute = tick.time.div_euclid(60_000) * 60;
        minutes
            .entry(minute)
            .and_modify(|c| {
                c.high = c.high.max(tick.value);
                c.low = c.low.min(tick.value);
                c.close = tick.value;
            })
            .or_insert(Candle {
                time: minute,
                open: tick.value,
                high: tick.value,
                low: tick.value,
                close: tick.value,
            });
    }
    minutes.into_values().collect()
}
fn market_candles(items: Vec<MarketCandle>, now: i64) -> Vec<Candle> {
    let mut candles = BTreeMap::new();
    for item in items {
        if item.end_period_ts > now || item.end_period_ts <= 60 || item.end_period_ts % 60 != 0 {
            continue;
        }
        if let Some(Quote {
            open_dollars: Some(open),
            high_dollars: Some(high),
            low_dollars: Some(low),
            close_dollars: Some(close),
        }) = item.yes_ask
            && low <= open
            && open <= high
            && low <= close
            && close <= high
            && low >= Decimal::ZERO
            && high <= Decimal::ONE
        {
            candles.insert(
                item.end_period_ts - 60,
                Candle {
                    time: item.end_period_ts - 60,
                    open,
                    high,
                    low,
                    close,
                },
            );
        }
    }
    candles.into_values().collect()
}
pub struct HistoryService {
    http: reqwest::Client,
    credentials: Option<Credentials>,
    cached: Mutex<Option<(Instant, History)>>,
}
impl HistoryService {
    pub fn new(credentials: Option<Credentials>) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()?,
            credentials,
            cached: Mutex::new(None),
        })
    }
    pub async fn reference(&self) -> History {
        let mut cache = self.cached.lock().await;
        if let Some((at, result)) = &*cache
            && at.elapsed() < Duration::from_secs(300)
        {
            return result.clone();
        }
        let result = self.fetch_reference().await;
        *cache = Some((Instant::now(), result.clone()));
        result
    }
    async fn fetch_reference(&self) -> History {
        let Some(credentials) = &self.credentials else {
            return History {
                status: "credentials_required",
                candles: vec![],
            };
        };
        let hour = Utc::now().timestamp().div_euclid(3600) * 3600;
        let mut candles = Vec::new();
        let mut failures = 0;
        for offset in [2, 1] {
            let start = hour - offset * 3600;
            let Some(date) = DateTime::from_timestamp(start, 0) else {
                continue;
            };
            let timestamp = date.format("%Y-%m-%dT%H:00:00.000Z").to_string();
            let ts = Utc::now().timestamp_millis().to_string();
            let url = format!("{API}/cfbenchmarks/history/values");
            let response = self
                .http
                .get(url)
                .query(&[
                    ("id", "BRTI"),
                    ("timespan", "HOUR"),
                    ("timestamp", &timestamp),
                ])
                .header("KALSHI-ACCESS-KEY", &credentials.key_id)
                .header("KALSHI-ACCESS-TIMESTAMP", &ts)
                .header(
                    "KALSHI-ACCESS-SIGNATURE",
                    credentials.signature_for(&ts, CF_PATH),
                )
                .send()
                .await;
            let parsed = async {
                let mut response = response.ok()?.error_for_status().ok()?;
                if response
                    .content_length()
                    .is_some_and(|n| n > MAX_BYTES as u64)
                {
                    return None;
                }
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.ok()? {
                    if bytes.len() + chunk.len() > MAX_BYTES {
                        return None;
                    }
                    bytes.extend_from_slice(&chunk);
                }
                let data: CfEnvelope = serde_json::from_slice(&bytes).ok()?;
                if data.data.error.is_some() {
                    return None;
                }
                Some(aggregate(
                    data.data.payload?,
                    start * 1000,
                    (start + 3600) * 1000,
                ))
            }
            .await;
            match parsed {
                Some(mut data) if !data.is_empty() => candles.append(&mut data),
                _ => failures += 1,
            }
        }
        History {
            status: if candles.is_empty() {
                "unavailable"
            } else if failures > 0 {
                "partial"
            } else {
                "available"
            },
            candles,
        }
    }
    pub async fn market(
        &self,
        ticker: &str,
        open_time: i64,
    ) -> Result<MarketHistory, reqwest::Error> {
        // Ticker originates from Kalshi market discovery, not a browser-supplied path.
        let now = Utc::now().timestamp();
        let response: MarketEnvelope = self
            .http
            .get(format!(
                "{API}/series/KXBTC15M/markets/{ticker}/candlesticks"
            ))
            .query(&[
                ("start_ts", open_time.to_string()),
                ("end_ts", now.to_string()),
                ("period_interval", "1".into()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let candles = if response.ticker == ticker {
            market_candles(response.candlesticks, now)
        } else {
            vec![]
        };
        Ok(MarketHistory {
            ticker: ticker.to_owned(),
            status: if candles.is_empty() {
                "empty"
            } else {
                "available"
            },
            candles,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }
    #[test]
    fn reference_aggregates_ordered_actual_values_and_skips_out_of_window() {
        let candles = aggregate(
            vec![
                Tick {
                    time: 120_002,
                    value: d("102"),
                },
                Tick {
                    time: 120_001,
                    value: d("100"),
                },
                Tick {
                    time: 120_003,
                    value: d("98"),
                },
                Tick {
                    time: 240_000,
                    value: d("5"),
                },
            ],
            120_000,
            180_000,
        );
        assert_eq!(
            candles,
            vec![Candle {
                time: 120,
                open: d("100"),
                high: d("102"),
                low: d("98"),
                close: d("98")
            }]
        );
    }
    #[test]
    fn market_filters_missing_invalid_and_incomplete_candles() {
        let raw = r#"{"ticker":"T","candlesticks":[{"end_period_ts":120,"yes_ask":{"open_dollars":"0.4000","high_dollars":"0.5000","low_dollars":"0.3000","close_dollars":"0.4500"}},{"end_period_ts":180,"yes_ask":null},{"end_period_ts":240,"yes_ask":{"open_dollars":"0.4000","high_dollars":"0.5000","low_dollars":"0.3000","close_dollars":"0.4500"}}]}"#;
        let parsed: MarketEnvelope = serde_json::from_str(raw).unwrap();
        let c = market_candles(parsed.candlesticks, 180);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].time, 60);
        assert_eq!(c[0].close, d("0.45"));
    }
    #[test]
    fn parses_cf_passthrough_envelope() {
        let raw = r#"{"data":{"serverTime":"2026-01-01T00:00:00Z","payload":[{"time":120001,"value":"123.4500"}]}}"#;
        let parsed: CfEnvelope = serde_json::from_str(raw).unwrap();
        assert_eq!(
            parsed.data.payload.unwrap()[0].value.to_string(),
            "123.4500"
        );
    }
}
