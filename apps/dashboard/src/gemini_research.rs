//! Local request rehearsal. No provider client, external calls or order permissions.
use crate::{decision, forecast::DecisionRow};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{BufWriter, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

// Unknown top-level fields, including evaluator labels, are skipped by serde.
#[derive(Deserialize)]
struct Inventory {
    schema_version: u8,
    read_time: DateTime<Utc>,
    start_ts: i64,
    end_ts: i64,
    decisions: Vec<DecisionRow>,
}

fn side_depth(side: &Value) -> Result<(Decimal, Decimal), &'static str> {
    let levels: Vec<(Decimal, Decimal)> =
        serde_json::from_value(side.clone()).map_err(|_| "invalid_depth")?;
    let best = levels.first().ok_or("empty_depth")?.1;
    let total = levels
        .iter()
        .try_fold(Decimal::ZERO, |sum, level| sum.checked_add(level.1))
        .ok_or("depth_overflow")?;
    Ok((best, total))
}

fn request(packet: &Value) -> Result<Value, &'static str> {
    decision::validate_projection(packet)?;
    let open = packet["open_ts"].as_i64().ok_or("missing_open_time")?;
    let cutoff = decision::time(&packet["cutoff"])?;
    let receipt = decision::time(&packet["book_received_at"])?;
    let price = |key: &str| -> Result<Decimal, &'static str> {
        serde_json::from_value::<Decimal>(packet[key].clone())
            .map(|v| v * Decimal::ONE_HUNDRED)
            .map_err(|_| "invalid_price")
    };
    let bid = price("yes_bid_usd")?;
    let ask = price("yes_ask_usd")?;
    if ask <= bid {
        return Err("no_positive_spread");
    }
    let (yes_best, yes_depth) = side_depth(&packet["book"]["yes_dollars"])?;
    let (no_best, no_depth) = side_depth(&packet["book"]["no_dollars"])?;
    let evidence = json!({
      "asset":packet["asset"], "horizon_seconds":900,
      "official_rule":"YES when the official final reference average is at or above the contract target.",
      "seconds_to_expiry": (open+900) as f64 - cutoff.timestamp_millis() as f64/1000.,
      "book_receipt_age_ms": (cutoff-receipt).num_milliseconds(),
      "provider_publication_age":"unknown",
      "yes_bid_cents":bid,"yes_ask_cents":ask,"yes_midpoint_cents":(bid+ask)/Decimal::TWO,
      "yes_spread_cents":ask-bid,"yes_best_bid_contracts":yes_best,"no_best_bid_contracts":no_best,
      "yes_top_ten_bid_contracts":yes_depth,"no_top_ten_bid_contracts":no_depth
    });
    Ok(
        json!({"model":"gemini-decision-1","input":[{"type":"text","text":evidence.to_string()}],
           "questions":{"official_yes":{"type":"noul","instructions":"Using the supplied opening evidence, will this contract officially settle YES?"}}
        }),
    )
}

fn prepare(inventory: Inventory, output: impl Write) -> Result<Value, Box<dyn std::error::Error>> {
    if inventory.schema_version != 1
        || inventory.start_ts >= inventory.end_ts
        || inventory.end_ts > inventory.read_time.timestamp()
        || inventory.end_ts.saturating_sub(inventory.start_ts) > 7 * 86400
        || inventory.decisions.len() > 2016
    {
        return Err("invalid_inventory".into());
    }
    let mut writer = BufWriter::new(output);
    let mut seen = BTreeSet::new();
    let mut prepared = BTreeMap::new();
    let mut abstained = 0;
    for row in inventory.decisions {
        if !seen.insert((row.open_ts, row.asset.info().series)) {
            return Err("duplicate_decision".into());
        }
        if row.status == "abstain" {
            abstained += 1;
            continue;
        }
        if row.status != "forecast" {
            return Err("unknown_decision_status".into());
        }
        row.check_identity(inventory.start_ts, inventory.end_ts, inventory.read_time)?;
        let payload = match request(&row.packet) {
            Err("no_positive_spread") => {
                abstained += 1;
                continue;
            }
            result => result?,
        };
        let packet_hash = row.packet_hash.as_ref().ok_or("missing_packet_hash")?;
        let envelope = json!({"request_hash":decision::hash(&payload),"request":payload,
            "evaluation_only":{"original_run_id":row.run_id,"generation":row.generation,"packet_hash":packet_hash}});
        serde_json::to_writer(&mut writer, &envelope)?;
        writer.write_all(b"\n")?;
        *prepared.entry(row.asset.info().name).or_insert(0usize) += 1;
    }
    writer.flush()?;
    Ok(
        json!({"prepared_by_asset":prepared,"abstained":abstained,"seen":seen.len(),"provider_calls":0}),
    )
}

