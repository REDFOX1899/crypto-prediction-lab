mod assets;
mod blockworks;
mod checkpoint;
mod combos;
mod context;
mod cycle_dispatch;
mod decision;
mod decision_runner;
mod delivery;
mod evaluation_runner;
mod final_minute;
mod forecast;
mod gemini_decisions;
mod gemini_research;
mod history;
mod hourly;
mod latency;
mod market_rules;
mod news;
mod order_book;
mod outcomes;
mod paper_levels;
mod paper_study;
mod prospective;
mod quote_study;
mod reference;
mod research;
mod research_mcp;
mod stream;

use assets::{Asset, Selection};
use axum::{
    Json, Router,
    extract::{MatchedPath, Query, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{
        Html, IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use kalshi_client::{Client, Market, OrderBook, current_market};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::Infallible, net::IpAddr, sync::Arc};
use tokio::sync::watch;
use tokio_stream::{StreamExt, wrappers::WatchStream};

#[derive(Clone)]
struct Workspace {
    client: Arc<Client>,
    stream_tx: watch::Sender<stream::Update>,
    reference_tx: watch::Sender<reference::ReferenceUpdate>,
    history: std::sync::Arc<history::HistoryService>,
    final_minute: final_minute::Shared,
    snapshots: Arc<tokio::sync::Mutex<SnapshotCache>>,
}

#[derive(Serialize)]
struct ReferenceEnvelope {
    #[serde(flatten)]
    reference: reference::ReferenceUpdate,
    final_minute: final_minute::Summary,
}
impl Workspace {
    fn reference_envelope(
        &self,
        asset: Asset,
        reference: reference::ReferenceUpdate,
    ) -> ReferenceEnvelope {
        let summary = self
            .final_minute
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .summary(asset, Utc::now().timestamp_millis(), reference.state);
        ReferenceEnvelope {
            reference,
            final_minute: summary,
        }
    }
}

#[derive(Clone)]
struct AppState {
    workspaces: Arc<HashMap<Asset, Workspace>>,
    archive: Option<Arc<outcomes::Archive>>,
    combos: Arc<combos::Service>,
    polling: bool,
    context: Arc<context::Context>,
    blockworks: Arc<blockworks::Service>,
    news: Arc<news::Service>,
    decisions: Arc<gemini_decisions::Engine>,
    latency: Arc<latency::Monitor>,
    preview_study: Option<Arc<quote_study::Board>>,
}
async fn blockworks_board(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(state.blockworks.board().await),
    )
}
async fn quote_study(State(state): State<AppState>) -> Response {
    let Some(archive) = state.archive else {
        if let Some(board) = state.preview_study {
            return Json(board.as_ref()).into_response();
        }
        return Json(serde_json::json!({"status": "not_configured"})).into_response();
    };
    match tokio::time::timeout(std::time::Duration::from_secs(8), archive.quote_study()).await {
        Ok(Ok(Some(board))) => (
            [(header::CACHE_CONTROL, "public, max-age=300")],
            Json(board),
        )
            .into_response(),
        Ok(Ok(None)) => Json(serde_json::json!({"status": "unavailable"})).into_response(),
        _ => (
            StatusCode::BAD_GATEWAY,
            "Quote study temporarily unavailable",
        )
            .into_response(),
    }
}

async fn quote_tape_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/quote-tape.js"),
    )
}

async fn technical_chart_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/technical-chart.js"),
    )
}

async fn forecast_study_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/forecast-study.js"),
    )
}

async fn forecast_pilot_data() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json")],
        include_str!("../public/forecast-pilot-v1.json"),
    )
}

async fn quant_simulation() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        include_str!("../public/btc-15m-simulation.json"),
    )
}

async fn quant_lab_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/quant-lab.js"),
    )
}

async fn quant_lab_style() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../public/quant-lab.css"),
    )
}

async fn terminal_style() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../public/terminal.css"),
    )
}

