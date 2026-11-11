//! Pure, network-free baseline and point-in-time projection.
use crate::{
    assets::Asset,
    checkpoint::Phase,
    order_book::{Snapshot, research_view},
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const STRATEGY: &str = "market_midpoint_v1";
pub const ROUTING_MS: i64 = 1000;

pub fn time(value: &Value) -> Result<DateTime<Utc>, &'static str> {
    DateTime::parse_from_rfc3339(value.as_str().ok_or("missing_timestamp")?)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|_| "invalid_timestamp")
}
pub fn hash(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("JSON value"))
    )
}
pub fn run_id(asset: Asset, open: i64) -> String {
    format!("s{open}_{}_{STRATEGY}", asset.info().series)
}

/// Caller supplies only the fixed opening document. No archive/result access here.
pub fn packet(
    doc: &Value,
    asset: Asset,
    open: i64,
    cutoff: DateTime<Utc>,
    phase: Phase,
) -> Result<Value, &'static str> {
    let expected = format!(
        "projects/gitunderstand/databases/crypto-prediction-lab/documents/{}",
        phase.path(asset, open)
    );
    if doc["name"] != expected {
        return Err("snapshot_identity");
    }
    let created = time(&doc["createTime"])?;
    let updated = time(&doc["updateTime"])?;
    if created > cutoff || updated > cutoff {
        return Err("not_yet_available");
    }
    if created != updated {
        return Err("modified_snapshot");
    }
    let row: Value = serde_json::from_str(
        doc.pointer("/fields/payload/stringValue")
            .and_then(Value::as_str)
            .ok_or("missing_payload")?,
    )
    .map_err(|_| "invalid_payload")?;
    if row["schema_version"] != 1
        || row["asset"] != serde_json::to_value(asset).unwrap()
        || row["phase"] != phase.name()
        || time(&row["open_time"])?.timestamp() != open
        || time(&row["close_time"])?.timestamp() != open + 900
        || time(&row["intended_at"])?.timestamp() != open + phase.offset()
    {
        return Err("snapshot_identity");
    }
    if row["status"] != "available" {
        return Err("snapshot_unavailable");
    }
    let ts = &row["component_times"];
    let ordered = [
        time(&row["request_started_at"])?,
        time(&ts["metadata_request_started_at"])?,
        time(&ts["metadata_received_at"])?,
        time(&ts["book_request_started_at"])?,
        time(&ts["book_received_at"])?,
        time(&row["received_at"])?,
        time(&doc["createTime"])?,
    ];
    if ordered.windows(2).any(|w| w[0] > w[1])
        || ordered[0].timestamp() < open + phase.offset()
        || ordered[5].timestamp() >= open + phase.deadline()
    {
        return Err("invalid_timing");
    }
    if ordered[6] > cutoff {
        return Err("not_yet_available");
    }
    if (cutoff - ordered[5]).num_milliseconds() > 30_000 {
        return Err("stale_snapshot");
    }
    let obs = &row["observation"];
    let ticker = obs["ticker"].as_str().ok_or("missing_ticker")?;
    if !ticker.starts_with(&format!("{}-", asset.info().series))
        || ticker.len() > 96
        || !ticker
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        || time(&obs["open_time"])?.timestamp() != open
        || time(&obs["close_time"])?.timestamp() != open + 900
        || !matches!(obs["market_status"].as_str(), Some("open" | "active"))
    {
        return Err("market_identity");
    }
    if obs["strike_type"] != "greater_or_equal" {
        return Err("unsupported_rule");
    }
    let target: Decimal =
        serde_json::from_value(obs["target"].clone()).map_err(|_| "invalid_target")?;
    if target <= Decimal::ZERO {
        return Err("invalid_target");
    }
    let snapshot: Snapshot = serde_json::from_value(json!({"market_ticker":ticker,
        "yes_dollars_fp":obs["yes_bids"],"no_dollars_fp":obs["no_bids"]}))
    .map_err(|_| "invalid_book")?;
    let view = research_view(snapshot)?;
    let bid = view.yes_dollars[0].0;
    let ask = Decimal::ONE - view.no_dollars[0].0;
    Ok(
        json!({"schema_version":1,"asset":asset,"open_ts":open,"ticker":ticker,"cutoff":cutoff,
        "source_id":expected,"source_created_at":ordered[6],"received_at":ordered[5],
        "request_started_at":ordered[0],"metadata_request_started_at":ordered[1],
        "metadata_received_at":ordered[2],"book_request_started_at":ordered[3],"book_received_at":ordered[4],
        "target_usd":target,"comparator":"greater_or_equal","yes_bid_usd":bid,"yes_ask_usd":ask,
        "yes_midpoint_usd":(bid+ask)/Decimal::TWO,"spread_usd":ask-bid,"book":view,
        "provider_source_age":"unknown; REST publication time unavailable"}),
    )
}

