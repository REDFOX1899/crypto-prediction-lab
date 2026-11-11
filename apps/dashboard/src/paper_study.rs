//! Operator-only daily counterfactual replay. No exchange calls or account credentials.
//! Scorecard is the only function reachable from blinded MCP.
use crate::{decision, forecast, market_rules, paper_levels::Fees, prospective};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashMap, io::Write, os::unix::fs::OpenOptionsExt};

type Result<T> = std::result::Result<T, &'static str>;
const VERSION: &str = "opening_ioc_study_v1";
fn money(v: &Value) -> Result<Decimal> {
    serde_json::from_value(v.clone()).map_err(|_| "invalid_decimal")
}
struct Rules {
    ticks: Vec<Decimal>,
    coefficient: Decimal,
}
impl Rules {
    fn from_proof(packet: &Value, proof: Option<&Value>) -> Result<Self> {
        let proof = proof.ok_or("no_committed_fee_rules")?;
        market_rules::validate(proof, packet)?;
        let config = &proof["configuration"];
        if !matches!(
            config["effective_fee_type"].as_str(),
            Some("quadratic" | "quadratic_with_maker_fees" | "quadratic_with_combo_maker_fees")
        ) {
            return Err("unsupported_taker_fee");
        }
        let bands = serde_json::from_value::<Vec<kalshi_client::PriceRange>>(
            config["price_ranges"].clone(),
        )
        .map_err(|_| "invalid_grid")?;
        let grid = market_rules::ticks(&bands)?;
        let ticks = grid
            .iter()
            .copied()
            .filter(|price| grid.binary_search(&(Decimal::ONE - price)).is_ok())
            .collect();
        Ok(Self {
            ticks,
            coefficient: Decimal::new(7, 2) * money(&config["effective_fee_multiplier"])?,
        })
    }
    fn fee(&self, accumulator: &mut Fees, price: Decimal, qty: Decimal) -> Decimal {
        accumulator.apply_model_fee(
            price * qty,
            self.coefficient * qty * price * (Decimal::ONE - price),
        )
    }
    fn cap(&self, chance: Decimal) -> Option<Decimal> {
        self.ticks.iter().copied().rev().find(|p| {
            // Require one cent margin plus half a cent residual allowance.
            chance - p - self.fee(&mut Fees::default(), *p, Decimal::ONE) >= Decimal::new(15, 3)
        })
    }
}
fn card(packet: &Value, rules: Result<&Rules>, p: Option<Decimal>) -> Result<Value> {
    let ask = money(&packet["yes_ask_usd"])?;
    let no = Decimal::ONE - money(&packet["yes_bid_usd"])?;
    let reason = match (rules, p) {
        (Err(e), _) => e,
        (_, None) => "no_committed_forecast",
        _ => "no_validated_probability_bounds",
    };
    let level = |price: Decimal, probability: Option<Decimal>| {
        let fee = rules
            .ok()
            .map(|r| r.fee(&mut Fees::default(), price, Decimal::ONE));
        json!({"offer":price,"modeled_fee":fee,"break_even":fee.map(|f|price+f),
            "estimated_margin":probability.zip(fee).map(|(p,f)|p-price-f),
            "point_watch_price":rules.ok().and_then(|r|r.cap(probability?)),
            "eligible_price":null})
    };
    Ok(
        json!({"version":VERSION,"midpoint":packet["yes_midpoint_usd"],"p_yes":p,
        "YES":level(ask,p),"NO":level(no,p.map(|x|Decimal::ONE-x)),
        "decision":"pass","reason":reason,"paper_order_eligible":false,
        "cost_basis":"One contract, exchange taker configuration, assumed non-direct member rounding. Watch prices use the point estimate."}),
    )
}
pub fn scorecard(
    packet: &Value,
    proof: Option<&Value>,
    forecast: Option<&forecast::Frozen>,
) -> Result<Value> {
    let rules = Rules::from_proof(packet, proof);
    let p = forecast
        .map(|f| f.response(packet))
        .transpose()?
        .map(|f| money(&f["p_yes_calibrated"]))
        .transpose()?;
    card(packet, rules.as_ref().map_err(|e| *e), p)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    // Original decision bytes read at the generation in the frozen J1 manifest.
    record: Value,
    candidate: Option<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    evaluation: prospective::Input,
    evidence: HashMap<String, Evidence>,
}
#[derive(Clone, Copy)]
enum Strategy {
    Validated,
    Point,
    Midpoint,
    Favorite,
    Adverse,
}
impl Strategy {
    fn name(self) -> &'static str {
        match self {
            Self::Validated => "validated",
            Self::Point => "point_scenario",
            Self::Midpoint => "midpoint_scenario",
            Self::Favorite => "market_favorite",
            Self::Adverse => "point_stress",
        }
    }
}
#[derive(Clone, Copy)]
struct Intent {
    yes: bool,
    cap: Decimal,
}
fn intent(strategy: Strategy, packet: &Value, rules: &Rules, p: Decimal) -> Result<Option<Intent>> {
    if matches!(strategy, Strategy::Validated) {
        return Ok(None);
    }
    if matches!(strategy, Strategy::Favorite) {
        let midpoint = money(&packet["yes_midpoint_usd"])?;
        if midpoint == Decimal::new(5, 1) {
            return Ok(None);
        }
        let yes = midpoint > Decimal::new(5, 1);
        let cap = if yes {
            money(&packet["yes_ask_usd"])?
        } else {
            Decimal::ONE - money(&packet["yes_bid_usd"])?
        };
        return Ok(
            (cap + rules.fee(&mut Fees::default(), cap, Decimal::ONE) <= Decimal::ONE)
                .then_some(Intent { yes, cap }),
        );
    }
    let chance = if matches!(strategy, Strategy::Midpoint) {
        money(&packet["yes_midpoint_usd"])?
    } else {
        p
    };
    for (yes, prob, offer) in [
        (true, chance, money(&packet["yes_ask_usd"])?),
        (
            false,
            Decimal::ONE - chance,
            Decimal::ONE - money(&packet["yes_bid_usd"])?,
        ),
    ] {
        if let Some(cap) = rules.cap(prob)
            && offer <= cap
        {
            return Ok(Some(Intent { yes, cap }));
        }
    }
    Ok(None)
}
#[derive(Default)]
struct Fill {
    qty: Decimal,
    cost: Decimal,
    fees: Decimal,
    levels: Vec<Value>,
}
fn walk(intent: Intent, rules: &Rules, candidate: &Value, stress: bool) -> Result<Fill> {
    let bids: Vec<(Decimal, Decimal)> = serde_json::from_value(
        candidate["book"][if intent.yes {
            "no_dollars"
        } else {
            "yes_dollars"
        }]
        .clone(),
    )
    .map_err(|_| "invalid_book")?;
    let mut fill = Fill::default();
    let mut fees = Fees::default();
    for (bid, available) in bids {
        let ask = Decimal::ONE - bid;
        let index = rules
            .ticks
            .binary_search(&ask)
            .map_err(|_| "candidate_off_grid")?;
        let price = if stress {
            let Some(p) = rules.ticks.get(index + 1) else {
                break;
            };
            *p
        } else {
            ask
        };
        if price > intent.cap {
            break;
        }
        // Size resolution is hundredths of a contract. Leave smaller residuals unfilled.
        if available <= Decimal::ZERO || available.normalize().scale() > 2 {
            return Err("unsupported_size");
        }
        let displayed = if stress {
            (available / Decimal::TWO).trunc_with_scale(2)
        } else {
            available
        };
        let qty = displayed.min(Decimal::ONE - fill.qty);
        if qty.is_zero() {
            continue;
        }
        let fee = rules.fee(&mut fees, price, qty);
        fill.levels.push(json!({"price":price,"qty":qty,"fee":fee}));
        fill.cost += qty * price;
        fill.fees += fee;
        fill.qty += qty;
        if fill.qty == Decimal::ONE {
            break;
        }
    }
    if fill.cost + fill.fees > Decimal::ONE {
        return Err("cost_exceeds_reserved_dollar");
    }
    Ok(fill)
}