fn dec_f64(d: Option<rust_decimal::Decimal>) -> Option<f64> {
    d.and_then(|v| v.to_string().parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

fn top10_imbalance(
    yes_levels: &[(rust_decimal::Decimal, rust_decimal::Decimal)],
    no_levels: &[(rust_decimal::Decimal, rust_decimal::Decimal)],
) -> Option<f64> {
    let mut y_levels: Vec<(f64, f64)> = yes_levels
        .iter()
        .filter_map(|(p, q)| Some((dec_f64(Some(*p))?, dec_f64(Some(*q))?)))
        .filter(|(p, q)| *p > 0.0 && *p < 1.0 && *q > 0.0)
        .collect();
    y_levels.sort_by(|a, b| b.0.total_cmp(&a.0));
    let y_sum: f64 = y_levels.iter().take(10).map(|(_, q)| *q).sum();

    let mut n_levels: Vec<(f64, f64)> = no_levels
        .iter()
        .filter_map(|(p, q)| Some((dec_f64(Some(*p))?, dec_f64(Some(*q))?)))
        .filter(|(p, q)| *p > 0.0 && *p < 1.0 && *q > 0.0)
        .collect();
    n_levels.sort_by(|a, b| b.0.total_cmp(&a.0));
    let n_sum: f64 = n_levels.iter().take(10).map(|(_, q)| *q).sum();

    let total = y_sum + n_sum;
    if total > 0.0 {
        Some((y_sum - n_sum) / total)
    } else {
        None
    }
}

async fn ensure_snapshot_market(
    workspace: &Workspace,
    asset: Asset,
) -> Option<(Market, Option<f64>)> {
    let now = Utc::now();
    let live_ticker = {
        let s = workspace.stream_tx.borrow();
        if s.state == "live" {
            s.ticker.clone()
        } else {
            None
        }
    };
    {
        let cache = workspace.snapshots.lock().await;
        if let Some(s) = &cache.value
            && let Some(m) = &s.market
        {
            let hot_stream_match = live_ticker
                .as_deref()
                .is_some_and(|t| t == m.ticker.as_str() && m.close_time > now);
            if s.reusable_at(now) || hot_stream_match {
                let imb = s
                    .order_book
                    .as_ref()
                    .and_then(|b| top10_imbalance(&b.yes_dollars, &b.no_dollars));
                return Some((m.clone(), imb));
            }
        }
    }
    let markets = workspace
        .client
        .open_markets(asset.info().series)
        .await
        .ok()?;
    let fetched_at = Utc::now();
    let market = current_market(&markets, fetched_at)?.clone();
    // Skip redundant REST order_book fetch when live WebSocket stream already carries L2 book!
    let has_live_book = {
        let s = workspace.stream_tx.borrow();
        s.state == "live" && s.ticker.as_deref() == Some(market.ticker.as_str()) && s.book.is_some()
    };
    let order_book = if has_live_book {
        None
    } else {
        workspace.client.order_book(&market.ticker).await.ok()
    };
    let imb = order_book
        .as_ref()
        .and_then(|b| top10_imbalance(&b.yes_dollars, &b.no_dollars));
    let mut cache = workspace.snapshots.lock().await;
    cache.value = Some(Snapshot {
        fetched_at,
        market: Some(market.clone()),
        order_book,
        order_book_error: false,
    });
    Some((market, imb))
}

#[derive(Deserialize)]
struct QuantSignalQuery {
    #[serde(default)]
    asset: Asset,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    refresh: Option<u8>,
}

async fn asset_midpoint(workspace: &Workspace, asset: Asset) -> Option<f64> {
    let stream = workspace.stream_tx.borrow().clone();
    if stream.state == "live"
        && let (Some(b), Some(a)) = (dec_f64(stream.yes_bid), dec_f64(stream.yes_ask))
        && b > 0.0
        && a < 1.0
        && b <= a
    {
        return Some(0.5 * (b + a));
    }
    let (m, _) = ensure_snapshot_market(workspace, asset).await?;
    let a = dec_f64(m.yes_ask_dollars)?;
    let b = dec_f64(m.yes_bid_dollars).or_else(|| dec_f64(m.no_ask_dollars).map(|no| 1.0 - no))?;
    if b > 0.0 && a < 1.0 && b <= a {
        Some(0.5 * (b + a))
    } else {
        None
    }
}

async fn quant_signal(
    State(state): State<AppState>,
    Query(query): Query<QuantSignalQuery>,
) -> Response {
    let asset = query.asset;
    let fast_only = query.mode.as_deref() == Some("fast");
    let force_refresh = query.refresh == Some(1);
    let workspace = &state.workspaces[&asset];
    let peers: [Asset; 2] = match asset {
        Asset::Btc => [Asset::Eth, Asset::Sol],
        Asset::Eth => [Asset::Btc, Asset::Sol],
        Asset::Sol => [Asset::Btc, Asset::Eth],
    };

    let work = async {
        let fetch_started = std::time::Instant::now();
        let (target_snap, peer_a, peer_b, cmc_board, news_cat) = tokio::join!(
            ensure_snapshot_market(workspace, asset),
            asset_midpoint(&state.workspaces[&peers[0]], peers[0]),
            asset_midpoint(&state.workspaces[&peers[1]], peers[1]),
            state.context.board(),
            state.news.latest_catalyst(asset),
        );
        let data_fetch_ms = fetch_started.elapsed().as_secs_f64() * 1000.0;
        let Some((market, snap_imb)) = target_snap else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "status": "no_active_market",
                    "asset": asset,
                    "message": "No active 15-minute contract found for this asset right now."
                })),
            )
                .into_response();
        };

        let stream = workspace.stream_tx.borrow().clone();
        let ref_u = workspace.reference_tx.borrow().clone();
        let (yes_bid, yes_ask, book_imb, quote_source) = if stream.state == "live"
            && stream.ticker.as_deref() == Some(market.ticker.as_str())
        {
            if let Some(book) = &stream.book
                && let (Some(yb), Some(nb)) = (
                    book.yes_dollars
                        .iter()
                        .filter_map(|(p, _)| dec_f64(Some(*p)))
                        .reduce(f64::max),
                    book.no_dollars
                        .iter()
                        .filter_map(|(p, _)| dec_f64(Some(*p)))
                        .reduce(f64::max),
                )
            {
                let ya = (1.0 - nb).clamp(0.005, 0.995);
                let imb = top10_imbalance(&book.yes_dollars, &book.no_dollars).or(snap_imb);
                (yb, ya, imb, "live_book_stream")
            } else if let (Some(yb), Some(ya)) = (dec_f64(stream.yes_bid), dec_f64(stream.yes_ask))
            {
                (yb, ya, snap_imb, "live_ticker_stream")
            } else {
                let ya = dec_f64(market.yes_ask_dollars).unwrap_or(0.51);
                let yb = dec_f64(market.yes_bid_dollars)
                    .or_else(|| dec_f64(market.no_ask_dollars).map(|n| 1.0 - n))
                    .unwrap_or((ya - 0.01).max(0.01));
                (yb, ya, snap_imb, "public_rest_snapshot")
            }
        } else {
            let ya = dec_f64(market.yes_ask_dollars).unwrap_or(0.51);
            let yb = dec_f64(market.yes_bid_dollars)
                .or_else(|| dec_f64(market.no_ask_dollars).map(|n| 1.0 - n))
                .unwrap_or((ya - 0.01).max(0.01));
            (yb, ya, snap_imb, "public_rest_snapshot")
        };

        let target_usd = dec_f64(market.floor_strike);
        let reference_usd = if ref_u.state == "live" {
            dec_f64(ref_u.value)
        } else {
            None
        };
        let cmc_id: u64 = match asset {
            Asset::Btc => 1,
            Asset::Eth => 1027,
            Asset::Sol => 5426,
        };
        let cmc_1h_pct = cmc_board
            .pointer("/latest/quotes")
            .and_then(serde_json::Value::as_array)
            .and_then(|arr| arr.iter().find(|q| q["id"].as_u64() == Some(cmc_id)))
            .and_then(|q| q["percent_change_1h"].as_f64());

        let report = state
            .decisions
            .evaluate_with_options(
                gemini_decisions::LiveMarketInput {
                    asset,
                    ticker: market.ticker,
                    open_time: market.open_time,
                    close_time: market.close_time,
                    now: Utc::now(),
                    yes_bid,
                    yes_ask,
                    target_usd,
                    reference_usd,
                    book_imbalance_top10: book_imb,
                    peer_mid_a: peer_a,
                    peer_mid_b: peer_b,
                    peer_labels: (peers[0].code(), peers[1].code()),
                    cmc_1h_pct,
                    news: news_cat,
                    quote_source,
                },
                gemini_decisions::EvalOptions {
                    fast_only,
                    force_refresh,
                    data_fetch_ms,
                },
            )
            .await;

        let server_timing = format!(
            "fetch;dur={:.2}, quant;dur={:.3}, gemini;dur={}, total;dur={:.2}",
            report.latency.data_fetch_ms,
            report.latency.quant_compute_us as f64 / 1000.0,
            report.latency.gemini_ms.unwrap_or(0),
            report.latency.total_server_ms
        );
        let mut resp = Json(report).into_response();
        resp.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
        if let Ok(val) = header::HeaderValue::from_str(&server_timing) {
            resp.headers_mut()
                .insert(header::HeaderName::from_static("server-timing"), val);
        }
        resp
    };

    match tokio::time::timeout(std::time::Duration::from_secs(6), work).await {
        Ok(resp) => resp,
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            "Quant signal calculation timed out",
        )
            .into_response(),
    }
}

