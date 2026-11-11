//! Prospective exchange rule evidence and pure cutoff projection.
//! Public endpoint payloads are allowlisted before storage and never reach agents raw.
use crate::{assets::Asset, checkpoint, decision};
use chrono::{DateTime, Utc};
use kalshi_client::{Client, Market};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Change {
    id: String,
    at: DateTime<Utc>,
    fee_type: Option<String>,
    multiplier: Option<Decimal>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Series {
    ticker: String,
    fee_type: String,
    multiplier: Decimal,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation<T> {
    request_started_at: DateTime<Utc>,
    received_at: DateTime<Utc>,
    data: T,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    schema_version: u8,
    status: String,
    ticker: String,
    event_ticker: String,
    market_type: String,
    notional_usd: Decimal,
    market_request_started_at: DateTime<Utc>,
    market_received_at: DateTime<Utc>,
    price_ranges: Vec<kalshi_client::PriceRange>,
    price_level_structure: String,
    fee_waiver_expiration_time: Option<DateTime<Utc>>,
    series: Observation<Series>,
    series_changes: Observation<Vec<Change>>,
    event_changes: Observation<Vec<Change>>,
}
fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn decimal(v: &Value) -> Result<Decimal, &'static str> {
    let text = if let Some(s) = v.as_str() {
        s.to_owned()
    } else if v.is_number() {
        v.to_string()
    } else {
        return Err("invalid_multiplier");
    };
    let n: Decimal = text.parse().map_err(|_| "invalid_multiplier")?;
    if n < Decimal::ZERO || n > Decimal::from(100) || n.scale() > 4 {
        return Err("invalid_multiplier");
    }
    Ok(n)
}
fn model(s: &str) -> bool {
    matches!(
        s,
        "quadratic" | "quadratic_with_maker_fees" | "quadratic_with_combo_maker_fees" | "flat"
    )
}
fn normalize_series(raw: Value, series: &str) -> Result<Series, &'static str> {
    let s = &raw["series"];
    if s["ticker"] != series {
        return Err("series_identity");
    }
    let fee = s["fee_type"]
        .as_str()
        .filter(|s| model(s))
        .ok_or("unknown_fee_type")?;
    Ok(Series {
        ticker: series.into(),
        fee_type: fee.into(),
        multiplier: decimal(&s["fee_multiplier"])?,
    })
}
fn normalize_changes(
    raw: Value,
    series: &str,
    event: Option<&str>,
) -> Result<Vec<Change>, &'static str> {
    let key = if event.is_some() {
        "event_fee_changes"
    } else {
        "series_fee_change_arr"
    };
    if event.is_some() && raw["cursor"] != "" {
        return Err("incomplete_event_history");
    }
    let rows = raw[key]
        .as_array()
        .filter(|a| a.len() <= 32)
        .ok_or("incomplete_fee_history")?;
    let mut output = Vec::new();
    for row in rows {
        if row["series_ticker"] != series || event.is_some_and(|s| row["event_ticker"] != s) {
            return Err("fee_change_identity");
        }
        let id = row["id"]
            .as_str()
            .filter(|s| safe_id(s))
            .ok_or("invalid_change_id")?;
        let at = decision::time(&row["scheduled_ts"])?;
        let (kind, mult) = if event.is_some() {
            ("fee_type_override", "fee_multiplier_override")
        } else {
            ("fee_type", "fee_multiplier")
        };
        let fee = if row.get(kind) == Some(&Value::Null) && event.is_some() {
            None
        } else {
            Some(
                row[kind]
                    .as_str()
                    .filter(|s| model(s))
                    .ok_or("unknown_fee_type")?
                    .to_owned(),
            )
        };
        let multiplier = if row.get(mult) == Some(&Value::Null) && event.is_some() {
            None
        } else {
            Some(decimal(&row[mult])?)
        };
        output.push(Change {
            id: id.into(),
            at,
            fee_type: fee,
            multiplier,
        });
    }
    output.sort_by(|a, b| a.at.cmp(&b.at).then(a.id.cmp(&b.id)));
    if output.windows(2).any(|w| w[0].at == w[1].at) {
        return Err("ambiguous_fee_changes");
    }
    Ok(output)
}
async fn observed<T>(
    future: impl std::future::Future<Output = Result<Value, kalshi_client::Error>>,
    normalize: impl FnOnce(Value) -> Result<T, &'static str>,
) -> Result<Observation<T>, &'static str> {
    let start = Utc::now();
    let raw = tokio::time::timeout(std::time::Duration::from_secs(2), future)
        .await
        .map_err(|_| "rules_timeout")?
        .map_err(|_| "rules_source_unavailable")?;
    let received = Utc::now();
    Ok(Observation {
        request_started_at: start,
        received_at: received,
        data: normalize(raw)?,
    })
}
pub async fn capture(
    client: &Client,
    asset: Asset,
    market: &Market,
    start: DateTime<Utc>,
    received: DateTime<Utc>,
) -> Value {
    let work = async {
        let series = asset.info().series;
        let event = market
            .event_ticker
            .as_deref()
            .filter(|s| safe_id(s) && s.starts_with(&format!("{series}-")))
            .ok_or("event_identity")?;
        let (a, b, c) = tokio::join!(
            observed(client.series_rules(series), |v| normalize_series(v, series)),
            observed(client.series_fee_changes(series), |v| normalize_changes(
                v, series, None
            )),
            observed(client.event_fee_changes(event), |v| normalize_changes(
                v,
                series,
                Some(event)
            )),
        );
        Ok::<_, &'static str>(Evidence {
            schema_version: 1,
            status: "available".into(),
            ticker: market.ticker.clone(),
            event_ticker: event.into(),
            market_type: market.market_type.clone().ok_or("missing_market_type")?,
            notional_usd: market.notional_value_dollars.ok_or("missing_notional")?,
            market_request_started_at: start,
            market_received_at: received,
            price_ranges: market.price_ranges.clone(),
            price_level_structure: market
                .price_level_structure
                .clone()
                .ok_or("missing_tick_structure")?,
            fee_waiver_expiration_time: market.fee_waiver_expiration_time,
            series: a?,
            series_changes: b?,
            event_changes: c?,
        })
    };
    match work.await {
        Ok(e) => {
            let value = serde_json::to_value(e).expect("rule evidence");
            if serde_json::to_vec(&value).expect("rule evidence").len() > 8192 {
                json!({"schema_version":1,"status":"unavailable","reason":"rules_evidence_size_limit"})
            } else {
                value
            }
        }
        Err(reason) => json!({"schema_version":1,"status":"unavailable","reason":reason}),
    }
}

