//! A bounded scheduled observation with actual receipt timestamps.
use crate::assets::Asset;
use chrono::{DateTime, Utc};
use kalshi_client::{Client, current_market};
use serde_json::{Value, json};

pub fn slot(now: i64) -> Option<i64> {
    let open = now.div_euclid(900) * 900;
    (600..660).contains(&(now - open)).then_some(open)
}

pub async fn capture(client: &Client, asset: Asset, open: i64) -> Value {
    let started = Utc::now();
    let result = async {
        let markets = client.open_markets(asset.info().series).await.ok()?;
        let market = current_market(&markets, Utc::now())?;
        if market.open_time.timestamp() != open || market.close_time.timestamp()-open != 900 {
            return None;
        }
        let book = client.order_book(&market.ticker).await.ok()?;
        Some(json!({"ticker":market.ticker,"target":market.floor_strike,"target_text":market.yes_sub_title,
            "yes_bids":book.yes_dollars,"no_bids":book.no_dollars}))
    }.await;
    let received = Utc::now();
    let status = if result.is_none() {
        "unavailable"
    } else if received.timestamp() >= open + 660 {
        "late"
    } else {
        "available"
    };
    json!({"schema_version":1,"asset":asset,"open_time":DateTime::from_timestamp(open,0),
        "cutoff":DateTime::from_timestamp(open+600,0),"request_started_at":started,"received_at":received,
        "seconds_after_cutoff":(received.timestamp_millis()-(open+600)*1000) as f64/1000.,
        "status":status,"observation":result,
        "source":"Kalshi public market and order-book snapshots; sequential requests, non-atomic",
        "timing":"Captured during minute 10; actual receipt follows the cutoff. Never assume availability at the exact cutoff."})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_tenth_minute_is_eligible() {
        for seconds in [0, 599, 660, 899] {
            assert_eq!(slot(1800 + seconds), None);
        }
        for seconds in [600, 615, 659] {
            assert_eq!(slot(1800 + seconds), Some(1800));
        }
    }
}

/// Prospective research checkpoints. A follow-up is never claimed to be a decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Opening,
    ExecutionCandidate,
}
impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::ExecutionCandidate => "execution_candidate",
        }
    }
    pub fn offset(self) -> i64 {
        match self {
            Self::Opening => 0,
            Self::ExecutionCandidate => 45,
        }
    }
    pub fn deadline(self) -> i64 {
        match self {
            Self::Opening => 30,
            Self::ExecutionCandidate => 55,
        }
    }
    pub fn path(self, asset: Asset, open: i64) -> String {
        format!(
            "cycle_snapshots_v1/s{open}_{}_{}",
            asset
                .info()
                .series
                .trim_start_matches("KX")
                .trim_end_matches("15M"),
            self.name()
        )
    }
}

pub fn opening_slot(now: i64) -> Option<i64> {
    let open = now.div_euclid(900) * 900;
    (now - open < 55).then_some(open)
}

fn matches_window(market: &kalshi_client::Market, asset: Asset, open: i64) -> bool {
    market
        .ticker
        .starts_with(&format!("{}-", asset.info().series))
        && market.open_time.timestamp() == open
        && market.close_time.timestamp() == open + 900
        && matches!(market.status.as_str(), "open" | "active")
}