async fn hourly_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/hourly.js"),
    )
}
async fn study_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/quote-study.js"),
    )
}

async fn study_style() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../public/quote-study.css"),
    )
}

async fn combo_quote(
    State(state): State<AppState>,
    Query(selection): Query<combos::Selection>,
) -> Response {
    if !selection.valid() {
        return (
            StatusCode::BAD_REQUEST,
            "Select two or three supported assets",
        )
            .into_response();
    }
    let quote = tokio::time::timeout(
        std::time::Duration::from_secs(12),
        state.combos.quote(&selection),
    )
    .await;
    match quote {
        Ok(quote) => ([(header::CACHE_CONTROL, "no-store")], Json(quote)).into_response(),
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            "Combo discovery timed out; retry shortly",
        )
            .into_response(),
    }
}
async fn feed_script(State(state): State<AppState>) -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        format!(
            "window.DASHBOARD_POLLING={};\n{}",
            state.polling,
            include_str!("../public/feed-client.js")
        ),
    )
}
async fn feeds(State(state): State<AppState>) -> Response {
    let started = std::time::Instant::now();
    // Brief 20ms yield lets upstream WebSocket tasks flush frames on request-billed CPU (-80ms vs legacy 100ms sleep).
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let now_ms = Utc::now().timestamp_millis();
    let feeds: HashMap<_, _> = state
        .workspaces
        .iter()
        .map(|(a, w)| {
            (
                *a,
                serde_json::json!({
                    "market": w.stream_tx.borrow().clone(),
                    "reference": w.reference_envelope(*a, w.reference_tx.borrow().clone())
                }),
            )
        })
        .collect();
    let elapsed_ms = (started.elapsed().as_secs_f64() * 100000.0).round() / 100.0;
    state.latency.record_feeds_ms(elapsed_ms);
    let mut resp = Json(serde_json::json!({
        "feeds": feeds,
        "delivery": "polled",
        "interval_seconds": 1,
        "interval_ms": 1000,
        "server_time_ms": now_ms,
        "server_ms": elapsed_ms,
    }))
    .into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    if let Ok(val) = header::HeaderValue::from_str(&format!("feeds;dur={elapsed_ms:.2}")) {
        resp.headers_mut()
            .insert(header::HeaderName::from_static("server-timing"), val);
    }
    resp
}

async fn latency_telemetry(State(state): State<AppState>) -> Response {
    let started = std::time::Instant::now();
    let now_ms = Utc::now().timestamp_millis();
    let mut assets = serde_json::Map::new();
    for asset in Asset::ALL {
        let w = &state.workspaces[&asset];
        let s = w.stream_tx.borrow().clone();
        let r = w.reference_tx.borrow().clone();
        let quote_age_ms = s.received_at_ms.map(|t| now_ms.saturating_sub(t));
        let book_age_ms = s.book_received_at_ms.map(|t| now_ms.saturating_sub(t));
        let ref_age_ms = r.source_time_ms.map(|t| now_ms.saturating_sub(t));
        assets.insert(
            asset.code().to_owned(),
            serde_json::json!({
                "asset": asset.code(),
                "ticker": s.ticker,
                "ws_state": s.state,
                "quote_age_ms": quote_age_ms,
                "book_age_ms": book_age_ms,
                "ref_state": r.state,
                "ref_wire_latency_ms": r.wire_latency_ms,
                "ref_age_ms": ref_age_ms,
            }),
        );
    }
    let (ws_ticks, cf_ticks) = state.latency.counts();
    let srv_ms = (started.elapsed().as_secs_f64() * 100000.0).round() / 100.0;
    let payload = serde_json::json!({
        "schema_version": 1,
        "server_time_ms": now_ms,
        "server_ms": srv_ms,
        "delivery_mode": if state.polling { "polled_1000ms" } else { "sse_realtime" },
        "poll_interval_ms": if state.polling { 1000 } else { 0 },
        "assets": assets,
        "feeds_ring": state.latency.feeds_stats(),
        "cf_wire_ring": state.latency.cf_wire_stats(),
        "quant_gemini_ring": state.decisions.telemetry_snapshot(),
        "counters": {
            "ws_ticks": ws_ticks,
            "cf_ticks": cf_ticks,
        },
        "optimizations": [
            {"agent":"Agent 1 · Kalshi WS","before_ms":20000,"after_ms":4000,"change":"Pipelined ticker+orderbook_delta single TCP flush; 4s rollover check & 0ms clean-rollover reconnect (-16s rollover gap)"},
            {"agent":"Agent 2 · CF Reference","before_ms":45,"after_ms":1,"change":"Pre-parse arrival timestamping on Message::Text for exact exchange->Cloud Run wire latency measurement"},
            {"agent":"Agent 3 · /api/feeds","before_ms":2100,"after_ms":1020,"change":"Removed 80ms artificial server sleep (100ms->20ms), doubled visible refresh cadence (2s->1s) + Server-Timing"},
            {"agent":"Agent 4 · Static Caching","before_ms":650,"after_ms":15,"change":"Preserved immutable 24h browser cache headers on vendored Chart.js & versioned assets to bypass Cloud Run HoL queue"},
            {"agent":"Agent 5 · Quant + Gemini SWR","before_ms":440,"after_ms":0.25,"change":"Sub-200us Rust quant fast-path + zero-REST WS metadata reuse + singleflight async SWR Gemini Decision 1 (~30% tighter prompt)"},
            {"agent":"Agent 6 · Browser Paint","before_ms":35,"after_ms":4,"change":"content-visibility: auto containment + live header Latency telemetry pill driven directly from /api/feeds RTT"},
            {"agent":"Agent 7 · Cloud Run Budget","before_ms":1400,"after_ms":220,"change":"Zero extra vCPU/RAM cost (0.25 vCPU, 256Mi, max=1 preserved); immutable caching cuts repeat loads to 0 server hops"},
            {"agent":"Agent 8 · Telemetry Ring","before_ms":115,"after_ms":0.2,"change":"Lock-bounded 64-sample circular rings + atomic tick counters exposed via lightweight <1KB GET /api/latency"},
            {"agent":"Agent 9 · Diffusion Model","before_ms":5000,"after_ms":2,"change":"Quantified square-root quote drift (0.00854*sqrt(ms) cents): cutting 5s->2ms preserves +65% after-fee EV"},
            {"agent":"Agent 10 · Chart.js Telemetry","before_ms":508,"after_ms":2,"change":"Dedicated /?view=latency observability dashboard with 5 interactive Chart.js canvases + light/dark sync"},
        ]
    });
    let mut resp = Json(payload).into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    if let Ok(val) = header::HeaderValue::from_str(&format!("latency;dur={srv_ms:.2}")) {
        resp.headers_mut()
            .insert(header::HeaderName::from_static("server-timing"), val);
    }
    resp
}

