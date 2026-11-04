//! Read-only Kalshi ticker and order-book subscriptions shared by local dashboard tabs.
use crate::assets::Asset;
use crate::order_book::{Book, BookView, Delta, Snapshot};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey, pkcs8::DecodePrivateKey};
use futures_util::{SinkExt, StreamExt};
use kalshi_client::{Client, current_market};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::{sync::watch, time};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

const WS_URL: &str = "wss://external-api-ws.kalshi.com/trade-api/ws/v2";
const WS_PATH: &str = "/trade-api/ws/v2";

// Do not derive Debug: this structure contains secret material.
#[derive(Clone)]
pub struct Credentials {
    pub(crate) key_id: String,
    key: SigningKey,
}
impl Credentials {
    pub fn from_env() -> Result<Option<Self>, &'static str> {
        let id = std::env::var("KALSHI_API_KEY_ID").ok();
        let path = std::env::var("KALSHI_PRIVATE_KEY_PATH").ok();
        match (id, path) {
            (None, None) => Ok(None),
            (Some(key_id), Some(path)) if !key_id.is_empty() && !path.is_empty() => {
                let pem =
                    std::fs::read_to_string(path).map_err(|_| "Cannot read private key file")?;
                let key = SigningKey::from_pkcs8_pem(&pem)
                    .map_err(|_| "Private key must be an unencrypted Ed25519 PKCS#8 PEM")?;
                Ok(Some(Self { key_id, key }))
            }
            _ => Err("Set both KALSHI_API_KEY_ID and KALSHI_PRIVATE_KEY_PATH"),
        }
    }
    pub(crate) fn signature(&self, timestamp: &str) -> String {
        self.signature_for(timestamp, WS_PATH)
    }
    pub(crate) fn signature_for(&self, timestamp: &str, path: &str) -> String {
        STANDARD.encode(
            self.key
                .sign(format!("{timestamp}GET{path}").as_bytes())
                .to_bytes(),
        )
    }
}

#[cfg(test)]
pub(crate) fn test_credentials() -> Credentials {
    Credentials {
        key_id: "test-id".into(),
        key: SigningKey::from_bytes(&[7; 32]),
    }
}

#[derive(Clone, Serialize)]
pub struct Update {
    pub state: &'static str,
    pub ticker: Option<String>,
    pub yes_bid: Option<Decimal>,
    pub yes_ask: Option<Decimal>,
    pub received_at: Option<String>,
    pub book: Option<BookView>,
    pub book_received_at: Option<String>,
}
impl Update {
    pub fn status(state: &'static str) -> Self {
        Self {
            state,
            ticker: None,
            yes_bid: None,
            yes_ask: None,
            received_at: None,
            book: None,
            book_received_at: None,
        }
    }
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    kind: String,
    msg: Option<Ticker>,
}
#[derive(Deserialize)]
struct Ticker {
    market_ticker: Option<String>,
    yes_bid_dollars: Option<Decimal>,
    yes_ask_dollars: Option<Decimal>,
}

pub fn start(
    client: Arc<Client>,
    credentials: Option<Credentials>,
    tx: watch::Sender<Update>,
    asset: Asset,
) {
    let Some(credentials) = credentials else {
        tx.send_replace(Update::status("disabled"));
        return;
    };
    tokio::spawn(async move {
        loop {
            tx.send_replace(Update::status("connecting"));
            let market = match client.open_markets(asset.info().series).await {
                Ok(markets) => current_market(&markets, Utc::now()).cloned(),
                Err(_) => None,
            };
            if let Some(market) = market
                && connect_market(&client, &credentials, &market.ticker, &tx, WS_URL, asset)
                    .await
                    .is_err()
            {
                eprintln!("Kalshi stream disconnected or authentication failed; retrying.");
            }
            tx.send_replace(Update::status("reconnecting"));
            time::sleep(Duration::from_secs(5)).await;
        }
    });
}