/// One non-recycled dollar per scheduled market per strategy, at most $3 per
/// aligned cycle and $288 per UTC day. Payouts cannot influence subsequent intents.
#[derive(Default)]
struct Wallet {
    spent: Decimal,
    paid: Decimal,
    reserved: Decimal,
    unsettled_qty: Decimal,
    fills: usize,
    passes: usize,
    no_fills: usize,
    unknown: usize,
    fees: Decimal,
    settlements: Vec<(DateTime<Utc>, Decimal)>,
    cash_events: Vec<(DateTime<Utc>, Decimal)>,
}
impl Wallet {
    fn summary(&mut self, expected: usize) -> Value {
        self.cash_events.sort_by_key(|e| e.0);
        let mut series = Vec::new();
        let (mut cash, mut peak, mut drawdown) =
            (Decimal::from(288), Decimal::from(288), Decimal::ZERO);
        for (time, change) in &self.cash_events {
            cash += change;
            peak = peak.max(cash);
            drawdown = drawdown.max(peak - cash);
            series.push(json!({"at":time,"cash":cash}));
        }
        self.settlements.sort_by_key(|e| e.0);
        let mut equity_path = Vec::new();
        let mut equity = Decimal::from(288);
        let mut high = equity;
        let mut equity_drawdown = Decimal::ZERO;
        for (time, net) in &self.settlements {
            equity += net;
            high = high.max(equity);
            equity_drawdown = equity_drawdown.max(high - equity);
            equity_path.push(json!({"at":time,"equity":equity,"drawdown":high-equity}));
        }
        let cash_delta = self.paid - self.spent;
        let bounds = [
            cash_delta - self.reserved,
            cash_delta + self.unsettled_qty + self.reserved,
        ];
        let complete = self.reserved.is_zero() && self.unsettled_qty.is_zero();
        json!({"scheduled":expected,"passes":self.passes,"filled":self.fills,"verified_no_fill":self.no_fills,"unknown_execution":self.unknown,
            "modeled_fees":self.fees,"capital_spent":self.spent,"settled_payout":self.paid,"unknown_reservation":self.reserved,
            "unsettled_quantity":self.unsettled_qty,"net_pnl":complete.then_some(cash_delta),"net_bounds":bounds,
            "net_per_scheduled":(complete && expected>0).then(||cash_delta/Decimal::from(expected)),
            "net_per_dollar_spent":(complete && !self.spent.is_zero()).then(||cash_delta/self.spent),
            "free_cash":Decimal::from(288)+cash_delta-self.reserved,
            "settled_equity_path":equity_path,"settled_drawdown":equity_drawdown,"observed_cash_drawdown":drawdown,"observed_cash_path":series})
    }
}
struct Opportunity<'a> {
    packet: &'a Value,
    card: Value,
    probability: Decimal,
    rules: Result<Rules>,
    candidate: Value,
    label: Option<&'a prospective::Label>,
    previous_label: Option<&'a prospective::Label>,
    committed: DateTime<Utc>,
}
fn opportunity<'a>(
    row: &prospective::Row,
    input: &'a prospective::Input,
    evidence: &HashMap<String, Evidence>,
) -> Result<Opportunity<'a>> {
    let probs = row.probabilities.ok_or("forecast_unavailable")?;
    let object = row.decision.as_ref().ok_or("missing_decision")?;
    let saved = input
        .decisions
        .iter()
        .find(|s| s.object == *object)
        .ok_or("missing_decision")?;
    let packet = &saved.record["packet"];
    let extra = evidence.get(&object.name);
    if extra.is_some_and(|e| prospective::decision_projection(&e.record) != saved.record) {
        return Err("decision_version_mismatch");
    }
    let proof = extra.map(|e| &e.record).and_then(|r| {
        r.get("market_rules")
            .filter(|p| r["rules_hash"] == decision::hash(p))
    });
    let rules = Rules::from_proof(packet, proof);
    let probability = money(&json!(probs[4]))?;
    let card = card(packet, rules.as_ref().map_err(|e| *e), Some(probability))?;
    let candidate = decision::link(
        &saved.record,
        object.committed_at,
        extra
            .and_then(|e| e.candidate.as_ref())
            .filter(|d| decision::time(&d["createTime"]).is_ok_and(|t| t <= input.asof)),
        row.asset,
        row.open_ts,
        input.asof,
    );
    let label = input
        .labels
        .iter()
        .find(|l| row.label_id.as_ref() == Some(&l.id()) && packet["ticker"] == l.ticker);
    let previous_label = label.and_then(|current| {
        input
            .previous_labels
            .iter()
            .filter(|l| {
                l.asset == row.asset
                    && l.open_ts == row.open_ts
                    && l.ticker == current.ticker
                    && l.available_at < current.available_at
            })
            .max_by_key(|l| l.available_at)
    });
    Ok(Opportunity {
        previous_label,
        packet,
        card,
        probability,
        rules,
        candidate,
        label,
        committed: object.committed_at,
    })
}
fn simulate(strategy: Strategy, op: &Opportunity, wallet: &mut Wallet) -> Result<Value> {
    let eligible = op
        .rules
        .as_ref()
        .ok()
        .map(|rules| intent(strategy, op.packet, rules, op.probability))
        .transpose()?
        .flatten();
    let Some(order) = eligible else {
        wallet.passes += 1;
        let reason = match (&op.rules, strategy) {
            (Err(e), _) => *e,
            (_, Strategy::Favorite) => "midpoint_tie_or_budget_limit",
            (_, Strategy::Validated) => "no_validated_probability_bounds",
            _ => "offer_above_watch_price",
        };
        return Ok(json!({"status":"pass","reason":reason}));
    };
    let rules = op.rules.as_ref().map_err(|e| *e)?;
    if wallet.spent + wallet.reserved + Decimal::ONE > Decimal::from(288) {
        wallet.passes += 1;
        return Ok(json!({"status":"pass","reason":"insufficient_virtual_funds"}));
    }
    let side = if order.yes { "YES" } else { "NO" };
    let candidate = &op.candidate["candidate"];
    let stress = matches!(strategy, Strategy::Adverse);
    let execution = || -> Result<Fill> {
        if candidate.is_null() {
            return Err("missing_eligible_execution_book");
        }
        if stress
            && decision::time(&candidate["book_request_started_at"])?
                < op.committed + chrono::Duration::seconds(10)
        {
            return Err("no_book_after_stressed_latency");
        }
        walk(order, rules, candidate, stress)
    };
    let mut record =
        json!({"side":side,"cap":order.cap,"reserved_usd":"1","intent_at":op.committed,"fills":[]});
    let fill = match execution() {
        Ok(f) => f,
        Err(reason) => {
            wallet.reserved += Decimal::ONE;
            wallet.unknown += 1;
            record["status"] = json!("unknown_execution");
            record["reason"] = json!(reason);
            return Ok(record);
        }
    };
    record["execution_at"] = candidate["source_created_at"].clone();
    record["execution_book"] = candidate["source_id"].clone();
    record["fills"] = json!(fill.levels);
    record["filled_qty"] = json!(fill.qty);
    record["ioc_canceled_qty"] = json!(Decimal::ONE - fill.qty);
    if fill.qty.is_zero() {
        wallet.no_fills += 1;
        record["status"] = json!("no_fill");
        return Ok(record);
    }
    let spent = fill.cost + fill.fees;
    wallet.fills += 1;
    wallet.spent += spent;
    wallet.fees += fill.fees;
    wallet
        .cash_events
        .push((decision::time(&record["execution_at"])?, -spent));
    record["status"] = json!("filled");
    record["cost_with_fees"] = json!(spent);
    let prior = op
        .previous_label
        .filter(|l| matches!(l.result.as_str(), "yes" | "no"));
    let prior_payout = prior
        .map(|l| {
            if (l.result == "yes") == order.yes {
                fill.qty
            } else {
                Decimal::ZERO
            }
        })
        .unwrap_or_default();
    if let Some(l) = prior {
        wallet.cash_events.push((l.available_at, prior_payout));
        wallet
            .settlements
            .push((l.available_at, prior_payout - spent));
        record["previous_settlement"] =
            json!({"label_id":l.id(),"at":l.available_at,"result":l.result,"payout":prior_payout});
    }
    if let Some(l) = op.label
        && matches!(l.result.as_str(), "yes" | "no")
    {
        let payout = if (l.result == "yes") == order.yes {
            fill.qty
        } else {
            Decimal::ZERO
        };
        wallet.paid += payout;
        wallet
            .cash_events
            .push((l.available_at, payout - prior_payout));
        wallet.settlements.push((
            l.available_at,
            payout - if prior.is_some() { prior_payout } else { spent },
        ));
        record["settled_at"] = json!(l.available_at);
        record["label_id"] = json!(l.id());
        record["net_pnl"] = json!(payout - spent);
    } else {
        if let (Some(_), Some(l)) = (prior, op.label) {
            wallet.cash_events.push((l.available_at, -prior_payout));
            wallet
                .settlements
                .push((l.available_at, spent - prior_payout));
            record["invalidated_at"] = json!(l.available_at);
        }
        wallet.unsettled_qty += fill.qty;
        record["settlement"] = json!(if op.label.is_some() {
            "non_binary_treatment_unknown"
        } else {
            "pending"
        });
    }
    Ok(record)
}
fn study(bundle: Bundle) -> Result<Value> {
    let (input, report, rows) = prospective::evaluate(bundle.evaluation)?;
    let strategies = [
        Strategy::Validated,
        Strategy::Point,
        Strategy::Midpoint,
        Strategy::Adverse,
        Strategy::Favorite,
    ];
    let mut wallets: [Wallet; 5] = Default::default();
    let mut cycles = Vec::new();
    for row in &rows {
        let mut cycle =
            json!({"asset":row.asset,"open_ts":row.open_ts,"forecast_status":row.forecast_status});
        match opportunity(row, &input, &bundle.evidence) {
            Ok(op) => {
                cycle["scorecard"] = op.card.clone();
                cycle["ticker"] = op.packet["ticker"].clone();
                cycle["cutoff"] = op.packet["cutoff"].clone();
                cycle["committed_at"] = json!(op.committed);
                cycle["packet_hash"] = json!(decision::hash(op.packet));
                cycle["candidate_at"] = op.candidate["candidate"]["source_created_at"].clone();
                cycle["book_received_at"] = op.packet["book_received_at"].clone();
                cycle["decision_generation"] = json!(row.decision.as_ref().map(|d| &d.generation));
                cycle["spread"] = op.packet["spread_usd"].clone();
                cycle["execution_book_received_at"] =
                    op.candidate["candidate"]["book_received_at"].clone();
                cycle["candidate_reason"] = op.candidate["reason"].clone();
                if let Some(l) = op.label {
                    cycle["official_result"] = json!(l.result);
                    cycle["result_available_at"] = json!(l.available_at);
                }
                let mut simulations = serde_json::Map::new();
                for (i, s) in strategies.iter().enumerate() {
                    simulations.insert(s.name().into(), simulate(*s, &op, &mut wallets[i])?);
                }
                cycle["strategies"] = Value::Object(simulations);
            }
            Err("forecast_unavailable" | "missing_decision") => {
                for (i, s) in strategies.iter().enumerate() {
                    wallets[i].passes += 1;
                    cycle["strategies"][s.name()] =
                        json!({"status":"pass","reason":row.forecast_status});
                }
            }
            Err(e) => return Err(e),
        }
        cycles.push(cycle);
    }
    let mut health = serde_json::Map::new();
    for asset in crate::assets::Asset::ALL {
        let symbol = prospective::symbol(asset);
        let slots: Vec<_> = cycles.iter().filter(|r| r["asset"] == symbol).collect();
        let latency = |key: &str, offset: i64| {
            let mut ms: Vec<_> = slots
                .iter()
                .filter_map(|r| {
                    Some(
                        decision::time(&r[key]).ok()?.timestamp_millis()
                            - (r["open_ts"].as_i64()? + offset) * 1000,
                    )
                })
                .collect();
            ms.sort_unstable();
            let q = |p: usize| ms.get((ms.len() * p).div_ceil(100).saturating_sub(1));
            json!({"n":ms.len(),"p50_ms":q(50),"p95_ms":q(95)})
        };
        health.insert(symbol,json!({"expected":slots.len(),"opening_receipt":latency("book_received_at",0),"decision_commit":latency("committed_at",0),
          "execution_receipt_after_target":latency("execution_book_received_at",45),"official_availability_after_close":latency("result_available_at",900)}));
    }
    let mut totals = serde_json::Map::new();
    for (i, s) in strategies.iter().enumerate() {
        totals.insert(s.name().into(), wallets[i].summary(rows.len()));
    }
    Ok(
        json!({"schema":VERSION,"phase":"retrospective_engineering","asof":input.asof,"day":input.day,
        "forecast_experiment":input.experiment.manifest.id(),"engine_sha256":decision::hash(&json!([
        include_str!("paper_study.rs"),include_str!("paper_levels.rs"),include_str!("decision.rs"),include_str!("forecast.rs"),
        include_str!("market_rules.rs"),include_str!("prospective.rs"),include_str!("order_book.rs"),include_str!("checkpoint.rs"),
        include_str!("../../../Cargo.lock")])),
        "capture_health":health,"external_model_calls":0,"observed_cloud_cost":null,"evaluation_hash":decision::hash(&report),"forecast_evaluation":report["evaluation"],"official_corrections":report["official_outcome_corrections"],"previous_evaluation":report["previous_report"],"cycles":cycles,"strategies":totals,
        "abstain_net_usd":"0","portfolio_basis":"Independent daily virtual wallets; one non-recycled dollar per asset/window. Counterfactual displayed-liquidity IOC, one contract. No actual orders."}),
    )
}
pub fn cli() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var("EVALUATION_INPUT")?;
    if std::fs::metadata(&path)?.len() > 32 * 1024 * 1024 {
        return Err("paper input exceeds one day limit".into());
    }
    let bundle = serde_json::from_slice(&std::fs::read(path)?)?;
    let report = study(bundle)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(std::env::var("EVALUATION_OUTPUT")?)?;
    file.write_all(&serde_json::to_vec(&report)?)?;
    println!("Saved private paper report {}", decision::hash(&report));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assets::Asset,
        decision::tests::{at, open},
        market_rules::tests::with_rules,
    };
    fn grid() -> Rules {
        Rules {
            ticks: (1..100).map(|n| Decimal::new(n, 2)).collect(),
            coefficient: Decimal::new(7, 2),
        }
    }
    #[test]
    fn fee_multiplier_dynamic_grid_and_blinded_scorecard() {
        for asset in Asset::ALL {
            let (doc, packet) = with_rules(asset);
            let proof = market_rules::project(&doc, &packet).unwrap();
            let mut rules = Rules::from_proof(&packet, Some(&proof)).unwrap();
            assert!(rules.ticks.contains(&Decimal::new(999, 3)));
            let p = Decimal::new(56, 2);
            assert_eq!(rules.cap(p), Some(Decimal::new(52, 2)));
            rules.coefficient *= Decimal::TWO;
            assert_eq!(rules.cap(p), Some(Decimal::new(50, 2)));
            let saved = scorecard(&packet, Some(&proof), None).unwrap();
            assert_eq!(saved["reason"], "no_committed_forecast");
            assert!(saved["YES"]["eligible_price"].is_null());
            let mut tampered = proof;
            tampered["configuration"]["effective_fee_multiplier"] = json!("0");
            assert!(Rules::from_proof(&packet, Some(&tampered)).is_err());
        }
    }
    #[test]
    fn both_sides_partial_fills_and_later_price_above_limit() {
        let c = json!({"book":{"no_dollars":[["0.52","0.3"],["0.51","0.4"],["0.44","10"]],"yes_dollars":[["0.55","50"]]}});
        for (yes, qty, cost) in [
            (true, Decimal::new(7, 1), Decimal::new(34, 2)),
            (false, Decimal::ONE, Decimal::new(45, 2)),
        ] {
            let f = walk(
                Intent {
                    yes,
                    cap: Decimal::new(50, 2),
                },
                &grid(),
                &c,
                false,
            )
            .unwrap();
            assert_eq!((f.qty, f.cost), (qty, cost));
            assert!(f.fees > Decimal::ZERO);
        }
        assert_eq!(
            walk(
                Intent {
                    yes: true,
                    cap: Decimal::new(45, 2)
                },
                &grid(),
                &c,
                false
            )
            .unwrap()
            .qty,
            Decimal::ZERO
        );
    }
    fn label(result: &str, sec: i64) -> prospective::Label {
        prospective::Label {
            asset: Asset::Btc,
            ticker: "test".to_owned(),
            open_ts: open(),
            close_ts: open() + 900,
            result: result.into(),
            source_id: "test".into(),
            source_version: at(sec).to_rfc3339(),
            captured_at: at(sec),
            available_at: at(sec),
        }
    }
    fn trade<'a>(
        packet: &'a Value,
        yes: bool,
        prior: Option<&'a prospective::Label>,
        current: Option<&'a prospective::Label>,
    ) -> Opportunity<'a> {
        Opportunity {
            packet,
            card: Value::Null,
            probability: if yes {
                Decimal::new(85, 2)
            } else {
                Decimal::new(15, 2)
            },
            rules: Ok(grid()),
            candidate: json!({"candidate":{"source_created_at":at(50),"book_request_started_at":at(45),"book":{
                "yes_dollars":[["0.60","1"]],"no_dollars":[["0.48","1"]]}}}),
            label: current,
            previous_label: prior,
            committed: at(10),
        }
    }
    #[test]
    fn settles_yes_no_corrects_and_preserves_pending() {
        let p = json!({"yes_ask_usd":"0.57","yes_bid_usd":"0.45","yes_midpoint_usd":"0.51"});
        for yes in [true, false] {
            let first = label(if yes { "no" } else { "yes" }, 905);
            let second = label(if yes { "yes" } else { "no" }, 945);
            let mut wallet = Wallet::default();
            let r = simulate(
                Strategy::Point,
                &trade(&p, yes, Some(&first), Some(&second)),
                &mut wallet,
            )
            .unwrap();
            let net = money(&r["net_pnl"]).unwrap();
            assert_eq!(
                net,
                if yes {
                    Decimal::new(46, 2)
                } else {
                    Decimal::new(58, 2)
                }
            );
            assert_eq!(wallet.settlements.len(), 2);
            assert_eq!(money(&wallet.summary(1)["net_pnl"]).unwrap(), net);
            let void = label("void", 950);
            let mut wallet = Wallet::default();
            let r = simulate(
                Strategy::Point,
                &trade(&p, yes, Some(&second), Some(&void)),
                &mut wallet,
            )
            .unwrap();
            assert_eq!(r["settlement"], "non_binary_treatment_unknown");
            assert_eq!(wallet.paid, Decimal::ZERO);
            assert!(wallet.summary(1)["net_pnl"].is_null());
        }
    }
    #[test]
    fn unknown_depth_and_budget_never_become_zero_pnl_or_fill() {
        let p = json!({"yes_ask_usd":"0.55","yes_bid_usd":"0.48"});
        let mut op = trade(&p, true, None, None);
        op.candidate = Value::Null;
        let mut wallet = Wallet::default();
        let r = simulate(Strategy::Point, &op, &mut wallet).unwrap();
        assert_eq!(r["status"], "unknown_execution");
        assert_eq!(wallet.reserved, Decimal::ONE);
        assert!(wallet.summary(1)["net_pnl"].is_null());
        wallet.reserved = Decimal::from(288);
        assert_eq!(
            simulate(Strategy::Point, &op, &mut wallet).unwrap()["reason"],
            "insufficient_virtual_funds"
        );
    }
}
