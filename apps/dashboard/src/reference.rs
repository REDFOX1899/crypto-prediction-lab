//! Independent read-only CF Benchmarks BRTI feed; failure does not interrupt the order book.
use crate::stream::Credentials;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::{sync::watch, time};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

const URL: &str = "wss://external-api-ws.kalshi.com/trade-api/ws/v2";
#[derive(Clone, Serialize)]
pub struct ReferenceUpdate {
    pub state: &'static str,
    pub value: Option<Decimal>,
    pub source_time_ms: Option<i64>,
}
impl ReferenceUpdate {
    pub fn status(state: &'static str) -> Self {
        Self {
            state,
            value: None,
            source_time_ms: None,
        }
    }
}
#[derive(Deserialize)]
struct Frame {
    index_id: String,
    data: String,
}
#[derive(Deserialize)]
struct Value {
    id: String,
    time: i64,
    value: Decimal,
}

fn parse_reference(value: serde_json::Value) -> Result<Value, ()> {
    let frame: Frame = serde_json::from_value(value).map_err(|_| ())?;
    let data: Value = serde_json::from_str(&frame.data).map_err(|_| ())?;
    if frame.index_id != "BRTI" || data.id != "BRTI" || data.value <= Decimal::ZERO {
        return Err(());
    }
    Ok(data)
}

pub fn start(credentials: Option<Credentials>, tx: watch::Sender<ReferenceUpdate>) {
    let Some(credentials) = credentials else {
        tx.send_replace(ReferenceUpdate::status("disabled"));
        return;
    };
    tokio::spawn(async move {
        loop {
            tx.send_replace(ReferenceUpdate::status("connecting"));
            if connect(&credentials, &tx, URL).await.is_err() {
                eprintln!("Bitcoin reference feed disconnected or unavailable; retrying.");
            }
            tx.send_replace(ReferenceUpdate::status("reconnecting"));
            time::sleep(Duration::from_secs(5)).await;
        }
    });
}
async fn connect(
    credentials: &Credentials,
    tx: &watch::Sender<ReferenceUpdate>,
    url: &str,
) -> Result<(), ()> {
    let ts = Utc::now().timestamp_millis().to_string();
    let mut request = url.into_client_request().map_err(|_| ())?;
    request.headers_mut().insert(
        "KALSHI-ACCESS-KEY",
        credentials.key_id.parse().map_err(|_| ())?,
    );
    request
        .headers_mut()
        .insert("KALSHI-ACCESS-TIMESTAMP", ts.parse().map_err(|_| ())?);
    request.headers_mut().insert(
        "KALSHI-ACCESS-SIGNATURE",
        credentials.signature(&ts).parse().map_err(|_| ())?,
    );
    let (mut socket, _) = time::timeout(Duration::from_secs(15), connect_async(request))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    let cmd = serde_json::json!({"id":1,"cmd":"subscribe","params":{"channels":["cfbenchmarks_value"],"index_ids":["BRTI"]}});
    socket
        .send(Message::Text(cmd.to_string().into()))
        .await
        .map_err(|_| ())?;
    let mut tick = time::interval(Duration::from_secs(5));
    let mut last_frame = time::Instant::now();
    let mut last_value: Option<time::Instant> = None;
    let mut latest_source_time = None;
    loop {
        tokio::select! {
            frame=socket.next()=>{
                let frame=frame.ok_or(())?.map_err(|_| ())?;
                last_frame=time::Instant::now();
                match frame {
                    Message::Text(text)=>{
                        let json:serde_json::Value=serde_json::from_str(&text).map_err(|_| ())?;
                        match json.get("type").and_then(|x|x.as_str()) {
                            Some("error")=>return Err(()),
                            Some("cfbenchmarks_value")=>{
                                let value=parse_reference(json.get("msg").cloned().ok_or(())?)?;
                                if latest_source_time.is_none_or(|last|value.time>last) {
                                    latest_source_time=Some(value.time);
                                    // A newly received tick with an old source timestamp is not live.
                                    if Utc::now().timestamp_millis().saturating_sub(value.time) <= 10_000 {
                                        last_value=Some(time::Instant::now());
                                        tx.send_replace(ReferenceUpdate {state:"live",value:Some(value.value),source_time_ms:Some(value.time)});
                                    }
                                }
                            }
                            _=>{}
                        }
                    }
                    Message::Ping(payload)=>socket.send(Message::Pong(payload)).await.map_err(|_| ())?,
                    Message::Close(_)=>return Err(()),
                    _=>{}
                }
            }
            _=tick.tick()=>{
                if last_frame.elapsed()>Duration::from_secs(35) {return Err(())}
                if last_value.is_some_and(|t|t.elapsed()>Duration::from_secs(10)) {tx.send_replace(ReferenceUpdate::status("stale")); last_value=None;}
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_nested_upstream_value_and_rejects_wrong_index() {
        let msg = serde_json::json!({"index_id":"BRTI","data":"{\"type\":\"value\",\"id\":\"BRTI\",\"time\":1710000000123,\"value\":\"68000.12\"}"});
        let v = parse_reference(msg.clone()).unwrap();
        assert_eq!(v.value.to_string(), "68000.12");
        assert_eq!(v.time, 1710000000123);
        let mut wrong = msg;
        wrong["index_id"] = "ETHUSD_RTI".into();
        assert!(parse_reference(wrong).is_err());
    }
    #[tokio::test]
    async fn subscribes_to_brti_and_forwards_price() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            let command = ws.next().await.unwrap().unwrap().into_text().unwrap();
            let command: serde_json::Value = serde_json::from_str(&command).unwrap();
            assert_eq!(command["params"]["channels"][0], "cfbenchmarks_value");
            assert_eq!(command["params"]["index_ids"][0], "BRTI");
            let now = Utc::now().timestamp_millis();
            let nested = serde_json::json!({"id":"BRTI","time":now,"value":"76000.12"}).to_string();
            let message=serde_json::json!({"type":"cfbenchmarks_value","msg":{"index_id":"BRTI","data":nested}}).to_string();
            ws.send(Message::Text(message.into())).await.unwrap();
            time::sleep(Duration::from_millis(100)).await;
        });
        let (tx, mut rx) = watch::channel(ReferenceUpdate::status("connecting"));
        let credentials = crate::stream::test_credentials();
        let url = format!("ws://{address}/trade-api/ws/v2");
        let task = tokio::spawn(async move { connect(&credentials, &tx, &url).await });
        time::timeout(Duration::from_secs(3), rx.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(rx.borrow().state, "live");
        assert_eq!(rx.borrow().value.unwrap().to_string(), "76000.12");
        server.await.unwrap();
        let _ = task.await;
    }
}