/// Validate an untrusted serialized opening projection with exact allowlist equality.
/// No clock, network, label, or later-observation access.
pub fn validate_projection(p: &Value) -> Result<(), &'static str> {
    let deny = "invalid_projection";
    let asset: Asset = serde_json::from_value(p["asset"].clone()).map_err(|_| deny)?;
    let open = p["open_ts"].as_i64().ok_or(deny)?;
    let cutoff = time(&p["cutoff"]).map_err(|_| deny)?;
    if open.rem_euclid(900) != 0 || cutoff.timestamp() < open || cutoff.timestamp() >= open + 40 {
        return Err(deny);
    }
    let row = json!({"schema_version":1,"asset":asset,"phase":"opening","open_time":DateTime::from_timestamp(open,0),
            "close_time":DateTime::from_timestamp(open+900,0),"intended_at":DateTime::from_timestamp(open,0),
            "status":"available","request_started_at":p["request_started_at"],"received_at":p["received_at"],
            "component_times":{"metadata_request_started_at":p["metadata_request_started_at"],"metadata_received_at":p["metadata_received_at"],
            "book_request_started_at":p["book_request_started_at"],"book_received_at":p["book_received_at"]},
            "observation":{"ticker":p["ticker"],"open_time":DateTime::from_timestamp(open,0),"close_time":DateTime::from_timestamp(open+900,0),
            "market_status":"active","target":p["target_usd"],"strike_type":p["comparator"],
            "yes_bids":p["book"]["yes_dollars"],"no_bids":p["book"]["no_dollars"]}});
    let doc = json!({"name":p["source_id"],"createTime":p["source_created_at"],"updateTime":p["source_created_at"],
            "fields":{"payload":{"stringValue":row.to_string()}}});
    let projected = packet(&doc, asset, open, cutoff, Phase::Opening).map_err(|_| deny)?;
    if projected != *p {
        return Err(deny);
    }
    Ok(())
}

pub fn decide(doc: Option<&Value>, asset: Asset, open: i64, cutoff: DateTime<Utc>) -> Value {
    let projected = if cutoff.timestamp() >= open + 40 || cutoff.timestamp() < open {
        Err("decision_deadline")
    } else {
        doc.ok_or("missing_opening")
            .and_then(|d| packet(d, asset, open, cutoff, Phase::Opening))
    };
    let mut result = json!({"schema_version":1,"run_id":run_id(asset,open),"strategy":STRATEGY,
        "asset":asset,"open_ts":open,"cutoff":cutoff,"produced_at":Utc::now(),
        "status":"abstain","reason":null,"packet":null,"packet_hash":null,"p_yes":null,
        "order_policy":"none; fee, calibration and buy-level policies are separate work",
        "interpretation":"Observed market midpoint benchmark; no validated predictive edge"});
    match projected {
        Ok(packet) => {
            result["status"] = json!("forecast");
            result["p_yes"] = packet["yes_midpoint_usd"].clone();
            result["packet_hash"] = json!(hash(&packet));
            result["packet"] = packet;
        }
        Err(reason) => result["reason"] = json!(reason),
    }
    result
}

