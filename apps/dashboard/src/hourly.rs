//! On-demand public threshold quotes for the next clock hour.
use crate::assets::{Asset, Selection};
use axum::{
    Json,
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::LazyLock, time::Duration};
use tokio::sync::Mutex;

#[derive(Deserialize, Serialize, Clone)]
struct Strike {
    ticker: String,
    close_time: DateTime<Utc>,
    floor_strike: Option<f64>,
    strike_type: String,
    yes_ask_dollars: Option<String>,
    no_ask_dollars: Option<String>,
    status: String,
    yes_sub_title: String,
}
#[derive(Deserialize)]
struct Listing {
    markets: Vec<Strike>,
    cursor: String,
}
#[derive(Serialize, Clone)]
struct Board {
    fetched_at: DateTime<Utc>,
    markets: Vec<Strike>,
}
type Cache = HashMap<Asset, (DateTime<Utc>, Option<Board>)>;
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

async fn read(asset: Asset, now: DateTime<Utc>) -> Option<Board> {
    let series = match asset {
        Asset::Btc => "KXBTCD",
        Asset::Eth => "KXETHD",
        Asset::Sol => "KXSOLD",
    };
    let close = (now.timestamp().div_euclid(3600) + 1) * 3600;
    let url = format!(
        "https://external-api.kalshi.com/trade-api/v2/markets?series_ticker={series}&status=open&min_close_ts={}&max_close_ts={}&limit=1000",
        close - 1,
        close + 1
    );
    let listing: Listing = CLIENT
        .get(url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    if !listing.cursor.is_empty() {
        return None;
    }
    let mut markets = listing.markets;
    markets.retain(|m| {
        m.close_time.timestamp() == close
            && m.status == "active"
            && m.strike_type == "greater"
            && m.floor_strike.is_some_and(f64::is_finite)
    });
    markets.sort_by(|a, b| {
        a.floor_strike
            .partial_cmp(&b.floor_strike)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Some(Board {
        fetched_at: now,
        markets,
    })
}
pub async fn board(Query(selection): Query<Selection>) -> Response {
    let mut cache = CACHE.lock().await;
    let now = Utc::now();
    let entry = cache
        .entry(selection.asset)
        .or_insert((DateTime::UNIX_EPOCH, None));
    if (now - entry.0).num_seconds() >= 20 || now.timestamp() / 3600 != entry.0.timestamp() / 3600 {
        *entry = (now, read(selection.asset, now).await);
    }
    match &entry.1 {
        Some(board) => (
            [(axum::http::header::CACHE_CONTROL, "no-store")],
            Json(board),
        )
            .into_response(),
        None => (StatusCode::BAD_GATEWAY, "Hourly quotes unavailable").into_response(),
    }
}
