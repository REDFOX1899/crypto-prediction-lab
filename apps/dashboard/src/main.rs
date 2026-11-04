mod assets;
mod history;
mod order_book;
mod reference;
mod stream;

use assets::{Asset, Selection};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{
        Html, IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use chrono::{DateTime, Utc};
use kalshi_client::{Client, Market, OrderBook, current_market};
use serde::Serialize;
use std::{collections::HashMap, convert::Infallible, sync::Arc};
use tokio::sync::watch;
use tokio_stream::{StreamExt, wrappers::WatchStream};

#[derive(Clone)]
struct Workspace {
    client: Arc<Client>,
    stream_tx: watch::Sender<stream::Update>,
    reference_tx: watch::Sender<reference::ReferenceUpdate>,
    history: std::sync::Arc<history::HistoryService>,
}

#[derive(Clone)]
struct AppState {
    workspaces: Arc<HashMap<Asset, Workspace>>,
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
    let events = WatchStream::new(state.workspaces[&selection.asset].reference_tx.subscribe()).map(
        |update| {
            Ok(Event::default()
                .json_data(update)
                .expect("reference update is serializable"))
        },
    );
    Sse::new(events).keep_alive(KeepAlive::default())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Arc::new(Client::new()?);
    let credentials = stream::Credentials::from_env()
        .map_err(|error| format!("Stream configuration: {error}"))?;
    let mut workspaces = HashMap::new();
    for asset in Asset::ALL {
        let (tx, _stream_rx) = watch::channel(stream::Update::status("connecting"));
        let (reference_tx, _reference_rx) =
            watch::channel(reference::ReferenceUpdate::status("connecting"));
        let history = Arc::new(history::HistoryService::new(credentials.clone(), asset)?);
        reference::start(credentials.clone(), reference_tx.clone(), asset);
        stream::start(client.clone(), credentials.clone(), tx.clone(), asset);
        workspaces.insert(
            asset,
            Workspace {
                client: client.clone(),
                stream_tx: tx,
                reference_tx,
                history,
            },
        );
    }
    let state = AppState {
        workspaces: Arc::new(workspaces),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/assets/lightweight-charts-5.2.1.js", get(chart_library))
        .route("/assets/chart-data.js", get(chart_data))
        .route("/licenses/lightweight-charts", get(chart_license))
        .route("/api/assets", get(asset_list))
        .route("/api/snapshot", get(snapshot))
        .route("/api/history/reference", get(historical_reference))
        .route("/api/history/market", get(historical_market))
        .route("/api/live", get(live))
        .route("/api/reference", get(reference_live))
        .with_state(state);
    let port: u16 = std::env::var("DASHBOARD_PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    println!("Crypto Prediction Lab: http://127.0.0.1:{port}");
    axum::serve(listener, app).await?;
    Ok(())
}