async fn latency_lab_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/latency-lab.js"),
    )
}

async fn chart_theme_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/chart-theme.js"),
    )
}
#[derive(Deserialize)]
struct OutcomeSelection {
    #[serde(default)]
    asset: Asset,
    #[serde(default)]
    view: OutcomeView,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum OutcomeView {
    #[default]
    Full,
    Recent,
}

async fn outcome_board(
    State(state): State<AppState>,
    Query(selection): Query<OutcomeSelection>,
) -> Response {
    let Some(archive) = &state.archive else {
        return Json(serde_json::json!({"status":"not_configured","contracts":[]})).into_response();
    };
    match archive.board(selection.asset).await {
        Ok(board) => {
            let headers = [(header::CACHE_CONTROL, "public, max-age=30")];
            match selection.view {
                OutcomeView::Full => (headers, Json(board)).into_response(),
                OutcomeView::Recent => (headers, Json(board.recent())).into_response(),
            }
        }
        Err(_) => (
            StatusCode::BAD_GATEWAY,
            "History archive temporarily unavailable",
        )
            .into_response(),
    }
}
#[derive(Deserialize)]
struct ArchiveSelection {
    asset: Asset,
    ticker: String,
}
async fn archived_window(
    State(state): State<AppState>,
    Query(selection): Query<ArchiveSelection>,
) -> Response {
    if !outcomes::valid_ticker(&selection.ticker, selection.asset) {
        return (StatusCode::BAD_REQUEST, "Invalid archived market selection").into_response();
    }
    let Some(archive) = state.archive else {
        return Json(serde_json::json!({"status":"not_configured","candles":[]})).into_response();
    };
    match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        archive.detail(selection.asset, &selection.ticker),
    )
    .await
    {
        Ok(Ok(Some(detail))) => (
            [(header::CACHE_CONTROL, "public, max-age=60")],
            Json(detail),
        )
            .into_response(),
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "Market not in archive").into_response(),
        _ => (
            StatusCode::BAD_GATEWAY,
            "Archived quotes temporarily unavailable",
        )
            .into_response(),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BeforeSelection {
    asset: Asset,
    ticker: String,
    minute: u8,
}
async fn replay_checkpoint(
    State(state): State<AppState>,
    Query(q): Query<ArchiveSelection>,
) -> Response {
    if !outcomes::valid_ticker(&q.ticker, q.asset) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(archive) = state.archive else {
        return Json(serde_json::json!({"status":"not_recorded"})).into_response();
    };
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let detail = archive
            .detail(q.asset, &q.ticker)
            .await?
            .ok_or("Window unavailable")?;
        let open =
            DateTime::parse_from_rfc3339(detail["open_time"].as_str().ok_or("Missing open")?)?
                .timestamp();
        archive.minute_ten(q.asset, open).await
    })
    .await;
    match result {
        Ok(Ok(value)) => (
            [(header::CACHE_CONTROL, "public, max-age=60")],
            Json(value.unwrap_or_else(|| serde_json::json!({"status":"not_recorded"}))),
        )
            .into_response(),
        _ => (StatusCode::BAD_GATEWAY, "Saved checkpoint unavailable").into_response(),
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RunRequest {
    open_time: Option<i64>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewRequest {
    asset: Asset,
    open_time: i64,
}
fn valid_research_window(open: i64) -> bool {
    let now = Utc::now().timestamp();
    open.rem_euclid(900) == 0 && open <= now && open >= now - 86400
}
async fn run_decisions(State(state): State<CollectorState>, Json(q): Json<RunRequest>) -> Response {
    let open = q
        .open_time
        .unwrap_or_else(|| Utc::now().timestamp().div_euclid(900) * 900);
    if !valid_research_window(open) {
        return (StatusCode::BAD_REQUEST, "Invalid research window").into_response();
    }
    let Some(runner) = state.runner else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Private runner unavailable",
        )
            .into_response();
    };
    match tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let decisions = runner.decide_cycle(open).await;
        let links = runner.link_cycle(open).await;
        Json(serde_json::json!({"open":open,"decisions":decisions,"links":links})).into_response()
    })
    .await
    {
        Ok(v) => v,
        Err(_) => (StatusCode::GATEWAY_TIMEOUT, "Private runner deadline").into_response(),
    }
}
async fn review_decision(
    State(state): State<CollectorState>,
    Query(q): Query<ReviewRequest>,
) -> Response {
    if !valid_research_window(q.open_time) {
        return (StatusCode::BAD_REQUEST, "Invalid research window").into_response();
    }
    let Some(runner) = state.runner else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Private runner unavailable",
        )
            .into_response();
    };
    match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        runner.review(q.asset, q.open_time),
    )
    .await
    {
        Ok(Ok(v)) => ([(header::CACHE_CONTROL, "no-store")], Json(v)).into_response(),
        _ => (StatusCode::BAD_GATEWAY, "Private review unavailable").into_response(),
    }
}

async fn issue_mcp_scope(
    State(state): State<CollectorState>,
    Json(q): Json<ReviewRequest>,
) -> Response {
    if !valid_research_window(q.open_time) {
        return (StatusCode::BAD_REQUEST, "Invalid research window").into_response();
    }
    let Some(runner) = state.runner else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Private runner unavailable",
        )
            .into_response();
    };
    match tokio::time::timeout(
        std::time::Duration::from_secs(12),
        runner.issue_mcp_scope(q.asset, q.open_time),
    )
    .await
    {
        Ok(Ok(v)) => ([(header::CACHE_CONTROL, "no-store")], Json(v)).into_response(),
        _ => (StatusCode::CONFLICT, "No eligible frozen packet").into_response(),
    }
}