pub(crate) fn ticks(ranges: &[kalshi_client::PriceRange]) -> Result<Vec<Decimal>, &'static str> {
    if ranges.is_empty() || ranges.len() > 8 {
        return Err("invalid_tick_ranges");
    }
    let mut prices = std::collections::BTreeSet::new();
    let mut end = Decimal::ZERO;
    for band in ranges {
        if band.start != end
            || band.end <= band.start
            || band.end > Decimal::ONE
            || band.step <= Decimal::ZERO
            || band.step.scale() > 4
            || band.start.scale() > 4
            || band.end.scale() > 4
            || (band.end - band.start) % band.step != Decimal::ZERO
        {
            return Err("invalid_tick_ranges");
        }
        let count = ((band.end - band.start) / band.step)
            .normalize()
            .to_string()
            .parse::<usize>()
            .map_err(|_| "invalid_tick_ranges")?;
        if count > 10000 {
            return Err("invalid_tick_ranges");
        }
        for n in 0..=count {
            let p = band.start + band.step * Decimal::from(n);
            if p > Decimal::ZERO && p < Decimal::ONE {
                prices.insert(p);
            }
        }
        end = band.end;
    }
    if end != Decimal::ONE || prices.is_empty() || prices.len() > 9999 {
        return Err("invalid_tick_ranges");
    }
    Ok(prices.into_iter().collect())
}
/// Validate a scope's normalized rule proof against its original opening packet.
/// Returns only currently effective configuration; future-effective rows are discarded.
fn resolve(e: &Evidence, packet: &Value) -> Result<Value, &'static str> {
    let cutoff = decision::time(&packet["cutoff"])?;
    let asset: Asset =
        serde_json::from_value(packet["asset"].clone()).map_err(|_| "rule_identity")?;
    let series = asset.info().series;
    let created = decision::time(&packet["source_created_at"])?;
    let overall = decision::time(&packet["received_at"])?;
    let original_start = decision::time(&packet["metadata_request_started_at"])?;
    let original_received = decision::time(&packet["metadata_received_at"])?;
    if e.schema_version != 1
        || e.status != "available"
        || e.ticker != packet["ticker"]
        || e.series.data.ticker != series
        || !safe_id(&e.event_ticker)
        || !e.event_ticker.starts_with(&format!("{series}-"))
        || e.market_type != "binary"
        || e.notional_usd != Decimal::ONE
        || e.market_request_started_at != original_start
        || e.market_received_at != original_received
        || e.price_level_structure.len() > 64
        || !safe_id(&e.price_level_structure)
    {
        return Err("rule_identity");
    }
    for (start, received) in [
        (e.series.request_started_at, e.series.received_at),
        (
            e.series_changes.request_started_at,
            e.series_changes.received_at,
        ),
        (
            e.event_changes.request_started_at,
            e.event_changes.received_at,
        ),
    ] {
        if start < e.market_received_at
            || start > received
            || received > created
            || received > overall
            || received > cutoff
        {
            return Err("rules_not_yet_available");
        }
    }
    for (changes, event) in [
        (&e.series_changes.data, false),
        (&e.event_changes.data, true),
    ] {
        if changes.len() > 32
            || changes.windows(2).any(|w| w[0].at >= w[1].at)
            || changes.iter().any(|c| {
                !safe_id(&c.id)
                    || c.fee_type.as_ref().is_some_and(|s| !model(s))
                    || c.multiplier.is_some_and(|m| decimal(&json!(m)).is_err())
                    || (!event && (c.fee_type.is_none() || c.multiplier.is_none()))
            })
        {
            return Err("invalid_fee_history");
        }
    }
    let price_ticks = ticks(&e.price_ranges)?;
    let mut fee = e.series.data.fee_type.clone();
    let mut multiplier = e.series.data.multiplier;
    if !model(&fee) || decimal(&json!(multiplier))? != multiplier {
        return Err("unknown_fee_type");
    }
    // A scheduled transition during the non-atomic series observation is ambiguous.
    if e.series_changes
        .data
        .iter()
        .any(|c| c.at >= e.series.request_started_at && c.at <= cutoff)
    {
        return Err("fee_transition_during_capture");
    }
    if let Some(c) = e
        .series_changes
        .data
        .iter()
        .filter(|c| c.at < e.series.request_started_at)
        .max_by_key(|c| c.at)
        && (c.fee_type.as_ref() != Some(&fee) || c.multiplier != Some(multiplier))
    {
        return Err("series_history_mismatch");
    }
    let chosen = e
        .event_changes
        .data
        .iter()
        .filter(|c| c.at <= cutoff)
        .max_by_key(|c| c.at);
    if let Some(c) = chosen {
        if let Some(f) = &c.fee_type {
            fee = f.clone();
        }
        if let Some(m) = c.multiplier {
            multiplier = m;
        }
    }
    if !model(&fee) || decimal(&json!(multiplier))? != multiplier {
        return Err("unknown_fee_type");
    }
    if e.fee_waiver_expiration_time.is_some_and(|t| t > cutoff) {
        return Err("unsupported_active_fee_waiver");
    }
    // Check all saved book prices against the actual dynamic grid on both sides.
    for side in ["yes_dollars", "no_dollars"] {
        let bids: Vec<(Decimal, Decimal)> = serde_json::from_value(packet["book"][side].clone())
            .map_err(|_| "invalid_rule_book")?;
        if bids.iter().any(|(p, _)| {
            price_ticks.binary_search(p).is_err()
                || price_ticks.binary_search(&(Decimal::ONE - *p)).is_err()
        }) {
            return Err("book_off_verified_grid");
        }
    }
    Ok(
        json!({"effective_fee_type":fee,"effective_fee_multiplier":multiplier,"fee_basis":if chosen.is_some(){"event history with per-field series fallback"}else{"series; complete event history has no effective override"},
        "effective_event_change":chosen,"price_ranges":e.price_ranges,"price_level_structure":e.price_level_structure,
        "notional_usd":e.notional_usd,"event_ticker":e.event_ticker,"supported_price_count":price_ticks.len(),
        "source_endpoints":{"market":format!("/markets/{}",e.ticker),"series":format!("/series/{series}"),"series_changes":"/series/fee_changes?show_historical=true","event_changes":"/events/fee_changes (complete event-filtered history)"},
        "fee_interpretation":"Exchange API configuration only; account discounts, FCM charges and member rounding profile are not verified"}),
    )
}

