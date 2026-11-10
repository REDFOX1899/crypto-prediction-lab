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
impl Board {
    /// Home only draws twelve official-result cells. The full board remains available to research views.
    pub fn recent(&self) -> Value {
        let contracts: Vec<_> = self.contracts.iter().take(12).map(|c| json!({
            "ticker":c.ticker, "asset": c.asset, "open_time": c.open_time, "close_time": c.close_time, "result": c.result
        })).collect();
        json!({"status": self.status, "fetched_at": self.fetched_at, "contracts": contracts})
    }
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
    study_cache: Mutex<Option<(Instant, Option<crate::quote_study::Board>)>>,
    detail_cache: Mutex<HashMap<String, (Instant, Value)>>,
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
            study_cache: Mutex::new(None),
            detail_cache: Mutex::new(HashMap::new()),
        })))
    }
    pub(crate) async fn minute_ten(
        &self,
        asset: Asset,
        open: i64,
    ) -> Result<Option<Value>, Failure> {
        let symbol = serde_json::to_value(asset)?
            .as_str()
            .ok_or("Invalid asset")?
            .to_owned();
        self.saved_payload(&format!("minute_ten_v1/s{open}_{symbol}"), 32_768)
            .await
    }

    pub(crate) async fn collect_minute_ten(&self) -> Result<Value, Failure> {
        let _guard = self
            .collection_lock
            .try_lock()
            .map_err(|_| "Collection already running")?;
        let Some(open) = crate::checkpoint::slot(Utc::now().timestamp()) else {
            return Ok(json!({"status":"outside_capture_minute","stored":0}));
        };
        let client = kalshi_client::Client::new()?;
        let mut stored = 0;
        for asset in Asset::ALL {
            if self.minute_ten(asset, open).await?.is_some() {
                continue;
            }
            let observation = crate::checkpoint::capture(&client, asset, open).await;
            let symbol = serde_json::to_value(asset)?
                .as_str()
                .ok_or("Invalid asset")?
                .to_owned();
            let name = format!(
                "{}/minute_ten_v1/s{open}_{symbol}",
                self.documents
                    .trim_start_matches("https://firestore.googleapis.com/v1/")
            );
            let response = self.http.post(format!("{}:commit",self.documents)).bearer_auth(self.token().await?)
                .json(&json!({"writes":[{"update":{"name":name,"fields":{"payload":{"stringValue":serde_json::to_string(&observation)?}}},
                "currentDocument":{"exists":false}}]})).send().await?;
            if response.status() == reqwest::StatusCode::CONFLICT {
                if self.minute_ten(asset, open).await?.is_none() {
                    return Err("Checkpoint conflict".into());
                }
            } else {
                response.error_for_status()?;
                stored += 1;
            }
        }
        Ok(json!({"status":"captured","stored":stored,"open":open}))
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
    /// Fixed collection for the manually published, licensed research pilot; no writes.
    pub(crate) async fn blockworks_document(&self, id: &str) -> Result<Option<Value>, Failure> {
        if id.len() > 80 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err("Invalid research document ID".into());
        }
        self.saved_payload(&format!("blockworks_pilots/{id}"), 900_000)
            .await
    }

    pub(crate) async fn quote_study(&self) -> Result<Option<crate::quote_study::Board>, Failure> {
        let mut cache = self.study_cache.lock().await;
        if let Some((at, board)) = &*cache
            && at.elapsed() < Duration::from_secs(600)
        {
            return Ok(board.clone());
        }
        let board = match self
            .saved_payload("quote_studies_v1/latest", 32_768)
            .await?
        {
            None => None,
            Some(mut manifest) => {
                let board: crate::quote_study::Board =
                    serde_json::from_value(manifest["board"].take())?;
                if !board.valid() {
                    return Err("Invalid saved study".into());
                }
                Some(board)
            }
        };
        *cache = Some((Instant::now(), board.clone()));
        Ok(board)
    }

    async fn saved_payload(&self, path: &str, byte_limit: usize) -> Result<Option<Value>, Failure> {
        let mut response = self
            .http
            .get(format!("{}/{path}", self.documents))
            .bearer_auth(self.token().await?)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        response.error_for_status_ref()?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > byte_limit {
                return Err("Research document exceeds limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let doc: Value = serde_json::from_slice(&bytes)?;
        let payload = doc
            .pointer("/fields/payload/stringValue")
            .and_then(Value::as_str)
            .ok_or("Missing research payload")?;
        Ok(Some(serde_json::from_str(payload)?))
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
    pub(crate) async fn context_recent(&self, limit: usize) -> Result<Vec<Value>, Failure> {
        let rows: Vec<Value> = self.http.post(format!("{}:runQuery", self.documents)).bearer_auth(self.token().await?).json(&json!({"structuredQuery":{"select":{"fields":[{"fieldPath":"payload"}]},"from":[{"collectionId":"cmc_samples"}],"orderBy":[{"field":{"fieldPath":"captured_at"},"direction":"DESCENDING"}],"limit":limit.clamp(1,288)}})).send().await?.error_for_status()?.json().await?;
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
    pub async fn detail(&self, asset: Asset, ticker: &str) -> Result<Option<Value>, Failure> {
        if !valid_ticker(ticker, asset) {
            return Err("Invalid archive selection".into());
        }
        let mut cache = self.detail_cache.lock().await;
        if let Some((at, value)) = cache.get(ticker)
            && at.elapsed() < Duration::from_secs(300)
        {
            return Ok(Some(value.clone()));
        }
        let mut response = self
            .http
            .get(format!(
                "{}/{}/{ticker}",
                self.documents,
                Self::collection(asset)
            ))
            .bearer_auth(self.token().await?)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        response.error_for_status_ref()?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err("Archived response too large".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let doc: Value = serde_json::from_slice(&bytes)?;
        let payload: Value = serde_json::from_str(
            doc.pointer("/fields/payload/stringValue")
                .and_then(Value::as_str)
                .ok_or("Missing archived payload")?,
        )?;
        let detail = normalized_detail(&payload, asset, ticker, Utc::now().timestamp())?;
        if cache.len() >= 32
            && let Some(key) = cache
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(k, _)| k.clone())
        {
            cache.remove(&key);
        }
        cache.insert(ticker.into(), (Instant::now(), detail.clone()));
        Ok(Some(detail))
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
fn normalized_detail(
    payload: &Value,
    asset: Asset,
    ticker: &str,
    now: i64,
) -> Result<Value, Failure> {
    let market = &payload["market"];
    if !valid_ticker(ticker, asset)
        || market["ticker"].as_str() != Some(ticker)
        || !matches!(market["result"].as_str(), Some("yes" | "no"))
    {
        return Err("Mismatched archive identity".into());
    }
    let open = market["open_time"].as_str().ok_or("Missing open time")?;
    let close = market["close_time"].as_str().ok_or("Missing close time")?;
    let start = DateTime::parse_from_rfc3339(open)?.timestamp();
    let end = DateTime::parse_from_rfc3339(close)?.timestamp();
    if end > now || end - start != 900 {
        return Err("Not a completed 15-minute window".into());
    }
    let candles =
        crate::history::archived_candles(payload["candlesticks"].clone(), ticker, start, end)?;
    Ok(
        json!({"status":"available","asset":asset,"ticker":ticker,"open_time":open,"close_time":close,
        "result":market["result"],"candles":candles,"source":"Archived Kalshi one-minute YES ask quotes",
        "updated_time":market["updated_time"]}),
    )
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

    #[tokio::test]
    async fn study_only_reads_the_fixed_manifest_and_reuses_the_public_projection() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };
        let server = MockServer::start().await;
        let mut board = crate::quote_study::tests::example();
        board["internal"] = json!("excluded");
        let manifest = json!({"partitions":{"opaque":"private rows"},"board":board});
        Mock::given(method("GET"))
            .and(path("/quote_studies_v1/latest"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"fields":{"payload":{"stringValue":manifest.to_string()}}}),
                ),
            )
            .expect(1)
            .mount(&server)
            .await;
        let archive = Archive {
            http: reqwest::Client::new(),
            documents: server.uri(),
            auth: Mutex::new(Some(Auth {
                token: "local-test".into(),
                until: Instant::now() + Duration::from_secs(60),
            })),
            cache: Mutex::new(HashMap::new()),
            collection_lock: Mutex::new(()),
            detail_cache: Mutex::new(HashMap::new()),
            study_cache: Mutex::new(None),
        };
        for _ in 0..2 {
            let public =
                serde_json::to_value(archive.quote_study().await.unwrap().unwrap()).unwrap();
            assert_eq!(public, crate::quote_study::tests::example());
        }
    }

    fn archived_payload() -> Value {
        json!({"market":{"ticker":"KXBTC15M-TEST","result":"yes","open_time":"2026-11-09T20:00:00Z","close_time":"2026-11-09T20:15:00Z"},
        "candlesticks":{"ticker":"KXBTC15M-TEST","candlesticks":[
            {"end_period_ts":1794254460_i64,"yes_ask":{"open_dollars":"0.40","high_dollars":"0.60","low_dollars":"0.30","close_dollars":"0.50"}}
        ]},"private_debug":"must never reach the public API"})
    }
    #[test]
    fn archive_detail_rejects_wrong_identity_future_void_and_wrong_duration() {
        let payload = archived_payload();
        let now = DateTime::parse_from_rfc3339("2026-11-09T21:00:00Z")
            .unwrap()
            .timestamp();
        assert!(normalized_detail(&payload, Asset::Eth, "KXBTC15M-TEST", now).is_err());
        assert!(normalized_detail(&payload, Asset::Btc, "KXBTC15M-OTHER", now).is_err());
        assert!(normalized_detail(&payload, Asset::Btc, "KXBTC15M-TEST", now - 3600).is_err());
        for (key, value) in [("result", "void"), ("close_time", "2026-11-09T20:30:00Z")] {
            let mut bad = payload.clone();
            bad["market"][key] = json!(value);
            assert!(normalized_detail(&bad, Asset::Btc, "KXBTC15M-TEST", now).is_err());
        }
        let mut bad = payload.clone();
        bad["candlesticks"]["ticker"] = json!("KXETH15M-TEST");
        assert!(normalized_detail(&bad, Asset::Btc, "KXBTC15M-TEST", now).is_err());
    }
    #[test]
    fn archive_detail_returns_only_valid_in_window_quotes_and_no_raw_payload() {
        let mut payload = archived_payload();
        let start = DateTime::parse_from_rfc3339("2026-11-09T20:00:00Z")
            .unwrap()
            .timestamp();
        let good = json!({"end_period_ts":start+60,"yes_ask":{"open_dollars":"0.40","high_dollars":"0.60","low_dollars":"0.30","close_dollars":"0.50"}});
        let mut outside = good.clone();
        outside["end_period_ts"] = json!(start);
        let mut impossible = good.clone();
        impossible["yes_ask"]["high_dollars"] = json!("0.1");
        let mut incomplete = good.clone();
        incomplete["yes_ask"]["close_dollars"] = Value::Null;
        payload["candlesticks"]["candlesticks"] =
            json!([outside, good.clone(), good, impossible, incomplete]);
        let result = normalized_detail(&payload, Asset::Btc, "KXBTC15M-TEST", start + 900).unwrap();
        assert_eq!(result["candles"].as_array().unwrap().len(), 1);
        assert_eq!(result["candles"][0]["time"], start);
        assert!(result.get("private_debug").is_none());
        assert!(result.get("market").is_none());
    }
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