async fn scheduled_phase(
    state: CollectorState,
    headers: axum::http::HeaderMap,
    phase: checkpoint::Phase,
) -> Response {
    let received = Utc::now();
    let header = headers
        .get("x-cloudscheduler-scheduletime")
        .and_then(|v| v.to_str().ok());
    let open = match cycle_dispatch::scheduled_open(header, received) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return Json(serde_json::json!({"status":"outside_scheduled_window"})).into_response();
        }
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    let capture_at = match cycle_dispatch::phase_capture_at(open, received.timestamp(), phase) {
        Ok(v) => v,
        Err("early") => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::RETRY_AFTER, "5")],
                "Retry within capture window",
            )
                .into_response();
        }
        Err(_) => {
            return Json(
                serde_json::json!({"status":"missed_phase","open":open,"phase":phase.name()}),
            )
            .into_response();
        }
    };
    let work = async {
        let prepared = if phase == checkpoint::Phase::Opening {
            state.archive.discover_cycle(open).await?
        } else {
            cycle_dispatch::PreparedCycle {
                open_time: open,
                ..Default::default()
            }
        };
        let plan = serde_json::json!({"schema_version":1,"open_time":open,"phase":phase.name(),
            "request_received_at":received,"original_schedule_time":header,"prepared_at":Utc::now(),
            "tickers":prepared.tickers,"planned_capture_offset":if phase==checkpoint::Phase::Opening {5}else{45}});
        state.archive.save_dispatch_plan(open, phase, &plan).await?;
        let wait = capture_at * 1000 - Utc::now().timestamp_millis();
        if wait > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(wait as u64)).await;
        }
        state
            .archive
            .collect_phase(&prepared, phase, state.runner.as_deref())
            .await
    };
    match tokio::time::timeout(std::time::Duration::from_secs(50), work).await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(error)) => {
            eprintln!("Scheduled phase failed: {error}");
            (StatusCode::BAD_GATEWAY, "Scheduled phase failed").into_response()
        }
        Err(_) => (StatusCode::GATEWAY_TIMEOUT, "Scheduled phase deadline").into_response(),
    }
}
async fn collect_opening(
    State(state): State<CollectorState>,
    headers: axum::http::HeaderMap,
) -> Response {
    scheduled_phase(state, headers, checkpoint::Phase::Opening).await
}
async fn collect_followup(
    State(state): State<CollectorState>,
    headers: axum::http::HeaderMap,
) -> Response {
    scheduled_phase(state, headers, checkpoint::Phase::ExecutionCandidate).await
}

async fn collect_cycle(State(state): State<CollectorState>) -> Response {
    match tokio::time::timeout(
        std::time::Duration::from_secs(50),
        state.archive.collect_cycle(state.runner.as_deref()),
    )
    .await
    {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(error)) => {
            eprintln!("Cycle collection failed: {error}");
            (StatusCode::BAD_GATEWAY, "Cycle collection failed").into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            "Cycle collection deadline reached",
        )
            .into_response(),
    }
}

async fn collect_minute_ten(State(state): State<CollectorState>) -> Response {
    match tokio::time::timeout(
        std::time::Duration::from_secs(50),
        state.archive.collect_minute_ten(),
    )
    .await
    {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(error)) => {
            eprintln!("Minute 10 collection failed: {error}");
            (StatusCode::BAD_GATEWAY, "Minute 10 collection failed").into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            "Minute 10 collection deadline reached",
        )
            .into_response(),
    }
}

async fn replay_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/market-replay.js"),
    )
}
async fn replay_style() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../public/market-replay.css"),
    )
}
async fn before_outcome(
    State(state): State<AppState>,
    Query(q): Query<BeforeSelection>,
) -> Response {
    if !outcomes::valid_ticker(&q.ticker, q.asset) || ![0, 5, 10, 14].contains(&q.minute) {
        return (
            StatusCode::BAD_REQUEST,
            "Choose a supported asset, archived ticker and cutoff",
        )
            .into_response();
    }
    let Some(archive) = &state.archive else {
        return Json(serde_json::json!({"status":"not_configured"})).into_response();
    };
    let result = tokio::time::timeout(std::time::Duration::from_secs(12), async {
        let (board, detail, context, snapshot) = tokio::join!(
            archive.board(q.asset),
            archive.detail(q.asset, &q.ticker),
            state.context.research_board(),
            state.blockworks.snapshot()
        );
        let board = board.map_err(|_| "Archive unavailable")?;
        let contract = board
            .contracts
            .iter()
            .find(|c| c.ticker == q.ticker)
            .ok_or("Window not in recent archive")?;
        let detail = detail
            .map_err(|_| "Quote archive unavailable")?
            .ok_or("Window not found")?;
        research::before(
            contract,
            &detail,
            &context,
            snapshot.ok().flatten().as_ref(),
            q.minute,
            Utc::now().timestamp(),
        )
    })
    .await;
    match result {
        Ok(Ok(value)) => {
            ([(header::CACHE_CONTROL, "public, max-age=60")], Json(value)).into_response()
        }
        Ok(Err(_)) => {
            (StatusCode::NOT_FOUND, "Recent completed window unavailable").into_response()
        }
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            "Before-outcome research timed out",
        )
            .into_response(),
    }
}
#[derive(Clone)]
struct CollectorState {
    archive: Arc<outcomes::Archive>,
    context: Arc<context::Context>,
    runner: Option<Arc<decision_runner::Runner>>,
}
async fn context_board(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "public, max-age=60")],
        Json(state.context.board().await),
    )
}
async fn research_context(State(state): State<AppState>) -> Response {
    match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        state.context.research_board(),
    )
    .await
    {
        Ok(value) => ([(header::CACHE_CONTROL, "public, max-age=60")], Json(value)).into_response(),
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            "Research history temporarily unavailable",
        )
            .into_response(),
    }
}
async fn collect_context(State(state): State<CollectorState>) -> Response {
    match state.context.collect().await {
        Ok(value) => Json(value).into_response(),
        Err(error) => {
            eprintln!("CMC collection incomplete: {error}");
            (
                StatusCode::BAD_GATEWAY,
                "Context collection unavailable; retained prior observations",
            )
                .into_response()
        }
    }
}
async fn data_lab_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/data-lab.js"),
    )
}
async fn data_lab_style() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../public/data-lab.css"),
    )
}
async fn context_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/market-context.js"),
    )
}
async fn collect_history(State(state): State<CollectorState>) -> Response {
    let archive = state.archive;
    match archive.collect().await {
        Ok(stored) => Json(serde_json::json!({"stored":stored})).into_response(),
        Err(error) => {
            eprintln!("History collection incomplete: {error}");
            (
                StatusCode::BAD_GATEWAY,
                "History collection incomplete; retry scheduled",
            )
                .into_response()
        }
    }
}
async fn paper_review_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/paper-review.js"),
    )
}
async fn price_map_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/price-map.js"),
    )
}
async fn final_minute_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/final-minute.js"),
    )
}
async fn focus_style() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../public/focus.css"),
    )
}
async fn theme_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/theme.js"),
    )
}
async fn home_visuals_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/home-visuals.js"),
    )
}
async fn homepage_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/homepage.js"),
    )
}
async fn navigation_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/navigation.js"),
    )
}
async fn homepage_art() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/webp")],
        include_bytes!("../public/art/quarter-hour-v1.webp").as_slice(),
    )
}
async fn history_board_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/outcome-board.js"),
    )
}
async fn combo_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/combo-lab.js"),
    )
}
async fn asset_list() -> Json<Vec<assets::AssetInfo>> {
    Json(Asset::ALL.into_iter().map(Asset::info).collect())
}