pub fn project(doc: &Value, packet: &Value) -> Result<Value, &'static str> {
    let asset: Asset =
        serde_json::from_value(packet["asset"].clone()).map_err(|_| "rule_identity")?;
    let open = packet["open_ts"].as_i64().ok_or("rule_identity")?;
    let cutoff = decision::time(&packet["cutoff"])?;
    if decision::packet(doc, asset, open, cutoff, checkpoint::Phase::Opening)? != *packet {
        return Err("rule_packet_mismatch");
    }
    let row: Value = serde_json::from_str(
        doc["fields"]["payload"]["stringValue"]
            .as_str()
            .ok_or("missing_payload")?,
    )
    .map_err(|_| "invalid_payload")?;
    let raw = &row["observation"]["market_rules"];
    if raw["status"] != "available" {
        return Err("rules_not_captured");
    }
    let mut e: Evidence =
        serde_json::from_value(raw.clone()).map_err(|_| "invalid_rule_evidence")?;
    if row["observation"]["event_ticker"] != e.event_ticker {
        return Err("event_identity");
    }
    let configuration = resolve(&e, packet)?;
    // Never expose future-effective schedule contents, even if announced earlier.
    e.series_changes.data.retain(|c| c.at <= cutoff);
    e.event_changes.data.retain(|c| c.at <= cutoff);
    Ok(
        json!({"schema_version":1,"status":"verified_asof_cutoff","packet_hash":decision::hash(packet),
        "source_id":packet["source_id"],"source_created_at":packet["source_created_at"],"cutoff":cutoff,
        "evidence":e,"configuration":configuration}),
    )
}
pub fn validate(projected: &Value, packet: &Value) -> Result<(), &'static str> {
    let cutoff = decision::time(&packet["cutoff"])?;
    let e: Evidence = serde_json::from_value(projected["evidence"].clone())
        .map_err(|_| "invalid_rule_evidence")?;
    if e.series_changes
        .data
        .iter()
        .chain(e.event_changes.data.iter())
        .any(|c| c.at > cutoff)
    {
        return Err("future_rule_change");
    }
    let expected = json!({"schema_version":1,"status":"verified_asof_cutoff","packet_hash":decision::hash(packet),
        "source_id":packet["source_id"],"source_created_at":packet["source_created_at"],"cutoff":packet["cutoff"],
        "evidence":e,"configuration":resolve(&e,packet)?});
    if expected != *projected {
        return Err("rule_projection_mismatch");
    }
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::decision::tests::{at, fixture, open};
    pub fn with_rules(asset: Asset) -> (Value, Value) {
        let mut doc = fixture(asset, checkpoint::Phase::Opening, 5, 6, 7);
        let evidence = json!({"schema_version":1,"status":"available","ticker":format!("{}-TEST",asset.info().series),
            "event_ticker":format!("{}-EVENT",asset.info().series),"market_type":"binary","notional_usd":"1",
            "market_request_started_at":at(5),"market_received_at":at(5),"price_level_structure":"tapered_deci_cent",
            "price_ranges":[{"start":"0.0000","end":"0.1000","step":"0.0010"},{"start":"0.1000","end":"0.9000","step":"0.0100"},{"start":"0.9000","end":"1.0000","step":"0.0010"}],
            "fee_waiver_expiration_time":null,
            "series":{"request_started_at":at(5),"received_at":at(6),"data":{"ticker":asset.info().series,"fee_type":"quadratic","multiplier":"1"}},
            "series_changes":{"request_started_at":at(5),"received_at":at(6),"data":[]},
            "event_changes":{"request_started_at":at(5),"received_at":at(6),"data":[]}});
        edit(&mut doc, |r| {
            r["observation"]["event_ticker"] = evidence["event_ticker"].clone();
            r["observation"]["market_rules"] = evidence;
        });
        let packet =
            decision::packet(&doc, asset, open(), at(8), checkpoint::Phase::Opening).unwrap();
        (doc, packet)
    }
    fn edit(doc: &mut Value, f: impl FnOnce(&mut Value)) {
        let mut r: Value =
            serde_json::from_str(doc["fields"]["payload"]["stringValue"].as_str().unwrap())
                .unwrap();
        f(&mut r);
        doc["fields"]["payload"]["stringValue"] = json!(r.to_string());
    }
    fn change(at: DateTime<Utc>, kind: Option<&str>, mult: Option<&str>) -> Value {
        json!({"id":"change-test","at":at,"fee_type":kind,"multiplier":mult})
    }
    #[test]
    fn projects_verified_dynamic_grid_and_provenance_for_all_assets() {
        for asset in Asset::ALL {
            let (doc, p) = with_rules(asset);
            let rules = project(&doc, &p).unwrap();
            validate(&rules, &p).unwrap();
            assert_eq!(rules["status"], "verified_asof_cutoff");
            assert_eq!(rules["configuration"]["effective_fee_multiplier"], "1");
            assert_eq!(rules["configuration"]["supported_price_count"], 279);
            assert_eq!(rules["source_created_at"], p["source_created_at"]);
            assert_eq!(rules["packet_hash"], decision::hash(&p));
            assert!(serde_json::to_vec(&rules).unwrap().len() < 8192);
        }
    }
    #[test]
    fn event_overrides_clear_and_partial_fallback_use_only_effective_history() {
        let (doc, p) = with_rules(Asset::Btc);
        for (kind, mult, expected_kind, expected_mult) in [
            (Some("quadratic"), Some("0.5"), "quadratic", "0.5"),
            (None, Some("2"), "quadratic", "2"),
            (
                Some("quadratic_with_maker_fees"),
                None,
                "quadratic_with_maker_fees",
                "1",
            ),
            (None, None, "quadratic", "1"),
        ] {
            let mut d = doc.clone();
            edit(&mut d, |r| {
                r["observation"]["market_rules"]["event_changes"]["data"] =
                    json!([change(at(0), kind, mult)])
            });
            let rules = project(&d, &p).unwrap();
            assert_eq!(rules["configuration"]["effective_fee_type"], expected_kind);
            assert_eq!(
                rules["configuration"]["effective_fee_multiplier"],
                expected_mult
            );
        }
    }
    #[test]
    fn future_effective_changes_and_labels_cannot_influence_hash_or_escape_scope() {
        for asset in Asset::ALL {
            let (mut doc, p) = with_rules(asset);
            let original = project(&doc, &p).unwrap();
            edit(&mut doc, |r| {
                r["result"] = json!("yes");
                r["observation"]["expiration_value"] = json!("hindsight");
                r["observation"]["market_rules"]["event_changes"]["data"] =
                    json!([change(at(45), Some("flat"), Some("100"))]);
                r["observation"]["market_rules"]["series_changes"]["data"] =
                    json!([change(at(60), Some("quadratic"), Some("0"))]);
            });
            let rules = project(&doc, &p).unwrap();
            assert_eq!(rules, original);
            assert_eq!(decision::hash(&rules), decision::hash(&original));
            let mut injected = original.clone();
            injected["evidence"]["event_changes"]["data"] =
                json!([change(at(45), Some("flat"), Some("100"))]);
            assert_eq!(validate(&injected, &p), Err("future_rule_change"));
            let mut injected = original;
            injected["result"] = json!("yes");
            assert!(validate(&injected, &p).is_err());
        }
    }
    #[test]
    fn late_modified_wrong_identity_or_transition_evidence_is_denied() {
        let (doc, p) = with_rules(Asset::Eth);
        for (field, value) in [
            ("received_at", json!(at(9))),
            ("request_started_at", json!(at(4))),
        ] {
            let mut d = doc.clone();
            edit(&mut d, |r| {
                r["observation"]["market_rules"]["series"][field] = value
            });
            assert_eq!(project(&d, &p), Err("rules_not_yet_available"));
        }
        let mut d = doc.clone();
        d["createTime"] = json!(at(8) + chrono::Duration::nanoseconds(1));
        d["updateTime"] = d["createTime"].clone();
        assert_eq!(project(&d, &p), Err("not_yet_available"));
        let mut d = doc.clone();
        d["updateTime"] = json!(at(8));
        assert_eq!(project(&d, &p), Err("modified_snapshot"));
        let mut d = doc.clone();
        edit(&mut d, |r| {
            r["observation"]["event_ticker"] = json!("KXBTC15M-WRONG")
        });
        assert_eq!(project(&d, &p), Err("event_identity"));
        let mut d = doc.clone();
        edit(&mut d, |r| {
            r["observation"]["market_rules"]["series_changes"]["data"] =
                json!([change(at(6), Some("quadratic"), Some("2"))])
        });
        assert_eq!(project(&d, &p), Err("fee_transition_during_capture"));
        let mut d = doc.clone();
        edit(&mut d, |r| {
            r["observation"]["market_rules"]["series_changes"]["data"] =
                json!([change(at(0), Some("quadratic"), Some("2"))])
        });
        assert_eq!(project(&d, &p), Err("series_history_mismatch"));
    }
    #[test]
    fn malformed_grids_history_and_waivers_are_not_invented() {
        let (doc, p) = with_rules(Asset::Sol);
        for ranges in [
            json!([]),
            json!([{"start":"0","end":"1","step":"0"}]),
            json!([{"start":"0.1","end":"1","step":"0.01"}]),
            json!([{"start":"0","end":"1","step":"0.00001"}]),
        ] {
            let mut d = doc.clone();
            edit(&mut d, |r| {
                r["observation"]["market_rules"]["price_ranges"] = ranges
            });
            assert_eq!(project(&d, &p), Err("invalid_tick_ranges"));
        }
        let mut d = doc.clone();
        edit(&mut d, |r| {
            r["observation"]["market_rules"]["fee_waiver_expiration_time"] = json!(at(900))
        });
        assert_eq!(project(&d, &p), Err("unsupported_active_fee_waiver"));
        assert_eq!(
            normalize_changes(
                json!({"event_fee_changes":[],"cursor":"next"}),
                "KXSOL15M",
                Some("KXSOL15M-EVENT")
            ),
            Err("incomplete_event_history")
        );
        let raw = json!({"series":{"ticker":"KXBTC15M","fee_type":"quadratic","fee_multiplier":1,"result":"yes","title":"ignore me"}});
        let series = normalize_series(raw, "KXBTC15M").unwrap();
        assert!(
            !serde_json::to_value(series)
                .unwrap()
                .to_string()
                .contains("result")
        );
    }
    #[test]
    fn legacy_sources_stay_unavailable_instead_of_backfilled() {
        let doc = fixture(Asset::Btc, checkpoint::Phase::Opening, 5, 6, 7);
        let p =
            decision::packet(&doc, Asset::Btc, open(), at(8), checkpoint::Phase::Opening).unwrap();
        assert_eq!(project(&doc, &p), Err("rules_not_captured"));
    }
    #[tokio::test]
    async fn actual_capture_uses_bounded_no_cache_sources_and_strips_outcomes() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{header, method, path, query_param},
        };
        let server = MockServer::start().await;
        let now = Utc::now();
        let market:Market=serde_json::from_value(json!({"ticker":"KXBTC15M-TEST","title":"Test","status":"active","open_time":now,"close_time":now+chrono::Duration::minutes(15),
            "event_ticker":"KXBTC15M-EVENT","market_type":"binary","notional_value_dollars":"1","price_level_structure":"linear_cent","price_ranges":[{"start":"0","end":"1","step":"0.01"}]})).unwrap();
        for (route, body) in [
            (
                "/series/KXBTC15M",
                json!({"series":{"ticker":"KXBTC15M","fee_type":"quadratic","fee_multiplier":1,"result":"hindsight"}}),
            ),
            ("/series/fee_changes", json!({"series_fee_change_arr":[]})),
            (
                "/events/fee_changes",
                json!({"event_fee_changes":[],"cursor":""}),
            ),
        ] {
            let mut mock = Mock::given(method("GET"))
                .and(path(route))
                .and(header("cache-control", "no-cache"));
            if route == "/events/fee_changes" {
                mock = mock
                    .and(query_param("event_ticker", "KXBTC15M-EVENT"))
                    .and(query_param("limit", "100"));
            }
            if route == "/series/fee_changes" {
                mock = mock.and(query_param("show_historical", "true"));
            }
            mock.respond_with(ResponseTemplate::new(200).set_body_json(body))
                .expect(1)
                .mount(&server)
                .await;
        }
        let client = Client::with_base_url(&server.uri()).unwrap();
        let captured = capture(&client, Asset::Btc, &market, now, now).await;
        assert_eq!(captured["status"], "available");
        assert!(!captured.to_string().contains("hindsight"));
        let proof: Evidence = serde_json::from_value(captured).unwrap();
        assert!(proof.series.received_at >= proof.series.request_started_at);
    }
}
