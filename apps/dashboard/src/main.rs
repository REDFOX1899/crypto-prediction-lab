use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
use kalshi_client::{Client, Market, OrderBook, current_market};
use serde::Serialize;
use std::sync::Arc;

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

async fn snapshot(State(client): State<Arc<Client>>) -> Response {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Arc::new(Client::new()?);
    let app = Router::new()
        .route("/", get(index))
        .route("/api/snapshot", get(snapshot))
        .with_state(client);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("Crypto Prediction Lab: http://127.0.0.1:3000");
    axum::serve(listener, app).await?;
    Ok(())
}