pub async fn capture_cycle(client: &Client, asset: Asset, open: i64, phase: Phase) -> Value {
    let started = Utc::now();
    let mut times = json!({"metadata_request_started_at":null,"metadata_received_at":null,
        "book_request_started_at":null,"book_received_at":null,"metadata_attempts":[]});
    let budget = if phase == Phase::Opening { 8 } else { 4 };
    let result = tokio::time::timeout(std::time::Duration::from_secs(budget), async {
        if started.timestamp() < open + phase.offset()
            || started.timestamp() >= open + phase.deadline()
        {
            return Err("outside_phase");
        }
        // Discovery can lag rollover. Retry within the phase budget, retaining
        // every attempt's timing. A stored first attempt is never replaced.
        let attempts = if phase == Phase::Opening { 5 } else { 2 };
        let mut selected = None;
        for attempt in 0..attempts {
            if Utc::now().timestamp() >= open + phase.deadline() {
                break;
            }
            times["metadata_request_started_at"] = json!(Utc::now());
            let response = client.open_markets(asset.info().series).await;
            times["metadata_received_at"] = json!(Utc::now());
            let succeeded = response.is_ok();
            let market_count = response.as_ref().map_or(0, Vec::len);
            if let Ok(markets) = response {
                selected = markets.into_iter().find(|m| matches_window(m, asset, open));
            }
            let attempt_times = json!({"request_started_at":times["metadata_request_started_at"],
                "received_at":times["metadata_received_at"],"request_succeeded":succeeded,
                "market_count":market_count,"matched_window":selected.is_some()});
            times["metadata_attempts"].as_array_mut().expect("attempt list").push(attempt_times);
            if selected.is_some() || attempt == attempts - 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        let market = selected.ok_or("market_unavailable")?;
        if Utc::now().timestamp() >= open + phase.deadline() {
            return Err("phase_deadline");
        }
        times["book_request_started_at"] = json!(Utc::now());
        let book = client.order_book(&market.ticker).await;
        times["book_received_at"] = json!(Utc::now());
        let book = book.map_err(|_| "book_unavailable")?;
        Ok(json!({"ticker":market.ticker,"market_status":market.status,
            "open_time":market.open_time,"close_time":market.close_time,
            "target":market.floor_strike,"target_text":market.yes_sub_title,
            "strike_type":market.strike_type,"yes_bids":book.yes_dollars,"no_bids":book.no_dollars}))
    }).await;
    let received = Utc::now();
    let (observation, error) = match result {
        Ok(Ok(value)) => (Some(value), None),
        Ok(Err(error)) => (None, Some(error)),
        Err(_) => (None, Some("source_timeout")),
    };
    let status = if observation.is_none() {
        "unavailable"
    } else if received.timestamp() >= open + phase.deadline() {
        "late"
    } else {
        "available"
    };
    json!({"schema_version":1,"asset":asset,"phase":phase.name(),
        "open_time":DateTime::from_timestamp(open,0),"close_time":DateTime::from_timestamp(open+900,0),
        "intended_at":DateTime::from_timestamp(open+phase.offset(),0),
        "request_started_at":started,"received_at":received,"component_times":times,
        "seconds_after_intended":(received.timestamp_millis()-(open+phase.offset())*1000) as f64/1000.,
        "status":status,"error":error,"observation":observation,
        "decision_id":null,"decision_committed_at":null,
        "source":"Kalshi public REST; metadata and order book are sequential, non-atomic; provider event time unknown",
        "availability":"No earlier than actual receipt and durable document creation. Execution candidate is unlinked to any decision; not evidence of a fill."})
}

#[cfg(test)]
mod cycle_tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[test]
    fn phase_identity_and_time_windows() {
        for seconds in [0, 20, 45, 54] {
            assert_eq!(opening_slot(1800 + seconds), Some(1800));
        }
        for seconds in [55, 600, 899] {
            assert_eq!(opening_slot(1800 + seconds), None);
        }
        for asset in Asset::ALL {
            let path = Phase::Opening.path(asset, 1800);
            assert!(path.starts_with("cycle_snapshots_v1/s1800_"));
            assert_ne!(path, Phase::ExecutionCandidate.path(asset, 1800));
        }
    }

    #[tokio::test]
    async fn captured_components_have_actual_ordered_times_and_no_decision() {
        let server = MockServer::start().await;
        let open = Utc::now().timestamp() - 5;
        let market = json!({"ticker":"KXBTC15M-TEST","title":"Test","status":"active",
            "open_time":DateTime::from_timestamp(open,0),"close_time":DateTime::from_timestamp(open+900,0),
            "floor_strike":"100","strike_type":"greater_or_equal"});
        Mock::given(method("GET"))
            .and(path("/markets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"markets":[]})))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/markets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"markets":[market]})))
            .with_priority(10)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/markets/KXBTC15M-TEST/orderbook"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"orderbook_fp":{
                "yes_dollars":[["0.40","12"]],"no_dollars":[["0.55","7"]]}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = Client::with_base_url(&server.uri()).unwrap();
        let value = capture_cycle(&client, Asset::Btc, open, Phase::Opening).await;
        assert_eq!(value["status"], "available");
        assert_eq!(
            value["component_times"]["metadata_attempts"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(value["observation"]["ticker"], "KXBTC15M-TEST");
        assert!(value["decision_id"].is_null());
        let times = &value["component_times"];
        let ordered = [
            value["request_started_at"].clone(),
            times["metadata_request_started_at"].clone(),
            times["metadata_received_at"].clone(),
            times["book_request_started_at"].clone(),
            times["book_received_at"].clone(),
            value["received_at"].clone(),
        ];
        let parsed: Vec<_> = ordered
            .iter()
            .map(|v| DateTime::parse_from_rfc3339(v.as_str().unwrap()).unwrap())
            .collect();
        assert!(parsed.windows(2).all(|w| w[0] <= w[1]));
    }

    #[tokio::test]
    async fn wrong_window_is_retried_and_never_fetches_a_book() {
        let server = MockServer::start().await;
        let open = Utc::now().timestamp() - 5;
        let market = json!({"ticker":"KXETH15M-ROLLOVER","title":"Test","status":"active",
            "open_time":DateTime::from_timestamp(open+900,0),"close_time":DateTime::from_timestamp(open+1800,0)});
        Mock::given(method("GET"))
            .and(path("/markets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"markets":[market]})))
            .expect(5)
            .mount(&server)
            .await;
        let client = Client::with_base_url(&server.uri()).unwrap();
        let value = capture_cycle(&client, Asset::Eth, open, Phase::Opening).await;
        assert_eq!(value["status"], "unavailable");
        assert_eq!(value["error"], "market_unavailable");
        assert!(value["observation"].is_null());
        assert!(value["component_times"]["book_request_started_at"].is_null());
    }

    #[tokio::test]
    async fn expired_opening_and_early_followup_make_no_provider_requests() {
        let server = MockServer::start().await;
        let client = Client::with_base_url(&server.uri()).unwrap();
        let open = Utc::now().timestamp() - 35;
        for phase in [Phase::Opening, Phase::ExecutionCandidate] {
            let value = capture_cycle(&client, Asset::Sol, open, phase).await;
            assert_eq!(value["error"], "outside_phase");
            assert!(value["observation"].is_null());
        }
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