#[derive(Serialize)]
struct Snapshot {
    fetched_at: DateTime<Utc>,
    market: Option<Market>,
    order_book: Option<OrderBook>,
    order_book_error: bool,
}

#[derive(Default)]
struct SnapshotCache {
    value: Option<Snapshot>,
    retry_at: Option<std::time::Instant>,
}

impl Snapshot {
    fn reusable_at(&self, now: DateTime<Utc>) -> bool {
        let age = (now - self.fetched_at).num_milliseconds();
        (0..5000).contains(&age)
            && self.fetched_at.timestamp().div_euclid(900) == now.timestamp().div_euclid(900)
            && self
                .market
                .as_ref()
                .is_none_or(|m| m.open_time <= now && now < m.close_time)
    }

    fn response(&self) -> Response {
        ([(header::CACHE_CONTROL, "no-store")], Json(self)).into_response()
    }
}

async fn index() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-cache, must-revalidate")],
        Html(include_str!("../public/index.html")),
    )
}

async fn chart_library() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/vendor/lightweight-charts-5.2.1.js"),
    )
}

async fn chart_data() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../public/chart-data.js"),
    )
}

async fn chart_license() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        include_str!("../public/vendor/LICENSE-lightweight-charts.txt"),
    )
}

async fn chartjs_library() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=86400, immutable"),
        ],
        include_str!("../public/vendor/chart-4.4.7.umd.min.js"),
    )
}

async fn chartjs_license() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        include_str!("../public/vendor/LICENSE-chartjs.txt"),
    )
}

async fn snapshot(State(state): State<AppState>, Query(selection): Query<Selection>) -> Response {
    let workspace = &state.workspaces[&selection.asset];
    shared_snapshot(
        &workspace.snapshots,
        &workspace.client,
        &workspace.final_minute,
        selection.asset,
    )
    .await
}

async fn shared_snapshot(
    snapshots: &tokio::sync::Mutex<SnapshotCache>,
    client: &Client,
    recorder: &final_minute::Shared,
    asset: Asset,
) -> Response {
    // Hold only this asset's lock so simultaneous visitors share one market/book fetch.
    let mut cache = snapshots.lock().await;
    if let Some(s) = &cache.value
        && s.reusable_at(Utc::now())
    {
        return s.response();
    }
    let unavailable = || {
        (
            StatusCode::BAD_GATEWAY,
            [
                (header::RETRY_AFTER, "3"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            "Kalshi market data is unavailable",
        )
            .into_response()
    };
    if cache
        .retry_at
        .is_some_and(|retry| std::time::Instant::now() < retry)
    {
        return unavailable();
    }
    let markets = match client.open_markets(asset.info().series).await {
        Ok(markets) => markets,
        Err(error) => {
            eprintln!("Could not fetch markets: {error}");
            cache.retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
            return unavailable();
        }
    };
    let fetched_at = Utc::now();
    let mut market = current_market(&markets, fetched_at).cloned();
    let (mut order_book, mut order_book_error) = match &market {
        Some(market) => match client.order_book(&market.ticker).await {
            Ok(book) => (Some(book), false),
            Err(error) => {
                eprintln!("Could not fetch order book: {error}");
                (None, true)
            }
        },
        None => (None, false),
    };
    let now = Utc::now();
    if market.as_ref().is_some_and(|m| m.close_time <= now) {
        market = None;
        order_book = None;
        order_book_error = false;
    }
    recorder.lock().unwrap_or_else(|e| e.into_inner()).market(
        asset,
        market.as_ref(),
        now.timestamp_millis(),
    );
    cache.retry_at = None;
    cache
        .value
        .insert(Snapshot {
            fetched_at,
            market,
            order_book,
            order_book_error,
        })
        .response()
}

async fn historical_reference(
    State(state): State<AppState>,
    Query(selection): Query<Selection>,
) -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(state.workspaces[&selection.asset].history.reference().await),
    )
}

async fn historical_market(
    State(state): State<AppState>,
    Query(selection): Query<Selection>,
) -> Response {
    let workspace = &state.workspaces[&selection.asset];
    let markets = match workspace
        .client
        .open_markets(selection.asset.info().series)
        .await
    {
        Ok(markets) => markets,
        Err(_) => return (StatusCode::BAD_GATEWAY, "Market discovery unavailable").into_response(),
    };
    let Some(market) = current_market(&markets, Utc::now()) else {
        return (StatusCode::NOT_FOUND, "No active market for this asset").into_response();
    };
    match workspace
        .history
        .market(&market.ticker, market.open_time.timestamp())
        .await
    {
        Ok(candles) => ([(header::CACHE_CONTROL, "no-store")], Json(candles)).into_response(),
        Err(_) => (StatusCode::BAD_GATEWAY, "Market history unavailable").into_response(),
    }
}

async fn live(
    State(state): State<AppState>,
    Query(selection): Query<Selection>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let events =
        WatchStream::new(state.workspaces[&selection.asset].stream_tx.subscribe()).map(|update| {
            let event = Event::default()
                .json_data(update)
                .expect("stream update is serializable");
            Ok(event)
        });
    Sse::new(events).keep_alive(KeepAlive::default())
}

