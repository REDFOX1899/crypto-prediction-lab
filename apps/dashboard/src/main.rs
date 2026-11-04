mod order_book;
mod reference;
mod stream;

use axum::{
    Json, Router,
    extract::State,
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
use std::{convert::Infallible, sync::Arc};
use tokio::sync::watch;
use tokio_stream::{StreamExt, wrappers::WatchStream};

#[derive(Clone)]
struct AppState {
    client: Arc<Client>,
    stream_tx: watch::Sender<stream::Update>,
    reference_tx: watch::Sender<reference::ReferenceUpdate>,
}

const SERIES: &str = "KXBTC15M";

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

async fn snapshot(State(state): State<AppState>) -> Response {
    let client = state.client;
    let markets = match client.open_markets(SERIES).await {
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

async fn live(
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let events = WatchStream::new(state.stream_tx.subscribe()).map(|update| {
        let event = Event::default()
            .json_data(update)
            .expect("stream update is serializable");
        Ok(event)
    });
    Sse::new(events).keep_alive(KeepAlive::default())
}

async fn reference_live(
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let events = WatchStream::new(state.reference_tx.subscribe()).map(|update| {
        Ok(Event::default()
            .json_data(update)
            .expect("reference update is serializable"))
    });
    Sse::new(events).keep_alive(KeepAlive::default())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Arc::new(Client::new()?);
    let credentials = stream::Credentials::from_env()
        .map_err(|error| format!("Stream configuration: {error}"))?;
    let (tx, _stream_rx) = watch::channel(stream::Update::status("connecting"));
    let (reference_tx, _reference_rx) =
        watch::channel(reference::ReferenceUpdate::status("connecting"));
    reference::start(credentials.clone(), reference_tx.clone());
    stream::start(client.clone(), credentials, tx.clone());
    let state = AppState {
        client,
        stream_tx: tx,
        reference_tx,
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/api/snapshot", get(snapshot))
        .route("/api/live", get(live))
        .route("/api/reference", get(reference_live))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("Crypto Prediction Lab: http://127.0.0.1:3000");
    axum::serve(listener, app).await?;
    Ok(())
}