/// The durable object creation time is the commitment, never a client claim.
pub fn link(
    decision: &Value,
    committed: DateTime<Utc>,
    candidate: Option<&Value>,
    asset: Asset,
    open: i64,
    now: DateTime<Utc>,
) -> Value {
    let mut result = json!({"schema_version":1,"run_id":run_id(asset,open),"strategy":STRATEGY,
        "asset":asset,"open_ts":open,"decision_committed_at":committed,"routing_allowance_ms":ROUTING_MS,
        "packet_hash":decision["packet_hash"],"candidate_id":Phase::ExecutionCandidate.path(asset,open),
        "checked_at":now,"status":"ineligible","reason":null,"candidate":null,
        "interpretation":"Timing-eligible evidence only; no order or simulated fill"});
    let validate = || -> Result<Value, &'static str> {
        if decision["run_id"] != run_id(asset, open)
            || decision["asset"] != serde_json::to_value(asset).unwrap()
            || decision["strategy"] != STRATEGY
            || decision["open_ts"] != open
        {
            return Err("decision_identity");
        }
        if decision["status"] != "forecast" {
            return Err("decision_abstained");
        }
        let p = &decision["packet"];
        if decision["packet_hash"] != hash(p)
            || p["asset"] != decision["asset"]
            || p["open_ts"] != open
            || decision["p_yes"] != p["yes_midpoint_usd"]
        {
            return Err("decision_integrity");
        }
        let cutoff = time(&decision["cutoff"])?;
        if cutoff != time(&p["cutoff"])?
            || cutoff > committed
            || cutoff.timestamp() < open
            || committed.timestamp() >= open + 40
            || time(&decision["produced_at"])? > committed
        {
            return Err("late_or_invalid_commitment");
        }
        let doc = candidate.ok_or("missing_candidate")?;
        let available = time(&doc["createTime"])?;
        if available > now {
            return Err("not_yet_available");
        }
        let c = packet(doc, asset, open, available, Phase::ExecutionCandidate)?;
        if p["ticker"] != c["ticker"] {
            return Err("ticker_mismatch");
        }
        if time(&c["request_started_at"])? < committed + chrono::Duration::milliseconds(ROUTING_MS)
            || time(&c["book_request_started_at"])?
                < committed + chrono::Duration::milliseconds(ROUTING_MS)
        {
            return Err("candidate_precedes_decision");
        }
        Ok(c)
    };
    match validate() {
        Ok(c) => {
            result["status"] = json!("eligible_for_paper_testing");
            result["candidate"] = c;
        }
        Err(reason) => result["reason"] = json!(reason),
    }
    result
}

