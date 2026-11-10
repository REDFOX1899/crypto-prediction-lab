//! Durable settled contracts. Cloud Run IAM protects the separate collector service.
use crate::assets::Asset;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

type Failure = Box<dyn std::error::Error + Send + Sync>;
const API: &str = "https://external-api.kalshi.com/trade-api/v2";
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Entry {
    pub ask: Decimal,
    pub minute_start: i64,
    pub gross_multiple: Decimal,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Contract {
    pub ticker: String,
    pub asset: Asset,
    pub open_time: String,
    pub close_time: String,
    pub result: String,
    pub target: Option<String>,
    pub expiration_value: Option<String>,
    pub updated_time: Option<String>,
    pub best_entry: Option<Entry>,
    pub candle_count: usize,
    pub captured_at: String,
}
#[derive(Clone, Serialize)]
pub struct Board {
    pub status: &'static str,
    pub contracts: Vec<Contract>,
    pub fetched_at: String,
}
#[derive(Deserialize)]
struct Token {
    access_token: String,
    expires_in: u64,
}
struct Auth {
    token: String,
    until: Instant,
}
pub struct Archive {
    http: reqwest::Client,
    documents: String,
    auth: Mutex<Option<Auth>>,
    cache: Mutex<HashMap<Asset, (Instant, Board)>>,
    collection_lock: Mutex<()>,
}
impl Archive {
    pub fn from_env() -> Result<Option<Arc<Self>>, Failure> {
        let Ok(project) = std::env::var("HISTORY_PROJECT") else {
            return Ok(None);
        };
        let database = std::env::var("HISTORY_DATABASE")?;
        if !project
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
            || !database
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err("Invalid history configuration".into());
        }
        Ok(Some(Arc::new(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()?,
            documents: format!(
                "https://firestore.googleapis.com/v1/projects/{project}/databases/{database}/documents"
            ),
            auth: Mutex::new(None),
            cache: Mutex::new(HashMap::new()),
            collection_lock: Mutex::new(()),
        })))
    }
    async fn token(&self) -> Result<String, Failure> {
        let mut auth = self.auth.lock().await;
        if let Some(a) = &*auth
            && Instant::now() < a.until
        {
            return Ok(a.token.clone());
        }
        let token: Token = self.http.get("http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token").header("Metadata-Flavor", "Google").send().await?.error_for_status()?.json().await?;
        let until = Instant::now() + Duration::from_secs(token.expires_in.saturating_sub(60));
        let value = token.access_token.clone();
        *auth = Some(Auth {
            token: token.access_token,
            until,
        });
        Ok(value)
    }
    pub(crate) async fn context_get(&self) -> Result<Option<Value>, Failure> {
        let response = self
            .http
            .get(format!("{}/cmc_context/latest", self.documents))
            .bearer_auth(self.token().await?)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let doc: Value = response.error_for_status()?.json().await?;
        Ok(doc
            .pointer("/fields/payload/stringValue")
            .and_then(Value::as_str)
            .map(serde_json::from_str)
            .transpose()?)
    }
    pub(crate) async fn context_put(&self, value: &Value, sample: bool) -> Result<(), Failure> {
        let payload = serde_json::to_string(value)?;
        self.http
            .patch(format!("{}/cmc_context/latest", self.documents))
            .bearer_auth(self.token().await?)
            .json(&json!({"fields":{"payload":{"stringValue":payload}}}))
            .send()
            .await?
            .error_for_status()?;
        if sample {
            let at = value["quotes_checked_at"]
                .as_str()
                .ok_or("Missing observation time")?;
            let slot = DateTime::parse_from_rfc3339(at)?.timestamp() / 300;
            let summary = json!({"at":at,"quotes":value["quotes"]});
            self.http.patch(format!("{}/cmc_samples/s{slot}", self.documents)).bearer_auth(self.token().await?).json(&json!({"fields":{"captured_at":{"stringValue":at},"payload":{"stringValue":serde_json::to_string(&summary)?}}})).send().await?.error_for_status()?;
        }
        Ok(())
    }
    pub(crate) async fn context_recent(&self) -> Result<Vec<Value>, Failure> {
        let rows: Vec<Value> = self.http.post(format!("{}:runQuery", self.documents)).bearer_auth(self.token().await?).json(&json!({"structuredQuery":{"select":{"fields":[{"fieldPath":"payload"}]},"from":[{"collectionId":"cmc_samples"}],"orderBy":[{"field":{"fieldPath":"captured_at"},"direction":"DESCENDING"}],"limit":24}})).send().await?.error_for_status()?.json().await?;
        rows.iter()
            .filter_map(|r| {
                r.pointer("/document/fields/payload/stringValue")
                    .and_then(Value::as_str)
            })
            .map(|s| serde_json::from_str(s).map_err(Into::into))
            .collect()
    }
    fn collection(asset: Asset) -> String {
        format!(
            "contracts_{}",
            match asset {
                Asset::Btc => "btc",
                Asset::Eth => "eth",
                Asset::Sol => "sol",
            }
        )
    }
    async fn read(&self, asset: Asset, limit: usize) -> Result<Vec<Contract>, Failure> {
        let rows: Vec<Value> = self.http.post(format!("{}:runQuery", self.documents)).bearer_auth(self.token().await?).json(&json!({"structuredQuery":{
            "select":{"fields":[{"fieldPath":"summary"}]},"from":[{"collectionId":Self::collection(asset)}],
            "orderBy":[{"field":{"fieldPath":"close_time"},"direction":"DESCENDING"}],"limit":limit
        }})).send().await?.error_for_status()?.json().await?;
        let mut result = Vec::new();
        for row in rows {
            if let Some(s) = row
                .pointer("/document/fields/summary/stringValue")
                .and_then(Value::as_str)
            {
                result.push(serde_json::from_str(s)?);
            }
        }
        Ok(result)
    }
    pub async fn board(&self, asset: Asset) -> Result<Board, Failure> {
        let mut cache = self.cache.lock().await;
        if let Some((at, value)) = cache.get(&asset)
            && at.elapsed() < Duration::from_secs(60)
        {
            return Ok(value.clone());
        }
        let board = Board {
            status: "available",
            contracts: self.read(asset, 96).await?,
            fetched_at: Utc::now().to_rfc3339(),
        };
        cache.insert(asset, (Instant::now(), board.clone()));
        Ok(board)
    }
    pub async fn collect(&self) -> Result<usize, Failure> {
        // Avoid overlapping work per instance; writes use stable native ticker IDs.
        let _guard = self
            .collection_lock
            .try_lock()
            .map_err(|_| "Collection already running")?;
        let mut stored = 0;
        let mut failures = 0;
        for asset in Asset::ALL {
            let existing: HashMap<String, Contract> = self
                .read(asset, 220)
                .await?
                .into_iter()
                .map(|c| (c.ticker.clone(), c))
                .collect();
            let mut cursor = String::new();
            let mut seen = std::collections::HashSet::new();
            let mut attempted = 0;
            'pages: for _ in 0..4 {
                let page = kalshi_json(
                    &self.http,
                    &format!("{API}/markets"),
                    &[
                        ("series_ticker", asset.info().series.to_owned()),
                        ("status", "settled".into()),
                        (
                            "min_settled_ts",
                            (Utc::now().timestamp() - 172800).to_string(),
                        ),
                        ("limit", "200".into()),
                        ("cursor", cursor.clone()),
                    ],
                )
                .await?;
                let markets = page
                    .get("markets")
                    .and_then(Value::as_array)
                    .ok_or("Missing market records")?;
                for market in markets {
                    let Some(ticker) = market.get("ticker").and_then(Value::as_str) else {
                        continue;
                    };
                    if !valid_ticker(ticker, asset) {
                        continue;
                    }
                    let updated = market.get("updated_time").and_then(Value::as_str);
                    if existing
                        .get(ticker)
                        .is_some_and(|c| c.updated_time.as_deref() == updated && c.candle_count > 0)
                    {
                        continue;
                    }
                    if attempted >= 80 {
                        break 'pages;
                    }
                    attempted += 1;
                    match self.capture(asset, market).await {
                        Ok(()) => stored += 1,
                        Err(error) => {
                            failures += 1;
                            eprintln!("Could not archive completed contract {ticker}: {error}");
                        }
                    }
                }
                match page
                    .get("cursor")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    Some(next) if seen.insert(next.to_owned()) => cursor = next.to_owned(),
                    Some(_) => return Err("Repeated market cursor".into()),
                    None => break,
                }
            }
        }
        if failures > 0 {
            return Err(
                format!("Stored {stored} contracts, {failures} captures need retry").into(),
            );
        }
        Ok(stored)
    }
    async fn capture(&self, asset: Asset, market: &Value) -> Result<(), Failure> {
        let ticker = market["ticker"].as_str().ok_or("Missing ticker")?;
        let open = market["open_time"].as_str().ok_or("Missing open time")?;
        let close = market["close_time"].as_str().ok_or("Missing close time")?;
        let start = DateTime::parse_from_rfc3339(open)?.timestamp();
        let end = DateTime::parse_from_rfc3339(close)?.timestamp();
        let result = market["result"].as_str().ok_or("No settled result")?;
        // Do not interpret void, nonbinary or pending outcomes as UP/DOWN.
        if !["yes", "no"].contains(&result) || end > Utc::now().timestamp() {
            return Ok(());
        }
        let candles = kalshi_json(
            &self.http,
            &format!(
                "{API}/series/{}/markets/{ticker}/candlesticks",
                asset.info().series
            ),
            &[
                ("start_ts", start.to_string()),
                ("end_ts", end.to_string()),
                ("period_interval", "1".into()),
            ],
        )
        .await?;
        if candles["ticker"].as_str() != Some(ticker) {
            return Err("Mismatched candle ticker".into());
        }
        let bars = candles["candlesticks"]
            .as_array()
            .ok_or("Missing candle array")?;
        let summary = Contract {
            ticker: ticker.into(),
            asset,
            open_time: open.into(),
            close_time: close.into(),
            result: result.into(),
            target: market["yes_sub_title"].as_str().map(str::to_owned),
            expiration_value: market["expiration_value"].as_str().map(str::to_owned),
            updated_time: market["updated_time"].as_str().map(str::to_owned),
            best_entry: best_entry(bars, result, start, end),
            candle_count: bars.len(),
            captured_at: Utc::now().to_rfc3339(),
        };
        self.http.patch(format!("{}/{}/{}", self.documents, Self::collection(asset), ticker)).bearer_auth(self.token().await?).json(&json!({"fields":{
            "close_time":{"timestampValue":close},"summary":{"stringValue":serde_json::to_string(&summary)?},
            "payload":{"stringValue":serde_json::to_string(&json!({"schema_version":1,"market":market,"candlesticks":candles}))?}
        }})).send().await?.error_for_status()?;
        Ok(())
    }
}
pub fn valid_ticker(ticker: &str, asset: Asset) -> bool {
    ticker.starts_with(&format!("{}-", asset.info().series))
        && ticker.len() < 90
        && ticker
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}
/// Minute lows are hindsight quote observations, not executable fills or signals.
pub fn best_entry(bars: &[Value], result: &str, start: i64, end: i64) -> Option<Entry> {
    bars.iter()
        .filter_map(|bar| {
            let minute_end = bar["end_period_ts"].as_i64()?;
            if minute_end - 60 < start || minute_end > end {
                return None;
            }
            let value = match result {
                "yes" => bar
                    .pointer("/yes_ask/low_dollars")?
                    .as_str()?
                    .parse::<Decimal>()
                    .ok()?,
                "no" => {
                    Decimal::ONE
                        - bar
                            .pointer("/yes_bid/high_dollars")?
                            .as_str()?
                            .parse::<Decimal>()
                            .ok()?
                }
                _ => return None,
            };
            if value <= Decimal::ZERO || value >= Decimal::ONE {
                return None;
            }
            Some(Entry {
                ask: value,
                minute_start: minute_end - 60,
                gross_multiple: Decimal::ONE / value,
            })
        })
        .min_by(|a, b| {
            a.ask
                .cmp(&b.ask)
                .then_with(|| a.minute_start.cmp(&b.minute_start))
        })
}
async fn kalshi_json(
    http: &reqwest::Client,
    url: &str,
    query: &[(&str, String)],
) -> Result<Value, Failure> {
    for attempt in 0..4 {
        // Public REST has a separate quota; keep bootstrap capture deliberately slow.
        tokio::time::sleep(Duration::from_millis(750)).await;
        let response = http.get(url).query(query).send().await?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt < 3 {
            let seconds = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|s| s.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(2u64.pow(attempt + 1))
                .clamp(1, 30);
            tokio::time::sleep(Duration::from_secs(seconds)).await;
            continue;
        }
        return Ok(response.error_for_status()?.json().await?);
    }
    Err("Source retry limit reached".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hindsight_entry_uses_winning_ask_and_excludes_outside_minutes_and_zero() {
        let bars = vec![
            json!({"end_period_ts":120,"yes_ask":{"low_dollars":"0.20"},"yes_bid":{"high_dollars":"0.70"}}),
            json!({"end_period_ts":180,"yes_ask":{"low_dollars":"0.00"},"yes_bid":{"high_dollars":"1.00"}}),
            json!({"end_period_ts":240,"yes_ask":{"low_dollars":"0.01"},"yes_bid":{"high_dollars":"0.99"}}),
        ];
        let yes = best_entry(&bars, "yes", 60, 180).unwrap();
        assert_eq!(yes.ask, Decimal::new(20, 2));
        assert_eq!(yes.gross_multiple, Decimal::from(5));
        assert_eq!(yes.minute_start, 60);
        let no = best_entry(&bars, "no", 60, 180).unwrap();
        assert_eq!(no.ask, Decimal::new(30, 2));
        assert!(best_entry(&bars, "void", 60, 180).is_none());
    }
    #[tokio::test]
    async fn source_rate_limit_is_retried_before_parsing_json() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for response in [
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buffer = [0; 2048];
                let _ = socket.read(&mut buffer).await.unwrap();
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let value = kalshi_json(
            &reqwest::Client::new(),
            &format!("http://{address}/candles"),
            &[],
        )
        .await
        .unwrap();
        assert_eq!(value["ok"], true);
        server.await.unwrap();
    }
    #[test]
    fn ticker_cannot_escape_asset_collection() {
        assert!(valid_ticker("KXBTC15M-26NOV061200-00", Asset::Btc));
        assert!(!valid_ticker("KXETH15M-26NOV061200-00", Asset::Btc));
        assert!(!valid_ticker("KXBTC15M-../../bad", Asset::Btc));
    }
}
