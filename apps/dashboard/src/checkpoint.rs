//! A bounded scheduled observation with actual receipt timestamps.
use crate::assets::Asset;
use chrono::{DateTime, Utc};
use kalshi_client::{Client, current_market};
use rust_decimal::Decimal;
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

fn matches_window(market: &kalshi_client::Market, asset: Asset, open: i64) -> bool {
    market
        .ticker
        .starts_with(&format!("{}-", asset.info().series))
        && market.open_time.timestamp() == open
        && market.close_time.timestamp() == open + 900
        && matches!(market.status.as_str(), "open" | "active")
}

pub fn valid_hint(asset: Asset, ticker: &str) -> bool {
    ticker.starts_with(&format!("{}-", asset.info().series))
        && ticker.len() <= 96
        && ticker
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

pub async fn capture_cycle(
    client: &Client,
    asset: Asset,
    open: i64,
    phase: Phase,
    hint: Option<&str>,
) -> Value {
    let started = Utc::now();
    let mut times = json!({"metadata_request_started_at":null,"metadata_received_at":null,
        "book_request_started_at":null,"book_received_at":null,"metadata_attempts":[]});
    let budget_ms = match phase {
        Phase::Opening => ((open + 28) * 1000 - started.timestamp_millis()).clamp(0, 24_000),
        Phase::ExecutionCandidate => 4_000,
    };
    let capture = async {
        if started.timestamp() < open + phase.offset()
            || started.timestamp() >= open + phase.deadline()
        {
            return Err("outside_phase");
        }
        if hint.is_some_and(|s| !valid_hint(asset, s)) {
            return Err("invalid_ticker_hint");
        }
        let mut ticker = hint.map(str::to_owned);
        let attempts = if phase == Phase::Opening { 24 } else { 2 };
        let mut last_error = "market_unavailable";
        for attempt in 0..attempts {
            if Utc::now().timestamp() >= open + phase.deadline() {
                break;
            }
            times["metadata_request_started_at"] = json!(Utc::now());
            times["metadata_received_at"] = Value::Null;
            times["book_request_started_at"] = Value::Null;
            times["book_received_at"] = Value::Null;
            let response = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                if ticker.is_none() {
                    let markets = client.window_markets(asset.info().series, open).await?;
                    ticker = markets
                        .into_iter()
                        .find(|m| {
                            m.open_time.timestamp() == open
                                && m.close_time.timestamp() == open + 900
                                && valid_hint(asset, &m.ticker)
                        })
                        .map(|m| m.ticker);
                }
                match ticker.as_deref() {
                    Some(t) => client.market(t).await.map(Some),
                    None => Ok(None),
                }
            })
            .await;
            if response.is_ok() {
                times["metadata_received_at"] = json!(Utc::now());
            }
            let succeeded = matches!(&response, Ok(Ok(_)));
            let market = match response {
                Ok(Ok(m)) => m,
                _ => None,
            };
            let matched = market
                .as_ref()
                .is_some_and(|m| matches_window(m, asset, open));
            let ready = market.as_ref().is_some_and(|m| {
                matched
                    && m.floor_strike.is_some_and(|v| v > Decimal::ZERO)
                    && m.strike_type.as_deref() == Some("greater_or_equal")
            });
            let mut observation = None;
            if ready && Utc::now().timestamp() < open + phase.deadline() {
                let market = market.expect("ready metadata");
                times["book_request_started_at"] = json!(Utc::now());
                let book = tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    client.order_book(&market.ticker),
                )
                .await;
                if book.is_ok() {
                    times["book_received_at"] = json!(Utc::now());
                }
                match book {
                    Ok(Ok(book)) => {
                        let snapshot = crate::order_book::Snapshot {
                            market_ticker: market.ticker.clone(),
                            yes_dollars_fp: book.yes_dollars.clone(),
                            no_dollars_fp: book.no_dollars.clone(),
                        };
                        match crate::order_book::research_view(snapshot) {
                            Ok(_) => {
                                observation = Some(
                                    json!({"ticker":market.ticker,"market_status":market.status,
                                "open_time":market.open_time,"close_time":market.close_time,
                                "target":market.floor_strike,"target_text":market.yes_sub_title,"strike_type":market.strike_type,
                                "yes_bids":book.yes_dollars,"no_bids":book.no_dollars}),
                                )
                            }
                            Err(reason) => last_error = reason,
                        }
                    }
                    _ => last_error = "book_unavailable",
                }
            } else {
                last_error = if !succeeded {
                    "metadata_unavailable"
                } else if matched {
                    "metadata_not_ready"
                } else {
                    "market_unavailable"
                };
            }
            let attempt_times = json!({"request_started_at":times["metadata_request_started_at"],
                "received_at":times["metadata_received_at"],"completed_at":Utc::now(),
                "request_succeeded":succeeded,"matched_window":matched,"metadata_ready":ready,
                "ticker":ticker,"book_request_started_at":times["book_request_started_at"],
                "book_received_at":times["book_received_at"],"usable":observation.is_some(),
                "reason":if observation.is_some(){None}else{Some(last_error)}});
            times["metadata_attempts"]
                .as_array_mut()
                .expect("attempt list")
                .push(attempt_times);
            if let Some(value) = observation {
                return Ok(value);
            }
            if attempt + 1 < attempts {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
        Err(last_error)
    };
    let result =
        tokio::time::timeout(std::time::Duration::from_millis(budget_ms as u64), capture).await;
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
        "intended_at":DateTime::from_timestamp(open+phase.offset(),0),"request_started_at":started,"received_at":received,
        "component_times":times,"seconds_after_intended":(received.timestamp_millis()-(open+phase.offset())*1000) as f64/1000.,
        "status":status,"error":error,"observation":observation,"decision_id":null,"decision_committed_at":null,
        "source":"Kalshi public REST. Window-specific discovery and direct metadata. Sequential non-atomic book read. Provider event time unknown.",
        "availability":"No earlier than receipt and durable document creation. Candidate is unlinked until private decision verification."})
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
            assert!(
                crate::cycle_dispatch::PreparedCycle {
                    open_time: 1800,
                    ..Default::default()
                }
                .ready_at(1800 + seconds)
            );
        }
        for seconds in [55, 600, 899] {
            assert!(
                !crate::cycle_dispatch::PreparedCycle {
                    open_time: 1800,
                    ..Default::default()
                }
                .ready_at(1800 + seconds)
            );
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
            .and(path("/markets/KXBTC15M-TEST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"market":market})))
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
        let value = capture_cycle(&client, Asset::Btc, open, Phase::Opening, None).await;
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
        let open = Utc::now().timestamp() - 46;
        let market = json!({"ticker":"KXETH15M-ROLLOVER","title":"Test","status":"active",
            "open_time":DateTime::from_timestamp(open+900,0),"close_time":DateTime::from_timestamp(open+1800,0)});
        Mock::given(method("GET"))
            .and(path("/markets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"markets":[market]})))
            .expect(2)
            .mount(&server)
            .await;
        let client = Client::with_base_url(&server.uri()).unwrap();
        let value = capture_cycle(&client, Asset::Eth, open, Phase::ExecutionCandidate, None).await;
        assert_eq!(value["status"], "unavailable");
        assert_eq!(value["error"], "market_unavailable");
        assert!(value["observation"].is_null());
        assert!(value["component_times"]["book_request_started_at"].is_null());
    }

    #[tokio::test]
    async fn timed_out_retry_has_no_receipt_from_an_earlier_attempt() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/markets/KXBTC15M-TEST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"market":{"ticker":"KXBTC15M-TEST","title":"Test","status":"initialized","open_time":DateTime::from_timestamp(Utc::now().timestamp()-46,0),"close_time":DateTime::from_timestamp(Utc::now().timestamp()+854,0)}})))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/markets/KXBTC15M-TEST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(std::time::Duration::from_secs(10))
                    .set_body_json(json!({"market":{"ticker":"KXBTC15M-TEST","title":"Test","status":"initialized","open_time":DateTime::from_timestamp(Utc::now().timestamp()-46,0),"close_time":DateTime::from_timestamp(Utc::now().timestamp()+854,0)}})),
            )
            .with_priority(10)
            .expect(1)
            .mount(&server)
            .await;
        let client = Client::with_base_url(&server.uri()).unwrap();
        let value = capture_cycle(
            &client,
            Asset::Btc,
            Utc::now().timestamp() - 46,
            Phase::ExecutionCandidate,
            Some("KXBTC15M-TEST"),
        )
        .await;
        assert_eq!(value["error"], "metadata_unavailable");
        assert!(value["component_times"]["metadata_received_at"].is_null());
        assert_eq!(
            value["component_times"]["metadata_attempts"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn waits_for_target_and_two_sided_depth_without_using_open_list() {
        let server = MockServer::start().await;
        let open = Utc::now().timestamp() - 5;
        let market = json!({"ticker":"KXBTC15M-TEST","title":"Test","status":"active",
            "open_time":DateTime::from_timestamp(open,0),"close_time":DateTime::from_timestamp(open+900,0),
            "floor_strike":"100","strike_type":"greater_or_equal"});
        let mut unready = market.clone();
        unready["floor_strike"] = Value::Null;
        Mock::given(method("GET"))
            .and(path("/markets/KXBTC15M-TEST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"market":unready})))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/markets/KXBTC15M-TEST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"market":market})))
            .with_priority(10)
            .expect(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/markets/KXBTC15M-TEST/orderbook"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"orderbook_fp":{"yes_dollars":[],"no_dollars":[]}})),
            )
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/markets/KXBTC15M-TEST/orderbook"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"orderbook_fp":{"yes_dollars":[["0.4","12"]],"no_dollars":[["0.55","7"]]}}),
            ))
            .with_priority(10)
            .expect(1)
            .mount(&server)
            .await;
        let client = Client::with_base_url(&server.uri()).unwrap();
        let value = capture_cycle(
            &client,
            Asset::Btc,
            open,
            Phase::Opening,
            Some("KXBTC15M-TEST"),
        )
        .await;
        assert_eq!(value["status"], "available");
        let attempts = value["component_times"]["metadata_attempts"]
            .as_array()
            .unwrap();
        assert_eq!(attempts.len(), 3);
        assert_eq!(attempts[0]["metadata_ready"], false);
        assert_eq!(attempts[1]["reason"], "missing_or_invalid_depth");
        assert_eq!(attempts[2]["usable"], true);
        assert!(
            server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| r.url.path() != "/markets")
        );
    }

    #[tokio::test]
    async fn expired_opening_and_early_followup_make_no_provider_requests() {
        let server = MockServer::start().await;
        let client = Client::with_base_url(&server.uri()).unwrap();
        let open = Utc::now().timestamp() - 35;
        for phase in [Phase::Opening, Phase::ExecutionCandidate] {
            let value = capture_cycle(&client, Asset::Sol, open, phase, None).await;
            assert_eq!(value["error"], "outside_phase");
            assert!(value["observation"].is_null());
        }
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