#[cfg(test)]
pub mod tests {
    use super::*;
    pub fn open() -> i64 {
        DateTime::parse_from_rfc3339("2026-11-10T20:00:00Z")
            .unwrap()
            .timestamp()
    }
    pub fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(open() + seconds, 0).unwrap()
    }
    pub fn fixture(asset: Asset, phase: Phase, start: i64, received: i64, created: i64) -> Value {
        let row = json!({"schema_version":1,"asset":asset,"phase":phase.name(),
            "open_time":at(0),"close_time":at(900),"intended_at":at(phase.offset()),"status":"available",
            "request_started_at":at(start),"received_at":at(received),"component_times":{
                "metadata_request_started_at":at(start),"metadata_received_at":at(start),
                "book_request_started_at":at(start),"book_received_at":at(received)},
            "observation":{"ticker":format!("{}-TEST",asset.info().series),"market_status":"active",
                "open_time":at(0),"close_time":at(900),"target":"100","strike_type":"greater_or_equal",
                "yes_bids":[["0.40","12"],["0.30","3"]],"no_bids":[["0.55","7"]]}});
        json!({"name":format!("projects/gitunderstand/databases/crypto-prediction-lab/documents/{}",phase.path(asset,open())),
            "createTime":at(created),"updateTime":at(created),"fields":{"payload":{"stringValue":row.to_string()}}})
    }
    fn change(doc: &Value, edit: impl FnOnce(&mut Value)) -> Value {
        let mut d = doc.clone();
        let mut row: Value =
            serde_json::from_str(d["fields"]["payload"]["stringValue"].as_str().unwrap()).unwrap();
        edit(&mut row);
        d["fields"]["payload"]["stringValue"] = json!(row.to_string());
        d
    }
    fn forecast(asset: Asset) -> Value {
        let d = fixture(asset, Phase::Opening, 20, 21, 22);
        let mut v = decide(Some(&d), asset, open(), at(23));
        v["produced_at"] = json!(at(24));
        v
    }
    #[test]
    fn only_first_durable_available_version_can_enter_model() {
        let d = fixture(Asset::Btc, Phase::Opening, 20, 21, 22);
        for cutoff in [0, 20, 21] {
            assert_eq!(
                decide(Some(&d), Asset::Btc, open(), at(cutoff))["reason"],
                "not_yet_available"
            );
        }
        let v = decide(Some(&d), Asset::Btc, open(), at(22));
        assert_eq!(
            v["p_yes"].as_str().unwrap().parse::<Decimal>().unwrap(),
            Decimal::new(425, 3)
        );
        let mut modified = d.clone();
        modified["updateTime"] = json!(at(24));
        assert_eq!(
            decide(Some(&modified), Asset::Btc, open(), at(23))["reason"],
            "not_yet_available"
        );
        assert_eq!(
            decide(Some(&modified), Asset::Btc, open(), at(25))["reason"],
            "modified_snapshot"
        );
    }
    #[test]
    fn outcomes_and_future_fields_cannot_change_packet_or_forecast() {
        for asset in Asset::ALL {
            let d = fixture(asset, Phase::Opening, 20, 21, 22);
            let a = decide(Some(&d), asset, open(), at(23));
            let changed = change(&d, |r| {
                r["result"] = json!("yes");
                r["observation"]["result"] = json!("no");
                r["observation"]["expiration_value"] = json!("9999");
                r["future_candidate"] = json!({"price":"0.99"});
            });
            let b = decide(Some(&changed), asset, open(), at(23));
            for key in ["packet", "packet_hash", "p_yes"] {
                assert_eq!(a[key], b[key]);
            }
            assert!(!a["packet"].to_string().contains("result"));
        }
    }
    #[test]
    fn identity_depth_rule_and_age_fail_closed() {
        let d = fixture(Asset::Btc, Phase::Opening, 1, 2, 3);
        for (key, value, reason) in [
            ("yes_bids", json!([]), "missing_or_invalid_depth"),
            ("yes_bids", json!([["0.9", "1"]]), "crossed_book"),
            ("no_bids", json!([["0.5", "0"]]), "invalid_book"),
            ("strike_type", json!("less"), "unsupported_rule"),
            ("ticker", json!("KXETH15M-TEST"), "market_identity"),
        ] {
            let bad = change(&d, |r| r["observation"][key] = value);
            assert_eq!(
                decide(Some(&bad), Asset::Btc, open(), at(10))["reason"],
                reason
            );
        }
        assert_eq!(
            decide(Some(&d), Asset::Btc, open(), at(39))["reason"],
            "stale_snapshot"
        );
        assert_eq!(
            decide(Some(&d), Asset::Btc, open(), at(40))["reason"],
            "decision_deadline"
        );
        assert_eq!(
            decide(None, Asset::Sol, open(), at(10))["reason"],
            "missing_opening"
        );
        assert_eq!(
            decide(Some(&d), Asset::Eth, open(), at(10))["reason"],
            "snapshot_identity"
        );
        assert_eq!(
            decide(Some(&d), Asset::Btc, open() + 900, at(910))["reason"],
            "snapshot_identity"
        );
    }
    #[test]
    fn later_candidate_links_to_actual_commitment_and_survives_later_review() {
        for asset in Asset::ALL {
            let v = forecast(asset);
            let c = fixture(asset, Phase::ExecutionCandidate, 45, 46, 47);
            let result = link(&v, at(25), Some(&c), asset, open(), at(120));
            assert_eq!(result["status"], "eligible_for_paper_testing");
            assert_eq!(result["decision_committed_at"], json!(at(25)));
            assert_eq!(result["packet_hash"], v["packet_hash"]);
            assert_eq!(
                link(&v, at(25), Some(&c), asset, open(), at(46))["reason"],
                "not_yet_available"
            );
        }
    }
    #[test]
    fn late_tampered_unavailable_or_wrong_ticker_cannot_be_linked() {
        let v = forecast(Asset::Btc);
        let c = fixture(Asset::Btc, Phase::ExecutionCandidate, 45, 46, 47);
        assert_eq!(
            link(&v, at(40), Some(&c), Asset::Btc, open(), at(50))["reason"],
            "late_or_invalid_commitment"
        );
        let old = fixture(Asset::Btc, Phase::ExecutionCandidate, 20, 46, 47);
        assert_eq!(
            link(&v, at(25), Some(&old), Asset::Btc, open(), at(50))["reason"],
            "invalid_timing"
        );
        let wrong = change(&c, |r| r["observation"]["ticker"] = json!("KXBTC15M-OTHER"));
        assert_eq!(
            link(&v, at(25), Some(&wrong), Asset::Btc, open(), at(50))["reason"],
            "ticker_mismatch"
        );
        let mut tampered = v.clone();
        tampered["packet"]["target_usd"] = json!("200");
        assert_eq!(
            link(&tampered, at(25), Some(&c), Asset::Btc, open(), at(50))["reason"],
            "decision_integrity"
        );
        assert_eq!(
            link(&v, at(25), None, Asset::Btc, open(), at(50))["reason"],
            "missing_candidate"
        );
        let abstain = decide(None, Asset::Btc, open(), at(10));
        assert_eq!(
            link(&abstain, at(25), Some(&c), Asset::Btc, open(), at(50))["reason"],
            "decision_abstained"
        );
    }
}
