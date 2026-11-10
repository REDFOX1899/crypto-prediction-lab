mod assets;
mod blockworks;
mod combos;
mod context;
mod delivery;
mod final_minute;
mod history;
mod order_book;
mod outcomes;
mod quote_study;
mod reference;
mod research;
mod stream;

use assets::{Asset, Selection};
use axum::{
    Json, Router,
    extract::{Query, Request, State},
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
async fn outcome_board(
    State(state): State<AppState>,
    Query(selection): Query<Selection>,
) -> Response {
    let Some(archive) = &state.archive else {
        return Json(serde_json::json!({"status":"not_configured","contracts":[]})).into_response();
    };
    match archive.board(selection.asset).await {
        Ok(board) => ([(header::CACHE_CONTROL, "public, max-age=30")], Json(board)).into_response(),
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
    let client = &state.workspaces[&selection.asset].client;
    let markets = match client.open_markets(selection.asset.info().series).await {
        Ok(markets) => markets,
        Err(error) => {
            eprintln!("Could not fetch markets: {error}");
            return (StatusCode::BAD_GATEWAY, "Kalshi market data is unavailable").into_response();
        }
    };
    let market = current_market(&markets, Utc::now()).cloned();
    state.workspaces[&selection.asset]
        .final_minute
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .market(
            selection.asset,
            market.as_ref(),
            Utc::now().timestamp_millis(),
        );
    let (order_book, order_book_error) = match &market {
        Some(market) => match client.order_book(&market.ticker).await {
            Ok(book) => (Some(book), false),
            Err(error) => {
                eprintln!("Could not fetch order book: {error}");
                (None, true)
            }
        },
        None => (None, false),
    };
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(Snapshot {
            fetched_at: Utc::now(),
            market,
            order_book,
            order_book_error,
        }),
    )
        .into_response()
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
        let app = Router::new()
            .route("/collect", post(collect_history))
            .route("/collect-context", post(collect_context))
            .route("/api/health", get(|| async { "ok" }))
            .with_state(CollectorState { archive, context });
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
    let limiter = Arc::new(std::sync::Mutex::new(ApiBudget {
        tokens: 120.0,
        at: std::time::Instant::now(),
    }));
    let egress = Arc::new(std::sync::Mutex::new(EgressBudget {
        tokens: 1024.0 * 1024.0,
        at: std::time::Instant::now(),
    }));
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
            let limiter = limiter.clone();
            let egress = egress.clone();
            async move {
                if polling && matches!(request.uri().path(), "/api/live" | "/api/reference") {
                    return (
                        StatusCode::GONE,
                        "Use /api/feeds for budget-mode live updates",
                    )
                        .into_response();
                }
                if polling || request.uri().path().starts_with("/api/") {
                    let allowed = limiter
                        .lock()
                        .map(|mut budget| budget.allow(std::time::Instant::now()))
                        .unwrap_or(false);
                    if !allowed {
                        return (
                            StatusCode::TOO_MANY_REQUESTS,
                            [(header::RETRY_AFTER, "3")],
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
                    let allowed = egress
                        .lock()
                        .map(|mut b| b.allow(std::time::Instant::now(), bytes.len()))
                        .unwrap_or(false);
                    if !allowed {
                        return (
                            StatusCode::TOO_MANY_REQUESTS,
                            [(header::RETRY_AFTER, "30")],
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

// Bounds normal outbound bandwidth across visitors in the low-cost production mode.
// Infrastructure requests denied before reaching the app can still be billed by Google.
struct EgressBudget {
    tokens: f64,
    at: std::time::Instant,
}
impl EgressBudget {
    fn allow(&mut self, now: std::time::Instant, bytes: usize) -> bool {
        self.tokens = (self.tokens + now.saturating_duration_since(self.at).as_secs_f64() * 8192.0)
            .min(1024.0 * 1024.0);
        self.at = now;
        if self.tokens < bytes as f64 {
            return false;
        }
        self.tokens -= bytes as f64;
        true
    }
}
struct ApiBudget {
    tokens: f64,
    at: std::time::Instant,
}
impl ApiBudget {
    fn allow(&mut self, now: std::time::Instant) -> bool {
        self.tokens =
            (self.tokens + now.saturating_duration_since(self.at).as_secs_f64() * 2.0).min(120.0);
        self.at = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}
#[cfg(test)]
mod hosting_tests {
    use super::*;
    #[test]
    fn compressed_page_can_load_within_the_existing_shared_allowance() {
        let now = std::time::Instant::now();
        let raw = axum::body::Bytes::from_static(include_bytes!("../public/index.html"));
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("text/html"),
        );
        let compressed = delivery::encode(&mut headers, raw.clone(), true);
        let mut budget = EgressBudget {
            tokens: 64.0 * 1024.0,
            at: now,
        };
        assert!(!budget.allow(now, raw.len()));
        assert!(budget.allow(now, compressed.len()));
    }
    #[test]
    fn outbound_budget_limits_large_replies_and_refills() {
        let now = std::time::Instant::now();
        let mut b = EgressBudget {
            tokens: 1024.0 * 1024.0,
            at: now,
        };
        assert!(b.allow(now, 1024 * 1024));
        assert!(!b.allow(now, 1));
        assert!(b.allow(now + std::time::Duration::from_secs(1), 8192));
        assert!(!b.allow(now + std::time::Duration::from_secs(1), 1));
    }
    #[test]
    fn api_budget_replenishes_and_caps_bursts() {
        let now = std::time::Instant::now();
        let mut budget = ApiBudget {
            tokens: 120.0,
            at: now,
        };
        for _ in 0..120 {
            assert!(budget.allow(now));
        }
        assert!(!budget.allow(now));
        let later = now + std::time::Duration::from_secs(1);
        assert!(budget.allow(later));
        assert!(budget.allow(later));
        assert!(!budget.allow(later));
    }
}