pub fn cli() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(std::env::var("FORECAST_INPUT")?)?;
    const LIMIT: u64 = 16 * 1024 * 1024;
    let mut data = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut data)?;
    if data.len() as u64 > LIMIT {
        return Err("inventory_size_limit".into());
    }
    let inventory = serde_json::from_slice(&data)?;
    let out = std::env::var("GEMINI_REHEARSAL_OUTPUT")?;
    let dest = Path::new(&out);
    let tmp = dest.with_extension(format!("{}.tmp", std::process::id()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    let result = (|| -> Result<_, Box<dyn std::error::Error>> {
        let summary = prepare(inventory, &file)?;
        file.sync_all()?;
        std::fs::hard_link(&tmp, dest)?;
        Ok(summary)
    })();
    std::fs::remove_file(tmp)?;
    println!("{}", result?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{assets::Asset, decision::tests::open, forecast::tests::packet_at};
    #[test]
    fn request_contains_relative_time_and_frozen_book_only() {
        let p = packet_at(Asset::Btc, open(), 4);
        let r = request(&p).unwrap();
        let v: Value = serde_json::from_str(r["input"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(v["asset"], "BTC");
        assert_eq!(
            v["yes_bid_cents"].as_str().unwrap().parse::<f64>().unwrap(),
            24.
        );
        assert_eq!(
            v["yes_ask_cents"].as_str().unwrap().parse::<f64>().unwrap(),
            60.
        );
        assert!(v["seconds_to_expiry"].as_f64().unwrap() < 900.);
        assert!(!r.to_string().contains("KXBTC15M"));
        assert!(!r.to_string().contains("2026-"));
        let mut leaked = p.clone();
        leaked["result"] = json!("yes");
        assert!(request(&leaked).is_err());
    }
    #[test]
    fn changing_an_evaluator_label_does_not_change_the_request() {
        let ts = open();
        let p = packet_at(Asset::Eth, ts, 4);
        let id = decision::run_id(Asset::Eth, ts);
        let mut input = json!({
            "schema_version":1,"read_time":DateTime::from_timestamp(ts+1200,0),"start_ts":ts,"end_ts":ts+900,
            "decisions":[{"object_name":format!("decisions/v1/{id}.json"),"generation":"1","committed_at":DateTime::from_timestamp(ts+12,0),"run_id":id,"asset":"ETH","open_ts":ts,"status":"forecast","packet_hash":decision::hash(&p),"packet":p}],
            "labels":[{"yes":true}]});
        let mut first = Vec::new();
        prepare(serde_json::from_value(input.clone()).unwrap(), &mut first).unwrap();
        input["labels"] = json!([{"yes":false}]);
        let mut second = Vec::new();
        prepare(serde_json::from_value(input.clone()).unwrap(), &mut second).unwrap();
        assert_eq!(first, second);
        input["decisions"][0]["committed_at"] = json!(DateTime::from_timestamp(ts + 45, 0));
        assert!(prepare(serde_json::from_value(input).unwrap(), Vec::new()).is_err());
    }
}
