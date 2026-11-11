mod assets;
mod blockworks;
mod checkpoint;
mod combos;
mod context;
mod cycle_dispatch;
mod decision;
mod decision_runner;
mod delivery;
mod final_minute;
mod history;
mod order_book;
mod outcomes;
mod paper_levels;
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
async fn feeds(State(state): State<AppState>) -> impl IntoResponse {
    // A short request allows upstream WebSocket tasks to run on request-billed CPU.
    // No new upstream connections are opened for individual visitors.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let feeds: HashMap<_,_> = state.workspaces.iter().map(|(a,w)| (*a,serde_json::json!({"market":w.stream_tx.borrow().clone(),"reference":w.reference_envelope(*a,w.reference_tx.borrow().clone())}))).collect();
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({"feeds":feeds,"delivery":"polled","interval_seconds":2})),
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

async fn index() -> Html<&'static str> {
    Html(include_str!("../public/index.html"))
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
    if std::env::var("DASHBOARD_ROLE").as_deref() == Ok("collector") {
        let archive = archive.ok_or("Collector requires history configuration")?;
        let runner = decision_runner::Runner::from_env(archive.clone())
            .map_err(|_| "Invalid private runner configuration")?;
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
            });
        let listener = tokio::net::TcpListener::bind((address, port)).await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown())
            .await?;
        return Ok(());
    }
    let client = Arc::new(Client::new()?);
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
        );
        stream::start(client.clone(), credentials.clone(), tx.clone(), asset);
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
    let state = AppState {
        preview_study,
        workspaces: Arc::new(workspaces),
        archive,
        combos: combo_service,
        context,
        polling,
        blockworks,
    };
    let budgets = Arc::new(delivery::Budgets::new());
    let app = Router::new()
        .route("/", get(index))
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/outcomes", get(outcome_board))
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
        .route("/assets/data-lab.js", get(data_lab_script))
        .route("/assets/data-lab.css", get(data_lab_style))
        .route("/assets/market-context.js", get(context_script))
        .route("/api/combos/quote", get(combo_quote))
        .route("/api/feeds", get(feeds))
        .route("/assets/feed-client.js", get(feed_script))
        .route("/assets/homepage.js", get(homepage_script))
        .route("/assets/focus.css", get(focus_style))
        .route("/assets/theme.js", get(theme_script))
        .route("/assets/home-visuals.js", get(home_visuals_script))
        .route("/assets/final-minute.js", get(final_minute_script))
        .route("/assets/navigation.js", get(navigation_script))
        .route("/assets/quarter-hour-v1.webp", get(homepage_art))
        .route("/assets/outcome-board.js", get(history_board_script))
        .route("/assets/combo-lab.js", get(combo_script))
        .route("/assets/lightweight-charts-5.2.1.js", get(chart_library))
        .route("/assets/chart-data.js", get(chart_data))
        .route("/licenses/lightweight-charts", get(chart_license))
        .route("/api/assets", get(asset_list))
        .route("/api/snapshot", get(snapshot))
        .route("/api/history/reference", get(historical_reference))
        .route("/api/history/market", get(historical_market))
        .route("/api/live", get(live))
        .route("/api/reference", get(reference_live))
        .with_state(state)
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
                if static_asset {
                    response.headers_mut().insert(
                        header::CACHE_CONTROL,
                        header::HeaderValue::from_static("public, max-age=3600"),
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
