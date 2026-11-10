//! Cutoff-filtered research. Outcome labels and settlement values never enter features.
use crate::{
    assets::Asset,
    blockworks::{Candle, Snapshot},
    outcomes::Contract,
};
use chrono::DateTime;
use serde_json::{Value, json};

type Failure = Box<dyn std::error::Error + Send + Sync>;
fn timestamp(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s).ok().map(|t| t.timestamp())
}
fn number(v: &Value) -> Option<f64> {
    let n = v.as_f64().or_else(|| v.as_str()?.parse().ok())?;
    n.is_finite().then_some(n)
}
fn cmc_before(context: &Value, asset: Asset, cutoff: i64) -> Value {
    let symbol = serde_json::to_value(asset).unwrap_or(Value::Null);
    let best = context["history"].as_array().into_iter().flatten().filter_map(|s| {
        let at = timestamp(s["at"].as_str()?)?;
        if at > cutoff || cutoff-at > 660 { return None; }
        let quotes: Vec<_> = s["quotes"].as_array()?.iter().filter(|q|q["symbol"]==symbol).collect();
        if quotes.len()!=1 { return None; }
        let q = quotes[0]; let source = timestamp(q["source_at"].as_str()?)?;
        let price = q["price"].as_f64().filter(|p|p.is_finite() && *p>0.)?;
        if source > at || cutoff-source > 660 { return None; }
        Some((at, json!({"status":"available","price":price,"source_at":source,"collection_at":at,
            "age_seconds":cutoff-source,"source":"CoinMarketCap aggregate price",
            "availability":"Collection/source timestamps precede cutoff; first receipt was not separately archived"})))
    }).max_by_key(|(at,_)|*at);
    best.map(|(_,v)|v).unwrap_or_else(||json!({"status":"unavailable","reason":"No fresh saved CMC sample with both timestamps at or before cutoff"}))
}
fn quote_before(detail: &Value, start: i64, cutoff: i64, captured: Option<i64>) -> Value {
    let candles: Vec<Value> = detail["candles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let t = c["time"].as_i64()?;
            if t < start || t % 60 != 0 || t.checked_add(60)? > cutoff {
                return None;
            }
            let o = number(&c["open"])?;
            let h = number(&c["high"])?;
            let l = number(&c["low"])?;
            let cl = number(&c["close"])?;
            if l < 0. || h > 1. || l > o.min(cl) || h < o.max(cl) {
                return None;
            }
            Some(json!({"time":t,"open":o,"high":h,"low":l,"close":cl}))
        })
        .collect();
    json!({"source":"Archived Kalshi one-minute YES ask OHLC","candles":candles,"archive_captured_at":captured,
        "availability":"Retrospective quote archive; not a contemporaneous order-book recording"})
}
fn complete_return(candles: &[Candle], end: i64, n: i64) -> Option<f64> {
    let bars: Vec<_> = (0..n)
        .map(|i| candles.iter().find(|c| c.time == end - (n - i) * 300))
        .collect::<Option<_>>()?;
    Some((bars.last()?.close / bars.first()?.open - 1.) * 100.)
}
fn market_before(snapshot: Option<&Snapshot>, asset: Asset, cutoff: i64) -> Value {
    let Some(s) = snapshot else {
        return json!({"status":"unavailable","reason":"No licensed saved price/volume pilot available"});
    };
    let end = cutoff / 300 * 300;
    let start = end - 3600;
    let candles: Vec<Candle> = s
        .series
        .iter()
        .find(|r| r.asset == asset)
        .into_iter()
        .flat_map(|r| &r.candles)
        // Conservative interval-end rule: a provider timestamp is allowed only after another full 5m.
        .filter(|c| c.time >= start && c.time >= s.start && c.time + 300 <= end && c.time < s.end)
        .cloned()
        .collect();
    let volume = |from: i64, to: i64| -> Option<f64> {
        let mut sum = 0.;
        for t in (from..to).step_by(300) {
            sum += candles.iter().find(|c| c.time == t)?.volume?;
        }
        sum.is_finite().then_some(sum)
    };
    let recent_volume = volume(end - 900, end);
    let relative_volume = recent_volume
        .zip(volume(start, end - 900))
        .and_then(|(r, previous)| (previous > 0.).then_some(r / (previous / 3.)));
    let typical_move = (candles.len() == 12).then(|| {
        candles
            .iter()
            .map(|c| (c.close / c.open - 1.).abs() * 100.)
            .sum::<f64>()
            / 12.
    });
    json!({"status":"available","source":"Blockworks","model":"asset-price","granularity":"5m",
        "start":start,"end":end,"cutoff":cutoff,"captured_at":s.captured_at,
        "volume_unit":s.volume_unit,"expected_candles":12,"valid_candles":candles.len(),"candles":candles,
        "availability":"Retrospective history; original publication/receipt time unverified",
        "timestamp_rule":"Provider timestamp + five minutes must be at or before cutoff; bucket convention remains unverified",
        "metrics":{"return_5m_pct":complete_return(&candles,end,1),"return_15m_pct":complete_return(&candles,end,3),
            "return_60m_pct":complete_return(&candles,end,12),"typical_5m_move_pct":typical_move,
            "volume_15m":recent_volume,"relative_volume_15m":relative_volume}})
}
pub fn before(
    contract: &Contract,
    detail: &Value,
    context: &Value,
    snapshot: Option<&Snapshot>,
    minute: u8,
    now: i64,
) -> Result<Value, Failure> {
    let start = timestamp(&contract.open_time).ok_or("Invalid open time")?;
    let end = timestamp(&contract.close_time).ok_or("Invalid close time")?;
    if ![0, 5, 10, 14].contains(&minute)
        || end > now
        || end - start != 900
        || detail["asset"] != serde_json::to_value(contract.asset)?
        || detail["ticker"] != contract.ticker
        || !matches!(contract.result.as_str(), "yes" | "no")
    {
        return Err("Invalid completed research window".into());
    }
    let cutoff = start + i64::from(minute) * 60;
    Ok(
        json!({"status":"available","schema_version":1,"asset":contract.asset,"ticker":contract.ticker,
        "open_time":contract.open_time,"close_time":contract.close_time,"cutoff":cutoff,"minute":minute,
        "target":{"source_text":contract.target,"availability":"From settled archive; pre-cutoff observation not recorded"},
        "reference":{"status":"not_recorded","reason":"Historical CF observations were not archived for this cutoff; aggregate prices cannot replace them"},
        "order_book":{"status":"not_recorded","reason":"No contemporaneous historical depth or bid/ask spread recording"},
        "cmc":cmc_before(context,contract.asset,cutoff),
        "contract_quotes":quote_before(detail,start,cutoff,timestamp(&contract.captured_at)),
        "market_context":market_before(snapshot,contract.asset,cutoff),
        "notes":"Outcome, expiration value and hindsight best-entry are excluded. This is descriptive research, not a point-in-time training dataset."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockworks::Series;
    fn inputs() -> (Contract, Value, Value, Snapshot) {
        let c = Contract {
            ticker: "KXBTC15M-TEST".into(),
            asset: Asset::Btc,
            open_time: "2026-11-09T20:00:00Z".into(),
            close_time: "2026-11-09T20:15:00Z".into(),
            result: "yes".into(),
            target: Some("At or above $75,000".into()),
            expiration_value: Some("99999".into()),
            updated_time: None,
            best_entry: None,
            candle_count: 15,
            captured_at: "2026-11-09T20:20:00Z".into(),
        };
        let t = timestamp(&c.open_time).unwrap();
        let detail = json!({"asset":"BTC","ticker":c.ticker,"candles":[{"time":t,"open":"0.3","high":"0.5","low":"0.2","close":"0.4"},{"time":t+600,"open":"0.9","high":"1","low":"0.8","close":"0.99"}]});
        let context = json!({"history":[{"at":"2026-11-09T20:05:00Z","quotes":[{"symbol":"BTC","price":75000.,"source_at":"2026-11-09T20:04:00Z"}]},{"at":"2026-11-09T20:11:00Z","quotes":[{"symbol":"BTC","price":99999.,"source_at":"2026-11-09T20:11:00Z"}]}]});
        let bars = (0..14)
            .map(|i| Candle {
                time: t - 3600 + i * 300,
                open: 100.,
                high: 102.,
                low: 99.,
                close: 101.,
                volume: Some(10.),
            })
            .collect();
        let snapshot = Snapshot {
            schema_version: 1,
            provider: "Blockworks".into(),
            model: "asset-price".into(),
            granularity: "5m".into(),
            captured_at: t + 3600,
            start: t - 3600,
            end: t + 600,
            volume_unit: "unverified".into(),
            series: vec![Series {
                asset: Asset::Btc,
                asset_id: "test".into(),
                candles: bars,
                rejected_points: 0,
            }],
        };
        (c, detail, context, snapshot)
    }
    #[test]
    fn excludes_unfinished_quotes_future_samples_and_hindsight_labels() {
        let (c, d, context, s) = inputs();
        let out = before(&c, &d, &context, Some(&s), 10, i64::MAX).unwrap();
        assert_eq!(
            out["contract_quotes"]["candles"].as_array().unwrap().len(),
            1
        );
        assert_eq!(out["cmc"]["price"], 75000.);
        assert!(out.get("result").is_none());
        assert!(out.get("expiration_value").is_none());
        assert!(out.get("best_entry").is_none());
        let cutoff = out["cutoff"].as_i64().unwrap();
        assert!(
            out["market_context"]["candles"]
                .as_array()
                .unwrap()
                .iter()
                .all(|b| b["time"].as_i64().unwrap() + 300 <= cutoff)
        );
    }
    #[test]
    fn changing_the_winner_or_settlement_cannot_change_before_features() {
        let (mut c, d, context, s) = inputs();
        let a = before(&c, &d, &context, Some(&s), 5, i64::MAX).unwrap();
        c.result = "no".into();
        c.expiration_value = Some("1".into());
        assert_eq!(a, before(&c, &d, &context, Some(&s), 5, i64::MAX).unwrap());
    }
    #[test]
    fn incomplete_ranges_refuse_returns_and_null_volume_is_not_zero() {
        let (c, d, context, mut s) = inputs();
        s.series[0].candles.retain(|b| b.time != s.end - 600);
        let v = before(&c, &d, &context, Some(&s), 10, i64::MAX).unwrap();
        assert!(v["market_context"]["metrics"]["return_15m_pct"].is_null());
        assert!(v["market_context"]["metrics"]["relative_volume_15m"].is_null());
        let (_, _, _, mut s) = inputs();
        s.series[0].candles.last_mut().unwrap().volume = None;
        assert!(
            before(&c, &d, &context, Some(&s), 10, i64::MAX).unwrap()["market_context"]["metrics"]
                ["volume_15m"]
                .is_null()
        );
    }
    #[test]
    fn rejects_future_windows_unsupported_cutoffs_and_cross_asset_detail() {
        let (c, mut d, context, s) = inputs();
        assert!(before(&c, &d, &context, Some(&s), 15, i64::MAX).is_err());
        assert!(before(&c, &d, &context, Some(&s), 0, 0).is_err());
        d["asset"] = json!("ETH");
        assert!(before(&c, &d, &context, Some(&s), 0, i64::MAX).is_err());
    }
    #[test]
    fn no_missing_reference_book_or_pilot_values_are_invented() {
        let (c, d, context, _) = inputs();
        let out = before(&c, &d, &context, None, 0, i64::MAX).unwrap();
        assert_eq!(out["reference"]["status"], "not_recorded");
        assert_eq!(out["order_book"]["status"], "not_recorded");
        assert_eq!(out["market_context"]["status"], "unavailable");
        assert_eq!(out["cmc"]["status"], "unavailable");
        assert!(
            out["contract_quotes"]["candles"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