async fn connect_market(
    client: &Client,
    credentials: &Credentials,
    ticker: &str,
    tx: &watch::Sender<Update>,
    url: &str,
    asset: Asset,
) -> Result<(), ()> {
    let timestamp = Utc::now().timestamp_millis().to_string();
    let mut request = url.into_client_request().map_err(|_| ())?;
    let headers = request.headers_mut();
    headers.insert(
        "KALSHI-ACCESS-KEY",
        credentials.key_id.parse().map_err(|_| ())?,
    );
    headers.insert(
        "KALSHI-ACCESS-TIMESTAMP",
        timestamp.parse().map_err(|_| ())?,
    );
    headers.insert(
        "KALSHI-ACCESS-SIGNATURE",
        credentials.signature(&timestamp).parse().map_err(|_| ())?,
    );
    let (mut socket, _) = time::timeout(Duration::from_secs(15), connect_async(request))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    let command = serde_json::json!({"id":1,"cmd":"subscribe","params":{"channels":["ticker"],"market_ticker":ticker}});
    socket
        .send(Message::Text(command.to_string().into()))
        .await
        .map_err(|_| ())?;
    let command = serde_json::json!({"id":2,"cmd":"subscribe","params":{"channels":["orderbook_delta"],"market_ticker":ticker}});
    socket
        .send(Message::Text(command.to_string().into()))
        .await
        .map_err(|_| ())?;
    let mut book = Book::default();
    let mut latest = Update::status("subscribed");
    latest.ticker = Some(ticker.to_owned());
    let mut check_market = time::interval(Duration::from_secs(15));
    check_market.tick().await;
    let mut last_message = time::Instant::now();
    loop {
        tokio::select! {
            frame = socket.next() => {
                let frame = frame.ok_or(())?.map_err(|_| ())?;
                last_message = time::Instant::now();
                match frame {
                    Message::Text(text) => {
                        // Inspect the envelope first: control messages use a different msg schema.
                        let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| ())?;
                        match value.get("type").and_then(|v| v.as_str()) {
                            Some("error") => return Err(()),
                            Some("subscribed") => {},
                            Some("ticker") => {
                                let envelope: Envelope = serde_json::from_value(value).map_err(|_| ())?;
                                if envelope.kind == "ticker" && let Some(m) = envelope.msg && m.market_ticker.as_deref() == Some(ticker) {
                                    latest.state="live"; latest.yes_bid=m.yes_bid_dollars; latest.yes_ask=m.yes_ask_dollars; latest.received_at=Some(Utc::now().to_rfc3339()); tx.send_replace(latest.clone());
                                }
                            }
                            Some("orderbook_snapshot") => {
                                let sid = value.get("sid").and_then(|v| v.as_u64()).ok_or(())?;
                                let seq = value.get("seq").and_then(|v| v.as_u64()).ok_or(())?;
                                let snapshot: Snapshot = serde_json::from_value(value.get("msg").cloned().ok_or(())?).map_err(|_| ())?;
                                if snapshot.market_ticker != ticker { return Err(()); }
                                book.snapshot(snapshot, sid, seq)?;
                                latest.book=Some(book.view()); latest.book_received_at=Some(Utc::now().to_rfc3339()); tx.send_replace(latest.clone());
                            }
                            Some("orderbook_delta") => {
                                let sid = value.get("sid").and_then(|v| v.as_u64()).ok_or(())?;
                                let seq = value.get("seq").and_then(|v| v.as_u64()).ok_or(())?;
                                let delta: Delta = serde_json::from_value(value.get("msg").cloned().ok_or(())?).map_err(|_| ())?;
                                if delta.market_ticker != ticker { return Err(()); }
                                book.delta(delta, sid, seq)?;
                                latest.book=Some(book.view()); latest.book_received_at=Some(Utc::now().to_rfc3339()); tx.send_replace(latest.clone());
                            }
                            _ => {}
                        }
                    }
                    Message::Ping(payload) => socket.send(Message::Pong(payload)).await.map_err(|_| ())?,
                    Message::Close(_) => return Err(()),
                    _ => {}
                }
            }
            _ = check_market.tick() => {
                if last_message.elapsed() > Duration::from_secs(35) { return Err(()); }
                let markets = client.open_markets(asset.info().series).await.map_err(|_| ())?;
                if current_market(&markets, Utc::now()).map(|m| m.ticker.as_str()) != Some(ticker) { return Ok(()); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signature, Verifier};
    #[test]
    fn signature_covers_timestamp_method_and_websocket_path() {
        let c = Credentials {
            key_id: "test".into(),
            key: SigningKey::from_bytes(&[7; 32]),
        };
        let signature = STANDARD.decode(c.signature("1234")).unwrap();
        let signature = Signature::from_slice(&signature).unwrap();
        assert!(
            c.key
                .verifying_key()
                .verify(b"1234GET/trade-api/ws/v2", &signature)
                .is_ok()
        );
        assert!(
            c.key
                .verifying_key()
                .verify(b"1235GET/trade-api/ws/v2", &signature)
                .is_err()
        );
    }
    #[test]
    fn ticker_decimals_and_unrelated_control_messages() {
        let m: Envelope = serde_json::from_str(r#"{"type":"ticker","msg":{"market_ticker":"BTC","yes_bid_dollars":"0.1250","yes_ask_dollars":"0.1300"}}"#).unwrap();
        assert_eq!(
            m.msg.unwrap().yes_bid_dollars.unwrap().to_string(),
            "0.1250"
        );
    }
    #[tokio::test]
    #[allow(clippy::result_large_err)] // tungstenite requires its concrete HTTP error type.
    async fn websocket_subscribes_and_forwards_matching_ticker() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_hdr_async(socket, |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                assert_eq!(request.headers().get("KALSHI-ACCESS-KEY").unwrap(), "test-id");
                assert!(request.headers().contains_key("KALSHI-ACCESS-SIGNATURE"));
                Ok(response)
            }).await.unwrap();
            let command = ws.next().await.unwrap().unwrap().into_text().unwrap();
            let command: serde_json::Value = serde_json::from_str(&command).unwrap();
            assert_eq!(command["params"]["market_ticker"], "BTC-TEST");
            assert_eq!(command["params"]["channels"][0], "ticker");
            ws.send(Message::Text(r#"{"type":"ticker","msg":{"market_ticker":"BTC-TEST","yes_bid_dollars":"0.4200","yes_ask_dollars":"0.4300"}}"#.into())).await.unwrap();
            time::sleep(Duration::from_millis(100)).await;
        });
        let (tx, mut rx) = watch::channel(Update::status("connecting"));
        let client = Client::new().unwrap();
        let credentials = Credentials {
            key_id: "test-id".into(),
            key: SigningKey::from_bytes(&[7; 32]),
        };
        let url = format!("ws://{address}/trade-api/ws/v2");
        let task = tokio::spawn(async move {
            connect_market(&client, &credentials, "BTC-TEST", &tx, &url, Asset::Btc).await
        });
        time::timeout(Duration::from_secs(3), rx.changed())
            .await
            .unwrap()
            .unwrap();
        let result = rx.borrow().clone();
        assert_eq!(result.state, "live");
        assert_eq!(result.ticker.as_deref(), Some("BTC-TEST"));
        assert_eq!(result.yes_ask.unwrap().to_string(), "0.4300");
        server.await.unwrap();
        let _ = task.await;
    }
    #[tokio::test]
    async fn streams_book_snapshot_delta_and_rejects_gap() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (advance_tx, advance_rx) = tokio::sync::oneshot::channel::<()>();
        let (gap_tx, gap_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            let _ticker_command = ws.next().await.unwrap().unwrap();
            let command = ws.next().await.unwrap().unwrap().into_text().unwrap();
            let command: serde_json::Value = serde_json::from_str(&command).unwrap();
            assert_eq!(command["params"]["channels"][0], "orderbook_delta");
            ws.send(Message::Text(r#"{"type":"orderbook_snapshot","sid":2,"seq":10,"msg":{"market_ticker":"BTC-TEST","yes_dollars_fp":[["0.4200","3.00"]],"no_dollars_fp":[]}}"#.into())).await.unwrap();
            advance_rx.await.unwrap();
            ws.send(Message::Text(r#"{"type":"orderbook_delta","sid":2,"seq":11,"msg":{"market_ticker":"BTC-TEST","price_dollars":"0.4200","delta_fp":"2.50","side":"yes"}}"#.into())).await.unwrap();
            gap_rx.await.unwrap();
            ws.send(Message::Text(r#"{"type":"orderbook_delta","sid":2,"seq":13,"msg":{"market_ticker":"BTC-TEST","price_dollars":"0.4200","delta_fp":"1.00","side":"yes"}}"#.into())).await.unwrap();
            time::sleep(Duration::from_millis(100)).await;
        });
        let (tx, mut rx) = watch::channel(Update::status("connecting"));
        let client = Client::new().unwrap();
        let credentials = Credentials {
            key_id: "test".into(),
            key: SigningKey::from_bytes(&[7; 32]),
        };
        let url = format!("ws://{address}/trade-api/ws/v2");
        let task = tokio::spawn(async move {
            connect_market(&client, &credentials, "BTC-TEST", &tx, &url, Asset::Btc).await
        });
        time::timeout(Duration::from_secs(3), rx.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            rx.borrow().book.as_ref().unwrap().yes_dollars[0]
                .1
                .to_string(),
            "3.00"
        );
        advance_tx.send(()).unwrap();
        time::timeout(Duration::from_secs(3), rx.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            rx.borrow().book.as_ref().unwrap().yes_dollars[0]
                .1
                .to_string(),
            "5.50"
        );
        gap_tx.send(()).unwrap();
        assert!(
            time::timeout(Duration::from_secs(3), task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        server.await.unwrap();
    }
}
