//! Scheduled market context. The key exists only in the IAM-protected collector.
use crate::outcomes::Archive;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
type Failure = Box<dyn std::error::Error + Send + Sync>;
const API: &str = "https://pro-api.coinmarketcap.com";
const IDS: [(u64, &str); 3] = [(1, "BTC"), (1027, "ETH"), (5426, "SOL")];

pub struct Context {
    http: reqwest::Client,
    archive: Option<Arc<Archive>>,
    cache: Mutex<Option<(Instant, Value)>>,
    research_cache: Mutex<Option<(Instant, Value)>>,
    collecting: Mutex<()>,
}
impl Context {
    pub fn new(archive: Option<Arc<Archive>>) -> Result<Arc<Self>, Failure> {
        Ok(Arc::new(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()?,
            archive,
            cache: Mutex::new(None),
            research_cache: Mutex::new(None),
            collecting: Mutex::new(()),
        }))
    }
    pub async fn board(&self) -> Value {
        let mut cache = self.cache.lock().await;
        if let Some((at, v)) = &*cache
            && at.elapsed() < Duration::from_secs(120)
        {
            return v.clone();
        }
        let mut result = json!({"status":"unavailable","latest":null,"history":[]});
        if let Some(a) = &self.archive
            && let Ok(Some(latest)) = a.context_get().await
        {
            result = json!({"status":"available","latest":latest,"history":a.context_recent(24).await.unwrap_or_default()});
        }
        *cache = Some((Instant::now(), result.clone()));
        result
    }
    /// A bounded research projection; does not call the provider or collect data.
    pub async fn research_board(&self) -> Value {
        let mut cache = self.research_cache.lock().await;
        if let Some((at, value)) = &*cache
            && at.elapsed() < Duration::from_secs(300)
        {
            return value.clone();
        }
        let now = Utc::now();
        let result = if let Some(archive) = &self.archive {
            match archive.context_recent(288).await {
                Ok(rows) => {
                    json!({"status":"available","history":research_rows(rows,now),"fetched_at":now.to_rfc3339(),"horizon_hours":24})
                }
                Err(_) => json!({"status":"unavailable","history":[]}),
            }
        } else {
            json!({"status":"not_configured","history":[]})
        };
        *cache = Some((Instant::now(), result.clone()));
        result
    }
    async fn request(&self, key: &str, path: &str) -> Result<Value, Failure> {
        // Never include request errors, provider response bodies or authentication in logs.
        let response = self
            .http
            .get(format!("{API}{path}"))
            .header("X-CMC_PRO_API_KEY", key)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|_| "CMC request failed")?;
        if !response.status().is_success() {
            return Err(format!("CMC request unavailable: HTTP {}", response.status()).into());
        }
        if response.content_length().is_some_and(|s| s > 512_000) {
            return Err("CMC response too large".into());
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| "CMC response unavailable")?;
        if bytes.len() > 512_000 {
            return Err("CMC response too large".into());
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid CMC response")?;
        if !successful(&value) {
            return Err("CMC provider error".into());
        }
        Ok(value)
    }
    pub async fn collect(&self) -> Result<Value, Failure> {
        let _guard = self
            .collecting
            .try_lock()
            .map_err(|_| "Collection in progress")?;
        let archive = self
            .archive
            .as_ref()
            .ok_or("Context archive not configured")?;
        let path = std::env::var("CMC_API_KEY_PATH").map_err(|_| "CMC key not configured")?;
        let key = std::fs::read_to_string(path).map_err(|_| "CMC key unavailable")?;
        let key = key.trim();
        if key.is_empty() || key.len() > 256 || key.contains('\n') {
            return Err("Invalid CMC key file".into());
        }
        let now = Utc::now();
        let mut latest = archive.context_get().await?.unwrap_or_else(
            || json!({"schema_version":1,"quotes":[],"global":null,"sentiment":null}),
        );
        if !due(&latest, "quotes_checked_at", now, 295) {
            return Ok(json!({"status":"cached"}));
        }
        let usage = self.request(key, "/v1/key/info").await?;
        if !within_budget(&usage) {
            return Ok(json!({"status":"quota_paused"}));
        }
        let quotes = self
            .request(
                key,
                "/v3/cryptocurrency/quotes/latest?id=1,1027,5426&convert=USD",
            )
            .await?;
        let normalized = normalize_quotes(&quotes)?;
        let changed = latest["quotes"] != normalized;
        latest["quotes"] = normalized;
        latest["quotes_checked_at"] = json!(now.to_rfc3339());
        // Persist each successful step: retries never multiply successful upstream calls.
        archive.context_put(&latest, changed).await?;
        let mut complete = true;
        for (field, path, interval) in [
            (
                "global",
                "/v1/global-metrics/quotes/latest?convert=USD",
                3600,
            ),
            ("sentiment", "/v3/fear-and-greed/latest", 86400),
        ] {
            let checked = format!("{field}_checked_at");
            if !due(&latest, &checked, now, interval) {
                continue;
            }
            match self
                .request(key, path)
                .await
                .and_then(|v| normalize_extra(field, &v))
            {
                Ok(value) => {
                    latest[field] = value;
                    latest[&checked] = json!(now.to_rfc3339());
                    archive.context_put(&latest, false).await?;
                }
                Err(_) => complete = false,
            }
        }
        *self.cache.lock().await = None;
        Ok(json!({"status":if complete {"collected"}else{"partial"},"assets":3}))
    }
}
fn research_rows(rows: Vec<Value>, now: DateTime<Utc>) -> Vec<Value> {
    rows.into_iter()
        .take(288)
        .filter_map(|row| {
            let at = row["at"].as_str()?;
            let timestamp = DateTime::parse_from_rfc3339(at).ok()?.timestamp();
            if timestamp > now.timestamp() || timestamp < now.timestamp() - 86400 {
                return None;
            }
            let quotes: Vec<Value> = row["quotes"]
                .as_array()?
                .iter()
                .filter_map(|quote| {
                    let symbol = quote["symbol"].as_str()?;
                    let price = quote["price"].as_f64()?;
                    let source = quote["source_at"].as_str()?;
                    if !IDS.iter().any(|(_, s)| *s == symbol)
                        || !price.is_finite()
                        || price <= 0.0
                        || DateTime::parse_from_rfc3339(source).is_err()
                    {
                        return None;
                    }
                    Some(json!({"symbol":symbol,"price":quote["price"],"source_at":source}))
                })
                .collect();
            Some(json!({"at":at,"quotes":quotes}))
        })
        .collect()
}
fn successful(value: &Value) -> bool {
    let code = &value["status"]["error_code"];
    code.as_i64() == Some(0) || code.as_str() == Some("0")
}
fn due(value: &Value, field: &str, now: DateTime<Utc>, seconds: i64) -> bool {
    value[field]
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .is_none_or(|t| {
            (now - t.with_timezone(&Utc)).num_seconds() >= seconds
                || t.timestamp() > now.timestamp() + 60
        })
}
fn within_budget(usage: &Value) -> bool {
    let used = usage
        .pointer("/data/usage/current_month/credits_used")
        .and_then(Value::as_u64);
    let limit = usage
        .pointer("/data/plan/credit_limit_monthly")
        .and_then(Value::as_u64);
    matches!((used,limit),(Some(u),Some(l)) if l>0 && u < 12_000.min(l*9/10))
}
fn number(v: &Value) -> Value {
    v.as_f64()
        .filter(|n| n.is_finite())
        .map_or(Value::Null, |n| json!(n))
}
fn stamp(v: &Value) -> Result<Value, Failure> {
    let s = v.as_str().ok_or("Missing source timestamp")?;
    let at = DateTime::parse_from_rfc3339(s).map_err(|_| "Invalid source timestamp")?;
    if at.timestamp() > Utc::now().timestamp() + 120 {
        return Err("Future source timestamp".into());
    }
    Ok(json!(at.to_rfc3339()))
}
fn normalize_quotes(value: &Value) -> Result<Value, Failure> {
    let rows = value["data"].as_array().ok_or("Invalid quote response")?;
    let mut result = Vec::new();
    for (id, symbol) in IDS {
        let row = rows
            .iter()
            .find(|r| r["id"].as_u64() == Some(id) && r["symbol"].as_str() == Some(symbol))
            .ok_or("Missing required asset")?;
        let quote = row["quote"]
            .as_array()
            .and_then(|qs| qs.iter().find(|q| q["symbol"] == "USD"))
            .ok_or("Missing USD quote")?;
        if !quote["price"]
            .as_f64()
            .is_some_and(|n| n.is_finite() && n > 0.)
        {
            return Err("Invalid price".into());
        }
        let mut item = json!({"id":id,"symbol":symbol,"source_at":stamp(&quote["last_updated"])?});
        for f in [
            "price",
            "percent_change_1h",
            "percent_change_24h",
            "percent_change_7d",
            "volume_24h",
            "volume_change_24h",
            "market_cap",
            "market_cap_dominance",
        ] {
            item[f] = number(&quote[f]);
        }
        result.push(item);
    }
    Ok(json!(result))
}
fn normalize_extra(field: &str, v: &Value) -> Result<Value, Failure> {
    let d = &v["data"];
    if field == "global" {
        let q = &d["quote"]["USD"];
        Ok(
            json!({"source_at":stamp(&q["last_updated"])?,"market_cap":number(&q["total_market_cap"]),"volume_24h":number(&q["total_volume_24h"]),"change_24h":number(&q["total_market_cap_yesterday_percentage_change"]),"btc_dominance":number(&d["btc_dominance"]),"eth_dominance":number(&d["eth_dominance"])}),
        )
    } else {
        let n = d["value"]
            .as_f64()
            .filter(|n| n.is_finite() && (0.0..=100.0).contains(n))
            .ok_or("Invalid sentiment")?;
        Ok(
            json!({"source_at":stamp(&d["update_time"])?,"value":n,"label":d["value_classification"].as_str().unwrap_or("Unavailable")}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn research_history_bounds_time_and_projects_only_supported_price_observations() {
        let now = Utc::now();
        let row = |at: DateTime<Utc>| {
            json!({"at":at.to_rfc3339(),"private_debug":"never public","quotes":[
                {"symbol":"BTC","price":10,"source_at":at.to_rfc3339(),"extra":"not exposed"},
                {"symbol":"DOGE","price":20,"source_at":at.to_rfc3339()},
                {"symbol":"SOL","price":0,"source_at":at.to_rfc3339()},
                {"symbol":"ETH","price":30,"source_at":"invalid"}
            ]})
        };
        let result = research_rows(
            vec![
                row(now),
                row(now - chrono::Duration::hours(25)),
                row(now + chrono::Duration::minutes(1)),
            ],
            now,
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["quotes"].as_array().unwrap().len(), 1);
        assert!(result[0].get("private_debug").is_none());
        assert!(result[0]["quotes"][0].get("extra").is_none());
        assert_eq!(research_rows(vec![row(now); 400], now).len(), 288);
    }
    #[test]
    fn provider_success_accepts_numeric_and_string_zero_only() {
        assert!(successful(&json!({"status":{"error_code":0}})));
        assert!(successful(&json!({"status":{"error_code":"0"}})));
        assert!(!successful(&json!({"status":{"error_code":"1006"}})));
        assert!(!successful(&json!({"status":{"error_code":1006}})));
        assert!(!successful(&json!({})));
    }
    #[test]
    fn budget_fails_closed_and_reserves_credits() {
        assert!(!within_budget(&json!({})));
        let usage = |u, l| json!({"data":{"plan":{"credit_limit_monthly":l},"usage":{"current_month":{"credits_used":u}}}});
        assert!(within_budget(&usage(9000, 15000)));
        assert!(!within_budget(&usage(12000, 15000)));
        assert!(!within_budget(&usage(9000, 10000)));
    }
    #[test]
    fn cadence_handles_restart_and_future_clock() {
        let now = Utc::now();
        assert!(!due(&json!({"t":now.to_rfc3339()}), "t", now, 295));
        assert!(due(&json!({}), "t", now, 295));
        assert!(due(
            &json!({"t":(now+chrono::Duration::hours(1)).to_rfc3339()}),
            "t",
            now,
            295
        ));
    }
    #[test]
    fn quotes_require_exact_assets_and_source_times() {
        let rows: Vec<_>=IDS.iter().map(|(id,s)|json!({"id":id,"symbol":s,"quote":[{"symbol":"USD","price":10.,"last_updated":Utc::now().to_rfc3339()}]})).collect();
        assert_eq!(
            normalize_quotes(&json!({"data":rows}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let mut bad = rows.clone();
        bad[2]["symbol"] = json!("OTHER");
        assert!(normalize_quotes(&json!({"data":bad})).is_err());
        bad = rows;
        bad[1]["quote"][0]["last_updated"] = json!("invalid");
        assert!(normalize_quotes(&json!({"data":bad})).is_err());
    }
}
