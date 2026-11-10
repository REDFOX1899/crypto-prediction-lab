//! Explicit one-shot evaluation. Public requests never call Blockworks or read its key.
use crate::{assets::Asset, outcomes::Archive};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::Read,
    net::IpAddr,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

type Failure = Box<dyn std::error::Error + Send + Sync>;
const API: &str = "https://api.blockworks.com";
const MAX_BYTES: usize = 2_000_000;
const FIELDS: [&str; 6] = ["time", "open", "high", "low", "close", "volume"];

#[derive(Clone, Serialize, Deserialize)]
pub struct Candle {
    pub(crate) time: i64,
    pub(crate) open: f64,
    pub(crate) high: f64,
    pub(crate) low: f64,
    pub(crate) close: f64,
    pub(crate) volume: Option<f64>,
}
impl Candle {
    fn valid(&self, start: i64, end: i64) -> bool {
        self.time % 300 == 0
            && self.time >= start
            && self.time < end
            && [self.open, self.high, self.low, self.close]
                .iter()
                .all(|v| v.is_finite() && *v > 0.)
            && self.low <= self.open.min(self.close)
            && self.high >= self.open.max(self.close)
            && self.volume.is_none_or(|v| v.is_finite() && v >= 0.)
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Series {
    pub(crate) asset: Asset,
    pub(crate) asset_id: String,
    pub(crate) candles: Vec<Candle>,
    pub(crate) rejected_points: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub(crate) schema_version: u8,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) granularity: String,
    pub(crate) captured_at: i64,
    pub(crate) start: i64,
    pub(crate) end: i64,
    pub(crate) volume_unit: String,
    pub(crate) series: Vec<Series>,
}
fn slug(asset: Asset) -> &'static str {
    match asset {
        Asset::Btc => "bitcoin",
        Asset::Eth => "ethereum",
        Asset::Sol => "solana",
    }
}
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, Failure> {
    let file = std::fs::File::open(path).map_err(|_| "Blockworks file unavailable")?;
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Blockworks file unreadable")?;
    if bytes.len() > limit {
        return Err("Blockworks file exceeds size limit".into());
    }
    Ok(bytes)
}
/// An immutable local projection, loaded once. It cannot expose data on a public bind.
pub fn local_board(address: IpAddr) -> Result<Value, Failure> {
    let Ok(path) = std::env::var("BLOCKWORKS_LOCAL_SNAPSHOT") else {
        return Ok(json!({"status":"not_configured","series":[]}));
    };
    if !address.is_loopback() {
        return Err("Blockworks evaluation snapshots require a loopback address".into());
    }
    let bytes = read_bounded(Path::new(&path), MAX_BYTES)?;
    let snapshot: Snapshot =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid Blockworks snapshot")?;
    validate(&snapshot, Utc::now().timestamp())?;
    Ok(json!({"status":"available","snapshot":snapshot}))
}
fn validate(snapshot: &Snapshot, now: i64) -> Result<(), Failure> {
    if snapshot.schema_version != 1
        || snapshot.provider != "Blockworks"
        || snapshot.model != "asset-price"
        || snapshot.granularity != "5m"
        || snapshot.start < 0
        || snapshot.end <= snapshot.start
        || snapshot.end - snapshot.start > 7 * 86400
        || snapshot.start % 300 != 0
        || snapshot.end % 300 != 0
        || snapshot.captured_at < 300
        || snapshot.end > snapshot.captured_at - 300
        || snapshot.captured_at > now + 60
        || !["usd", "unverified"].contains(&snapshot.volume_unit.as_str())
        || snapshot.series.len() != 3
    {
        return Err("Invalid Blockworks snapshot bounds or identity".into());
    }
    let mut seen = HashSet::new();
    let mut ids = HashSet::new();
    for series in &snapshot.series {
        if !seen.insert(series.asset)
            || !valid_id(&series.asset_id)
            || !ids.insert(&series.asset_id)
            || series.candles.len() > 2016
            || series.candles.windows(2).any(|p| p[0].time >= p[1].time)
            || series
                .candles
                .iter()
                .any(|c| !c.valid(snapshot.start, snapshot.end))
        {
            return Err("Invalid Blockworks series".into());
        }
    }
    Ok(())
}
/// Licensed display reads saved observations only. No web request can invoke the provider.
pub struct Service {
    local: Option<Snapshot>,
    archive: Option<Arc<Archive>>,
    allowed: bool,
    cache: Mutex<Option<(Instant, Option<Snapshot>)>>,
}
impl Service {
    pub fn new(address: IpAddr, archive: Option<Arc<Archive>>) -> Result<Arc<Self>, Failure> {
        let board = local_board(address)?;
        let local = board
            .get("snapshot")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()
            .map_err(|_| "Invalid local research snapshot")?;
        Ok(Arc::new(Self {
            allowed: local.is_some()
                || std::env::var("BLOCKWORKS_PUBLIC_DISPLAY").as_deref() == Ok("approved"),
            local,
            archive,
            cache: Mutex::new(None),
        }))
    }
    pub(crate) async fn snapshot(&self) -> Result<Option<Snapshot>, Failure> {
        if !self.allowed {
            return Ok(None);
        }
        if let Some(s) = &self.local {
            return Ok(Some(s.clone()));
        }
        let mut cache = self.cache.lock().await;
        if let Some((at, value)) = &*cache
            && at.elapsed() < Duration::from_secs(600)
        {
            return Ok(value.clone());
        }
        let Some(archive) = &self.archive else {
            return Ok(None);
        };
        // Cache absence before attempting the reads, avoiding visitor-triggered retry storms.
        *cache = Some((Instant::now(), None));
        let Some(mut manifest) = archive.blockworks_document("latest").await? else {
            return Ok(None);
        };
        let capture = manifest["captured_at"]
            .as_i64()
            .filter(|t| *t > 0)
            .ok_or("Invalid research capture")?;
        if manifest["public_display_confirmed"] != true {
            return Err("Research display not approved".into());
        }
        let mut series = Vec::new();
        for asset in Asset::ALL {
            let symbol = serde_json::to_value(asset)?;
            let id = format!("p{capture}_{}", symbol.as_str().ok_or("Invalid asset")?);
            let row = archive
                .blockworks_document(&id)
                .await?
                .ok_or("Research series missing")?;
            if row["captured_at"] != capture || row["series"]["asset"] != symbol {
                return Err("Research capture mismatch".into());
            }
            series.push(row["series"].clone());
        }
        manifest["series"] = json!(series);
        let snapshot: Snapshot =
            serde_json::from_value(manifest).map_err(|_| "Invalid saved research snapshot")?;
        validate(&snapshot, Utc::now().timestamp())?;
        *cache = Some((Instant::now(), Some(snapshot.clone())));
        Ok(Some(snapshot))
    }
    pub async fn board(&self) -> Value {
        match self.snapshot().await {
            Ok(Some(mut s)) => {
                let pilot = json!({"start":s.start,"end":s.end,"captured_at":s.captured_at,
                    "valid_candles":s.series.iter().map(|r| r.candles.len()).sum::<usize>(),
                    "expected_candles":(s.end-s.start)/300*3});
                // Public chart response is bounded to one day; the seven-day pilot stays server-side.
                s.start = s.start.max(s.end - 86400);
                for row in &mut s.series {
                    row.candles.retain(|c| c.time >= s.start);
                }
                json!({"status":"available","snapshot":s,"pilot":pilot})
            }
            Ok(None) => json!({"status":"not_configured","series":[]}),
            Err(_) => json!({"status":"unavailable","series":[]}),
        }
    }
}

fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.chars().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}
async fn request(
    http: &reqwest::Client,
    key: &str,
    path: &str,
    params: &[(&str, &str)],
) -> Result<Value, Failure> {
    let mut auth =
        reqwest::header::HeaderValue::from_str(key).map_err(|_| "Invalid Blockworks key format")?;
    auth.set_sensitive(true);
    let mut response = http
        .get(format!("{API}{path}"))
        .header("X-Blockworks-API-Key", auth)
        .header("Accept", "application/json")
        .query(params)
        .send()
        .await
        .map_err(|_| "Blockworks connection failed; stopped without retry")?;
    if !response.status().is_success() {
        return Err(format!(
            "Blockworks HTTP {}; stopped without retry. No response body or credentials logged.",
            response.status().as_u16()
        )
        .into());
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BYTES as u64)
    {
        return Err("Blockworks response too large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Blockworks response unreadable")?
    {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err("Blockworks response too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid Blockworks response")?;
    if value.get("error") != Some(&Value::Null) {
        return Err("Blockworks provider error; stopped without retry".into());
    }
    Ok(value)
}
fn check_catalog(value: &Value) -> Result<(), Failure> {
    let models = value["data"]["models"]
        .as_array()
        .ok_or("Invalid Blockworks catalog")?;
    let model = models
        .iter()
        .find(|m| m["slug"] == "asset-price")
        .ok_or("Blockworks asset-price access not enabled")?;
    if !model["intervals"]
        .as_array()
        .is_some_and(|v| v.iter().any(|i| i == "5m"))
    {
        return Err("Blockworks five-minute history unavailable".into());
    }
    let columns = model["columns"]
        .as_array()
        .ok_or("Invalid Blockworks catalog columns")?;
    for field in FIELDS {
        if !columns.iter().any(|c| c["field"] == field) {
            return Err("Required Blockworks candle fields unavailable".into());
        }
    }
    Ok(())
}
fn assets(value: &Value) -> Result<Vec<(Asset, String)>, Failure> {
    let rows = value["data"].as_array().ok_or("Invalid asset lookup")?;
    if rows.len() != 3 {
        return Err("Blockworks asset lookup incomplete or ambiguous".into());
    }
    let mut result = Vec::new();
    for asset in Asset::ALL {
        let matches: Vec<_> = rows.iter().filter(|r| r["slug"] == slug(asset)).collect();
        if matches.len() != 1 || matches[0]["name"] != asset.info().name {
            return Err("Blockworks canonical asset identity mismatch".into());
        }
        let id = matches[0]["assetID"]
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or("Invalid Blockworks asset ID")?;
        result.push((asset, id.to_owned()));
    }
    if result
        .iter()
        .map(|(_, id)| id)
        .collect::<HashSet<_>>()
        .len()
        != 3
    {
        return Err("Duplicate Blockworks asset IDs".into());
    }
    Ok(result)
}
fn normalize(
    value: &Value,
    assets: &[(Asset, String)],
    start: i64,
    end: i64,
    captured_at: i64,
) -> Result<Snapshot, Failure> {
    let data = &value["data"];
    let schema = data["point_schema"]
        .as_array()
        .ok_or("Missing point schema")?;
    let index: Vec<usize> = FIELDS
        .iter()
        .map(|field| {
            let positions: Vec<_> = schema
                .iter()
                .enumerate()
                .filter(|(_, c)| c["field"] == *field)
                .map(|(i, _)| i)
                .collect();
            if positions.len() == 1 {
                Ok(positions[0])
            } else {
                Err("Missing or ambiguous Blockworks point field")
            }
        })
        .collect::<Result<_, _>>()?;
    if index[0] != 0 || index[1..5].iter().any(|i| schema[*i]["unit"] != "usd") {
        return Err("Unverified Blockworks timestamp or price units".into());
    }
    let rows = data["series"]
        .as_array()
        .ok_or("Missing Blockworks series")?;
    if rows.len() > 3 {
        return Err("Unexpected Blockworks series".into());
    }
    let mut seen = HashSet::new();
    let mut series = Vec::new();
    for row in rows {
        let id = row["key"].as_str().ok_or("Missing Blockworks series ID")?;
        let (asset, _) = assets
            .iter()
            .find(|(_, k)| k == id)
            .ok_or("Unknown Blockworks series ID")?;
        if !seen.insert(*asset) {
            return Err("Duplicate Blockworks series".into());
        }
        let points = row["points"]
            .as_array()
            .ok_or("Missing Blockworks points")?;
        if points.len() > 2016 {
            return Err("Blockworks pilot point limit exceeded".into());
        }
        let mut candles = Vec::new();
        let mut times = HashSet::new();
        let mut rejected_points = 0;
        for point in points {
            let Some(p) = point.as_array().filter(|p| p.len() == schema.len()) else {
                rejected_points += 1;
                continue;
            };
            let candle = (|| {
                Some(Candle {
                    time: p[index[0]].as_i64()?,
                    open: p[index[1]].as_f64()?,
                    high: p[index[2]].as_f64()?,
                    low: p[index[3]].as_f64()?,
                    close: p[index[4]].as_f64()?,
                    volume: if p[index[5]].is_null() {
                        None
                    } else {
                        Some(p[index[5]].as_f64()?)
                    },
                })
            })();
            if let Some(c) = candle.filter(|c| c.valid(start, end)) {
                if !times.insert(c.time) {
                    return Err(
                        "Duplicate Blockworks candle timestamps; inspect provider revisions".into(),
                    );
                }
                candles.push(c);
            } else {
                rejected_points += 1;
            }
        }
        candles.sort_by_key(|c| c.time);
        series.push(Series {
            asset: *asset,
            asset_id: id.into(),
            candles,
            rejected_points,
        });
    }
    for (asset, id) in assets {
        if !seen.contains(asset) {
            series.push(Series {
                asset: *asset,
                asset_id: id.clone(),
                candles: vec![],
                rejected_points: 0,
            });
        }
    }
    let snapshot = Snapshot {
        schema_version: 1,
        provider: "Blockworks".into(),
        model: "asset-price".into(),
        granularity: "5m".into(),
        captured_at,
        start,
        end,
        volume_unit: if schema[index[5]]["unit"] == "usd" {
            "usd"
        } else {
            "unverified"
        }
        .into(),
        series,
    };
    validate(&snapshot, captured_at)?;
    Ok(snapshot)
}
/// At most three sequential requests: accessible catalog, exact asset lookup, one batch.
/// No retries, pagination, cloud writes, recurring worker or HTTP collection route.
pub async fn pilot() -> Result<(), Failure> {
    let days: i64 = std::env::var("BLOCKWORKS_PILOT_DAYS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .map_err(|_| "Pilot days must be 1 or 7")?;
    if ![1, 7].contains(&days) {
        return Err("Pilot days must be 1 or 7".into());
    }
    let key_path =
        std::env::var("BLOCKWORKS_API_KEY_PATH").map_err(|_| "Set BLOCKWORKS_API_KEY_PATH")?;
    let output = std::env::var("BLOCKWORKS_PILOT_OUTPUT")
        .map_err(|_| "Set a new BLOCKWORKS_PILOT_OUTPUT path")?;
    if Path::new(&output).exists() {
        return Err("Pilot output already exists; no requests made".into());
    }
    let bytes = read_bounded(Path::new(&key_path), 512)?;
    let key = std::str::from_utf8(&bytes)
        .map_err(|_| "Invalid Blockworks key encoding")?
        .trim();
    if key.is_empty() || !key.bytes().all(|c| c.is_ascii_graphic()) {
        return Err("Key file must contain a single ASCII token".into());
    }
    let http = reqwest::Client::builder()
        .user_agent("CryptoPredictionLab/0.1 (read-only Blockworks evaluation)")
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "Blockworks client unavailable")?;
    println!(
        "One-shot pilot: at most 3 requests / {} days / {} points. Credit cost unverified. No automatic retries.",
        days,
        days * 864
    );
    let catalog = request(
        &http,
        key,
        "/query/timeseries/catalog",
        &[("accessible", "true"), ("publicOnly", "true")],
    )
    .await?;
    check_catalog(&catalog)?;
    let lookup = request(
        &http,
        key,
        "/query/tabular/assets",
        &[
            ("slugIsOneOf", "bitcoin,ethereum,solana"),
            ("selections", "assetID,slug,name"),
            ("pageSize", "3"),
        ],
    )
    .await?;
    let ids = assets(&lookup)?;
    // Leave one full interval as a conservative embargo; source publication delay is unverified.
    let end = Utc::now().timestamp() / 300 * 300 - 300;
    let start = end - days * 86400;
    let keys = ids
        .iter()
        .map(|(_, id)| id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let start_text = start.to_string();
    let end_text = end.to_string();
    let data = request(
        &http,
        key,
        "/query/timeseries/asset-price/5m",
        &[
            ("series", &keys),
            ("start", &start_text),
            ("end", &end_text),
            ("selections", "open,high,low,close,volume"),
        ],
    )
    .await?;
    let snapshot = normalize(&data, &ids, start, end, Utc::now().timestamp())?;
    let encoded = serde_json::to_vec(&snapshot).map_err(|_| "Snapshot encoding failed")?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&output)
        .map_err(|_| "Cannot create new pilot output")?;
    use std::io::Write;
    file.write_all(&encoded)
        .map_err(|_| "Cannot write pilot output")?;
    println!(
        "Pilot snapshot saved. Compare account credit usage before running another pilot. Load locally with BLOCKWORKS_LOCAL_SNAPSHOT; public display remains disabled."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ids() -> Vec<(Asset, String)> {
        Asset::ALL
            .into_iter()
            .enumerate()
            .map(|(i, a)| (a, format!("00000000-0000-0000-0000-{i:012}")))
            .collect()
    }
    fn payload() -> Value {
        // Deliberately reorder provider columns: requested selections do not define order.
        json!({"data":{"point_schema":[{"field":"time"},{"field":"volume"},{"field":"close","unit":"usd"},{"field":"low","unit":"usd"},{"field":"open","unit":"usd"},{"field":"high","unit":"usd"}],"series":[{"key":ids()[0].1,"points":[[0,null,11.,9.,10.,12.],[300,0.,10.,9.,11.,12.]]}]}})
    }
    #[test]
    fn maps_schema_order_preserves_null_and_zero_and_missing_series() {
        let s = normalize(&payload(), &ids(), 0, 600, 900).unwrap();
        assert_eq!(s.series.len(), 3);
        let c = &s.series[0].candles;
        assert_eq!(c[0].open, 10.);
        assert_eq!(c[0].close, 11.);
        assert_eq!(c[0].volume, None);
        assert_eq!(c[1].volume, Some(0.));
        assert!(s.series[1].candles.is_empty());
        assert_eq!(s.volume_unit, "unverified");
    }
    #[test]
    fn malformed_ohlc_outside_bounds_and_non_numeric_points_remain_gaps() {
        let mut p = payload();
        p["data"]["series"][0]["points"] = json!([
            [0, 1., 11., 9., 10., 8.],
            [300, "oops", 10., 9., 11., 12.],
            [600, 2., 10., 9., 11., 12.],
            [1, 2., 10., 9., 11., 12.]
        ]);
        let s = normalize(&p, &ids(), 0, 600, 900).unwrap();
        assert!(s.series[0].candles.is_empty());
        assert_eq!(s.series[0].rejected_points, 4);
    }
    #[test]
    fn fails_on_duplicate_times_ambiguous_schema_unknown_series_and_wrong_units() {
        let mut p = payload();
        p["data"]["series"][0]["points"][1][0] = json!(0);
        assert!(normalize(&p, &ids(), 0, 600, 900).is_err());
        p = payload();
        p["data"]["point_schema"][2]["field"] = json!("volume");
        assert!(normalize(&p, &ids(), 0, 600, 900).is_err());
        p = payload();
        p["data"]["point_schema"][2]["unit"] = json!("btc");
        assert!(normalize(&p, &ids(), 0, 600, 900).is_err());
        p = payload();
        p["data"]["series"][0]["key"] = json!("unknown");
        assert!(normalize(&p, &ids(), 0, 600, 900).is_err());
    }
    #[test]
    fn snapshots_reject_wrong_provider_future_capture_and_excess_horizon() {
        let mut s = normalize(&payload(), &ids(), 0, 600, 900).unwrap();
        s.provider = "Other".into();
        assert!(validate(&s, 900).is_err());
        s.provider = "Blockworks".into();
        s.captured_at = 1000;
        assert!(validate(&s, 900).is_err());
        s.captured_at = 700000;
        s.end = 650100;
        assert!(validate(&s, 700000).is_err());
    }
    #[test]
    fn catalog_and_canonical_lookup_fail_closed() {
        assert!(check_catalog(&json!({"data":{"models":[]}})).is_err());
        let rows: Vec<_> = ids()
            .iter()
            .map(|(a, id)| json!({"assetID":id,"slug":slug(*a),"name":a.info().name}))
            .collect();
        let good = json!({"data":rows});
        assert_eq!(assets(&good).unwrap().len(), 3);
        let mut bad = good;
        bad["data"][1]["name"] = json!("Wrapped Ethereum");
        assert!(assets(&bad).is_err());
    }
    #[test]
    fn snapshot_projection_drops_extra_fields() {
        let s = normalize(&payload(), &ids(), 0, 600, 900).unwrap();
        let mut value = serde_json::to_value(&s).unwrap();
        value["private_debug"] = json!("private");
        let parsed: Snapshot = serde_json::from_value(value).unwrap();
        assert!(!serde_json::to_string(&parsed).unwrap().contains("private"));
    }
}