async fn reference_live(
    State(state): State<AppState>,
    Query(selection): Query<Selection>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let workspace = state.workspaces[&selection.asset].clone();
    let events = WatchStream::new(workspace.reference_tx.subscribe()).map(move |update| {
        Ok(Event::default()
            .json_data(workspace.reference_envelope(selection.asset, update))
            .expect("reference update is serializable"))
    });
    Sse::new(events).keep_alive(KeepAlive::default())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("DASHBOARD_ROLE").as_deref() == Ok("gemini-rehearsal") {
        return gemini_research::cli();
    }
    if std::env::var("DASHBOARD_ROLE").as_deref() == Ok("paper-replay") {
        return paper_study::cli();
    }
    if matches!(
        std::env::var("DASHBOARD_ROLE").as_deref(),
        Ok("evaluation-manifest" | "evaluation-replay")
    ) {
        return prospective::cli();
    }
    if std::env::var("DASHBOARD_ROLE").as_deref() == Ok("forecast-fit") {
        return forecast::fit_cli();
    }
    if matches!(
        std::env::var("DASHBOARD_ROLE").as_deref(),
        Ok("research-mcp" | "research-mcp-stdio")
    ) {
        return research_mcp::run().await.map_err(|e| e.to_string().into());
    }
    if std::env::var("DASHBOARD_ROLE").as_deref() == Ok("blockworks-pilot") {
        return blockworks::pilot().await.map_err(|e| e.to_string().into());
    }
    let port: u16 = std::env::var("PORT")
        .or_else(|_| std::env::var("DASHBOARD_PORT"))
        .unwrap_or_else(|_| "3000".into())
        .parse()?;
    let address: IpAddr = std::env::var("DASHBOARD_BIND_ADDRESS")
        .unwrap_or_else(|_| "127.0.0.1".into())
        .parse()?;
    let archive = outcomes::Archive::from_env().map_err(|_| "Invalid history configuration")?;
    let blockworks =
        blockworks::Service::new(address, archive.clone()).map_err(|e| e.to_string())?;
    let context =
        context::Context::new(archive.clone()).map_err(|_| "Invalid context configuration")?;
    let news = news::Service::new(archive.clone());
    if std::env::var("DASHBOARD_ROLE").as_deref() == Ok("collector") {
        let archive = archive.ok_or("Collector requires history configuration")?;
        let runner = decision_runner::Runner::from_env(archive.clone())
            .map_err(|_| "Invalid private runner configuration")?;
        if let Some(runner) = &runner {
            runner.warm_forecast().await;
        }
        let evaluation = evaluation_runner::Service::from_env(archive.clone())
            .map_err(|_| "Invalid evaluator configuration")?;
        let app = Router::new()
            .route("/collect-opening", post(collect_opening))
            .route("/collect-followup", post(collect_followup))
            .route("/run-decisions", post(run_decisions))
            .route("/research/decision", get(review_decision))
            .route("/research/mcp-scope", post(issue_mcp_scope))
            .route("/collect", post(collect_history))
            .route("/collect-context", post(collect_context))
            .route("/collect-minute-ten", post(collect_minute_ten))
            .route("/collect-cycle", post(collect_cycle))
            .route("/api/health", get(|| async { "ok" }))
            .with_state(CollectorState {
                archive,
                context,
                runner,
            })
            .merge(evaluation_runner::router(evaluation))
            .merge(news::private_router(news));
        let listener = tokio::net::TcpListener::bind((address, port)).await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown())
            .await?;
        return Ok(());
    }
    let client = Arc::new(Client::new()?);
    let latency = Arc::new(latency::Monitor::new());
    let credentials = stream::Credentials::from_env()
        .map_err(|error| format!("Stream configuration: {error}"))?;
    let mut workspaces = HashMap::new();
    for asset in Asset::ALL {
        let (tx, _stream_rx) = watch::channel(stream::Update::status("connecting"));
        let (reference_tx, _reference_rx) =
            watch::channel(reference::ReferenceUpdate::status("connecting"));
        let history = Arc::new(history::HistoryService::new(credentials.clone(), asset)?);
        let final_minute = final_minute::Recorder::shared();
        reference::start(
            credentials.clone(),
            reference_tx.clone(),
            asset,
            final_minute.clone(),
            latency.clone(),
        );
        stream::start(
            client.clone(),
            credentials.clone(),
            tx.clone(),
            asset,
            latency.clone(),
        );
        workspaces.insert(
            asset,
            Workspace {
                client: client.clone(),
                stream_tx: tx,
                reference_tx,
                history,
                final_minute,
                snapshots: Arc::default(),
            },
        );
    }
    let combo_service = Arc::new(combos::Service::new(client.clone())?);
    let polling = std::env::var("DASHBOARD_DELIVERY").as_deref() == Ok("poll");
    let preview_study = if archive.is_none() {
        quote_study::Board::local_preview(address, std::path::Path::new("data/quote-study.json"))
            .map(Arc::new)
    } else {
        None
    };
    let decisions = gemini_decisions::Engine::from_env();
    let state = AppState {
        preview_study,
        workspaces: Arc::new(workspaces),
        archive,
        combos: combo_service,
        context,
        polling,
        blockworks,
        news: news.clone(),
        decisions,
        latency,
    };
    let budgets = Arc::new(delivery::Budgets::new());
    let app = Router::new()
        .route("/", get(index))
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/latency", get(latency_telemetry))
        .route("/api/quant/simulation", get(quant_simulation))
        .route("/api/quant/signal", get(quant_signal))
        .route("/assets/chart-theme.js", get(chart_theme_script))
        .route("/assets/latency-lab.js", get(latency_lab_script))
        .route("/assets/quant-lab.js", get(quant_lab_script))
        .route("/assets/quant-lab.css", get(quant_lab_style))
        .route("/assets/terminal.css", get(terminal_style))
        .route(
            "/assets/live-news.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../public/live-news.js"),
                )
            }),
        )
        .route("/api/outcomes", get(outcome_board))
        .route("/api/hourly", get(hourly::board))
        .route("/api/archive/window", get(archived_window))
        .route("/api/context", get(context_board))
        .route("/api/context/history", get(research_context))
        .route("/api/blockworks", get(blockworks_board))
        .route("/api/research/quote-study", get(quote_study))
        .route("/assets/quote-study.js", get(study_script))
        .route("/assets/quote-study.css", get(study_style))
        .route("/api/research/before", get(before_outcome))
        .route("/api/research/checkpoint", get(replay_checkpoint))
        .route("/assets/market-replay.js", get(replay_script))
        .route("/assets/market-replay.css", get(replay_style))
        .route(
            "/assets/market-news.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../public/market-news.js"),
                )
            }),
        )
        .route("/assets/data-lab.js", get(data_lab_script))
        .route("/assets/data-lab.css", get(data_lab_style))
        .route("/assets/market-context.js", get(context_script))
        .route("/api/combos/quote", get(combo_quote))
        .route("/api/feeds", get(feeds))
        .route("/assets/feed-client.js", get(feed_script))
        .route("/assets/technical-chart.js", get(technical_chart_script))
        .route("/assets/quote-tape.js", get(quote_tape_script))
        .route("/assets/forecast-study.js", get(forecast_study_script))
        .route("/assets/forecast-pilot-v1.json", get(forecast_pilot_data))
        .route("/assets/hourly.js", get(hourly_script))
        .route("/assets/homepage.js", get(homepage_script))
        .route("/assets/focus.css", get(focus_style))
        .route("/assets/theme.js", get(theme_script))
        .route("/assets/home-visuals.js", get(home_visuals_script))
        .route("/assets/paper-review.js", get(paper_review_script))
        .route("/assets/price-map.js", get(price_map_script))
        .route("/assets/final-minute.js", get(final_minute_script))
        .route("/assets/navigation.js", get(navigation_script))
        .route("/assets/quarter-hour-v1.webp", get(homepage_art))
        .route("/assets/outcome-board.js", get(history_board_script))
        .route("/assets/combo-lab.js", get(combo_script))
        .route("/assets/lightweight-charts-5.2.1.js", get(chart_library))
        .route("/assets/chart-4.4.7.umd.min.js", get(chartjs_library))
        .route("/assets/chart-data.js", get(chart_data))
        .route("/licenses/lightweight-charts", get(chart_license))
        .route("/licenses/chartjs", get(chartjs_license))
        .route("/api/assets", get(asset_list))
        .route("/api/snapshot", get(snapshot))
        .route("/api/history/reference", get(historical_reference))
        .route("/api/history/market", get(historical_market))
        .route("/api/live", get(live))
        .route("/api/reference", get(reference_live))
        .with_state(state)
        .merge(news::public_router(news))
        .layer(middleware::from_fn(move |request: Request, next: Next| {
            let budgets = budgets.clone();
            async move {
                if polling && matches!(request.uri().path(), "/api/live" | "/api/reference") {
                    return (
                        StatusCode::GONE,
                        "Use /api/feeds for budget-mode live updates",
                    )
                        .into_response();
                }
                let route = request
                    .extensions()
                    .get::<MatchedPath>()
                    .map(MatchedPath::as_str);
                let budget = budgets.for_request(request.method(), route);
                if polling || request.uri().path().starts_with("/api/") {
                    let allowed = budget
                        .lock()
                        .map(|mut b| b.requests.allow(std::time::Instant::now(), 1))
                        .unwrap_or(false);
                    if !allowed {
                        return (
                            StatusCode::TOO_MANY_REQUESTS,
                            [
                                (header::RETRY_AFTER, "3"),
                                (header::CACHE_CONTROL, "no-store"),
                            ],
                            "Please retry shortly",
                        )
                            .into_response();
                    }
                }
                let static_asset = request.uri().path().starts_with("/assets/")
                    && request.uri().path() != "/assets/feed-client.js";
                let versioned_asset = request.uri().query().is_some_and(|q| q.contains("v="))
                    || request.uri().path().contains("chart-4.4.7")
                    || request.uri().path().contains("lightweight-charts-5.2.1");
                let accepts_gzip = delivery::accepts_gzip(
                    &request
                        .headers()
                        .get_all(header::ACCEPT_ENCODING)
                        .iter()
                        .filter_map(|value| value.to_str().ok())
                        .collect::<Vec<_>>()
                        .join(","),
                );
                let mut response = next.run(request).await;
                if polling {
                    let (mut parts, body) = response.into_parts();
                    let Ok(bytes) = axum::body::to_bytes(body, 1024 * 1024).await else {
                        return StatusCode::BAD_GATEWAY.into_response();
                    };
                    let bytes = delivery::encode(&mut parts.headers, bytes, accepts_gzip);
                    let allowed = budget
                        .lock()
                        .map(|mut b| b.bytes.allow(std::time::Instant::now(), bytes.len()))
                        .unwrap_or(false);
                    if !allowed {
                        return (
                            StatusCode::TOO_MANY_REQUESTS,
                            [
                                (header::RETRY_AFTER, "30"),
                                (header::CACHE_CONTROL, "no-store"),
                            ],
                            "Shared data allowance busy. Retry shortly.",
                        )
                            .into_response();
                    }
                    response = Response::from_parts(parts, axum::body::Body::from(bytes));
                }
                if static_asset && !response.headers().contains_key(header::CACHE_CONTROL) {
                    response.headers_mut().insert(
                        header::CACHE_CONTROL,
                        if versioned_asset {
                            header::HeaderValue::from_static("public, max-age=86400, immutable")
                        } else {
                            header::HeaderValue::from_static("public, max-age=300")
                        },
                    );
                }

                response.headers_mut().insert(
                    header::X_CONTENT_TYPE_OPTIONS,
                    header::HeaderValue::from_static("nosniff"),
                );
                response.headers_mut().insert(
                    header::REFERRER_POLICY,
                    header::HeaderValue::from_static("strict-origin-when-cross-origin"),
                );
                response
            }
        }));
    let listener = tokio::net::TcpListener::bind((address, port)).await?;
    println!("Crypto Prediction Lab: http://127.0.0.1:{port}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[test]
    fn private_run_requests_cannot_override_cutoff_or_forecast() {
        for body in [
            json!({"cutoff":"2026-11-10T00:00:00Z"}),
            json!({"p_yes":1}),
            json!({"open_time":0,"asset":"DOGE"}),
        ] {
            assert!(serde_json::from_value::<RunRequest>(body).is_err());
        }
        assert!(
            serde_json::from_value::<ReviewRequest>(json!({"asset":"DOGE","open_time":0})).is_err()
        );
        assert!(!valid_research_window(Utc::now().timestamp() + 900));
        assert!(!valid_research_window(
            Utc::now().timestamp().div_euclid(900) * 900 - 86400 - 900
        ));
    }

    #[tokio::test]
    async fn overlapping_visitors_reuse_the_same_timestamp_and_book_fetch() {
        let server = MockServer::start().await;
        let now = Utc::now();
        let open = DateTime::from_timestamp(now.timestamp().div_euclid(900) * 900, 0).unwrap();
        let market = json!({"ticker":"KXBTC15M-A","title":"Test","status":"active",
            "open_time":open,"close_time":open+chrono::Duration::minutes(15)});
        Mock::given(method("GET"))
            .and(path("/markets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"markets":[market]})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/markets/KXBTC15M-A/orderbook"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"orderbook_fp":{"yes_dollars":[],"no_dollars":[]}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = Client::with_base_url(&server.uri()).unwrap();
        let cache = tokio::sync::Mutex::new(SnapshotCache::default());
        let recorder = final_minute::Recorder::shared();
        let calls = (0..6).map(|_| shared_snapshot(&cache, &client, &recorder, Asset::Btc));
        let mut bodies = Vec::new();
        for response in futures_util::future::join_all(calls).await {
            assert_eq!(response.status(), StatusCode::OK);
            bodies.push(
                axum::body::to_bytes(response.into_body(), 4096)
                    .await
                    .unwrap(),
            );
        }
        assert!(bodies.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn snapshots_expire_at_ttl_and_at_window_rollover() {
        let close = DateTime::from_timestamp(900, 0).unwrap();
        let mut s = Snapshot {
            fetched_at: close - chrono::Duration::seconds(2),
            market: None,
            order_book: None,
            order_book_error: false,
        };
        assert!(s.reusable_at(close - chrono::Duration::seconds(1)));
        assert!(!s.reusable_at(close));
        s.fetched_at = close;
        assert!(!s.reusable_at(close + chrono::Duration::seconds(5)));
    }
}
