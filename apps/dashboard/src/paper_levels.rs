//! Pure conditional price policy. No network, clock, outcomes, or execution state.
//! Fee/grid/probability assumptions are explicit; no preview is an eligible order.
use crate::decision;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Deserialize;
use serde_json::{Value, json};

pub const VERSION: &str = "paper_buy_levels_v1";
const MAX_QUANTITY: u32 = 100;
fn d(n: i64, scale: u32) -> Decimal {
    Decimal::new(n, scale)
}
fn ceil(value: Decimal, scale: u32) -> Decimal {
    value.round_dp_with_strategy(scale, RoundingStrategy::ToPositiveInfinity)
}
fn floor(value: Decimal, scale: u32) -> Decimal {
    value.round_dp_with_strategy(scale, RoundingStrategy::ToNegativeInfinity)
}

pub fn policy() -> Value {
    json!({"version":VERSION,"purpose":"Conditional paper research; never an eligible order",
        "fees":{"model":"assumed general quadratic taker","coefficient":"0.07","multiplier":"1",
            "applicability":"unverified for the assigned contract at cutoff","schedule_reference":"https://kalshi.com/docs/kalshi-fee-schedule.pdf",
            "rounding_reference":"https://docs.kalshi.com/getting_started/fee_rounding",
            "modeled_account":"non-direct member; one hypothetical fill per displayed price level",
            "trade_fee_rounding_usd":"0.000001","balance_precision_usd":"0.01",
            "cap_stress":"Separate one-contract orders rounded to cents; no rebates. Larger of modeled level-fill fee and unit-order stress is used for depth comparison.",
            "settlement_fee_usd":"0","excluded":"FCM surcharges, exits, discounts and unknown overrides"},
        "tick_grid":{"step_usd":"0.01","min_usd":"0.01","max_usd":"0.99","applicability":"assumed; not captured market tick metadata"},
        "minimum_edge_usd_per_contract":"0.01","residual_execution_allowance_usd_per_contract":"0.005",
        "max_quantity_contracts":MAX_QUANTITY,"max_total_cost_usd":"100","hypothetical_intent_lifetime_ms":1000,
        "selection":"Full requested size only; highest conservative total net, YES on an exact tie; at most one side",
        "execution":"Frozen opening book only. A watch level is not a fill; no paper balance or orders are written."})
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    packet_hash: String,
    quantity: String,
    forecast: Forecast,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Forecast {
    MidpointBenchmark {},
    Hypothetical {
        p_yes: String,
        lower: String,
        upper: String,
    },
}
fn probability(s: &str) -> Result<Decimal, &'static str> {
    let (whole, fraction) = s.split_once('.').map_or((s, None), |(a, b)| (a, Some(b)));
    if !matches!(whole, "0" | "1")
        || fraction
            .is_some_and(|f| f.is_empty() || f.len() > 6 || !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("INVALID_FORECAST");
    }
    let p: Decimal = s.parse().map_err(|_| "INVALID_FORECAST")?;
    if !(Decimal::ZERO..=Decimal::ONE).contains(&p) || p.scale() > 6 {
        return Err("INVALID_FORECAST");
    }
    Ok(p)
}

/// Current documented accumulator, with explicit non-direct-member precision.
/// Buy revenue is negative. All supplied fills are hypothetical modeled fills.
#[derive(Default)]
struct Fees {
    rounding_accumulator: Decimal,
}
impl Fees {
    fn fill(&mut self, price: Decimal, quantity: Decimal) -> Decimal {
        let cost = price * quantity;
        self.apply_model_fee(cost, d(7, 2) * quantity * price * (Decimal::ONE - price))
    }
    fn apply_model_fee(&mut self, cost: Decimal, model: Decimal) -> Decimal {
        let trade = ceil(model, 6);
        let rounding = ceil(cost + trade, 2) - cost - trade;
        self.rounding_accumulator += rounding;
        let rebate = floor(self.rounding_accumulator, 2).min(floor(trade + rounding, 2));
        self.rounding_accumulator -= rebate;
        trade + rounding - rebate
    }
}
fn unit_stress(price: Decimal) -> Decimal {
    ceil(ceil(d(7, 2) * price * (Decimal::ONE - price), 6), 2)
}
fn cap(probability: Decimal, edge: Decimal, residual: Decimal) -> Option<Decimal> {
    (1..100)
        .rev()
        .map(|c| d(c, 2))
        .find(|price| probability - *price - unit_stress(*price) - residual >= edge)
}

fn side(
    packet: &Value,
    yes: bool,
    probability: Decimal,
    quantity: Decimal,
    edge: Decimal,
    residual: Decimal,
) -> Result<Value, &'static str> {
    let level = cap(probability, edge, residual);
    let source = if yes { "no_dollars" } else { "yes_dollars" };
    let bids: Vec<(Decimal, Decimal)> =
        serde_json::from_value(packet["book"][source].clone()).map_err(|_| "INVALID_BOOK")?;
    let mut fees = Fees::default();
    let mut remaining = quantity;
    let mut cost = Decimal::ZERO;
    let mut model_fee = Decimal::ZERO;
    let mut stress_fee = Decimal::ZERO;
    let mut ladder = Vec::new();
    let mut unsupported = false;
    let best_ask = bids.first().map(|(bid, _)| Decimal::ONE - *bid);
    for (bid, available) in bids {
        let ask = Decimal::ONE - bid;
        if ask <= Decimal::ZERO || ask >= Decimal::ONE || available <= Decimal::ZERO {
            return Err("INVALID_BOOK");
        }
        // Unsupported precision never gets rounded into invented eligible depth.
        if (ask * d(100, 0)).fract() != Decimal::ZERO || available.scale() > 2 {
            unsupported = true;
            break;
        }
        if !level.is_some_and(|limit| ask <= limit) {
            break;
        }
        let taken = available.min(remaining);
        let fee = fees.fill(ask, taken);
        let stress = unit_stress(ask) * taken;
        cost += ask * taken;
        model_fee += fee;
        stress_fee += stress;
        ladder.push(json!({"ask_usd":ask,"displayed_contracts":available,"modeled_contracts":taken,"modeled_fee_usd":fee}));
        remaining -= taken;
        if remaining.is_zero() {
            break;
        }
    }
    let covered = quantity - remaining;
    let reserved_fee = model_fee.max(stress_fee);
    let net = covered * probability - cost - reserved_fee - covered * residual;
    let complete = remaining.is_zero() && !unsupported;
    let meets = complete && net >= quantity * edge && cost + reserved_fee <= Decimal::from(100);
    let reason = if level.is_none() {
        "NO_POSITIVE_PRICE"
    } else if unsupported {
        "UNSUPPORTED_ASSUMED_GRID_OR_SIZE_PRECISION"
    } else if best_ask.is_some_and(|a| a > level.unwrap()) {
        "OFFER_ABOVE_CAP"
    } else if !complete {
        "INSUFFICIENT_DEPTH_UNDER_CAP"
    } else if cost + reserved_fee > Decimal::from(100) {
        "VIRTUAL_COST_LIMIT"
    } else if !meets {
        "NET_EDGE_BELOW_MINIMUM"
    } else {
        "CONDITIONAL_FULL_SIZE_MATCH"
    };
    Ok(
        json!({"conservative_probability_assumption":probability,"buy_up_to_usd":level,
        "stress_fee_at_cap_usd":level.map(|p|unit_stress(p)*quantity),
        "modeled_single_fill_fee_at_cap_usd":level.map(|p|Fees::default().fill(p,quantity)),
        "conservative_net_at_cap_usd":level.map(|p|quantity*(probability-p-unit_stress(p)-residual)),
        "best_ask_at_cutoff_usd":best_ask,"depth_under_cap":{"covered_contracts":covered,"uncovered_contracts":remaining,
            "complete":complete,"cost_before_fees_usd":cost,"modeled_level_fill_fee_usd":model_fee,"reserved_fee_usd":reserved_fee,
            "residual_allowance_usd":covered*residual,"conservative_net_usd":net,"ladder":ladder},
        "conditional_full_size_match":meets,"reason_code":reason,"eligible_buy_price_usd":null}),
    )
}

/// The host must validate the opening packet before this pure calculation.
pub fn preview(packet: &Value, args: Value) -> Result<Value, &'static str> {
    let q: Request = serde_json::from_value(args).map_err(|_| "ASOF_DENIED")?;
    if q.packet_hash != decision::hash(packet) {
        return Err("ASOF_DENIED");
    }
    if q.quantity.is_empty()
        || q.quantity.len() > 3
        || !q.quantity.bytes().all(|b| b.is_ascii_digit())
    {
        return Err("INVALID_QUANTITY");
    }
    let quantity: u32 = q.quantity.parse().map_err(|_| "INVALID_QUANTITY")?;
    if quantity == 0 || quantity > MAX_QUANTITY {
        return Err("INVALID_QUANTITY");
    }
    let (basis, p, lower, upper) = match q.forecast {
        Forecast::MidpointBenchmark {} => {
            let p: Decimal = serde_json::from_value(packet["yes_midpoint_usd"].clone())
                .map_err(|_| "INVALID_FORECAST")?;
            ("midpoint_benchmark", p, p, p)
        }
        Forecast::Hypothetical {
            p_yes,
            lower,
            upper,
        } => (
            "hypothetical",
            probability(&p_yes)?,
            probability(&lower)?,
            probability(&upper)?,
        ),
    };
    if lower > p || p > upper || p < Decimal::ZERO || p > Decimal::ONE {
        return Err("INVALID_FORECAST");
    }
    let quantity = Decimal::from(quantity);
    let edge = d(1, 2);
    let residual = d(5, 3);
    let yes = side(packet, true, lower, quantity, edge, residual)?;
    let no = side(
        packet,
        false,
        Decimal::ONE - upper,
        quantity,
        edge,
        residual,
    )?;
    let selection = match (
        yes["conditional_full_size_match"] == true,
        no["conditional_full_size_match"] == true,
    ) {
        (true, true) => {
            let y: Decimal =
                serde_json::from_value(yes["depth_under_cap"]["conservative_net_usd"].clone())
                    .map_err(|_| "INVALID_BOOK")?;
            let n: Decimal =
                serde_json::from_value(no["depth_under_cap"]["conservative_net_usd"].clone())
                    .map_err(|_| "INVALID_BOOK")?;
            Some(if y >= n { "YES" } else { "NO" })
        }
        (true, false) => Some("YES"),
        (false, true) => Some("NO"),
        (false, false) => None,
    };
    let policy = policy();
    let policy_hash = decision::hash(&policy);
    let scenario = json!({"basis":basis,"p_yes":p,"lower":lower,"upper":upper,"quantity":quantity});
    let scenario_hash = decision::hash(
        &json!({"packet_hash":q.packet_hash,"policy_hash":policy_hash,"scenario":scenario}),
    );
    let cutoff = decision::time(&packet["cutoff"])?;
    let open = packet["open_ts"].as_i64().ok_or("ASOF_DENIED")?;
    let expiry = (cutoff + chrono::Duration::seconds(1))
        .min(chrono::DateTime::from_timestamp(open + 40, 0).ok_or("ASOF_DENIED")?);
    Ok(
        json!({"status":"conditional_paper_preview","policy_version":VERSION,"policy_hash":policy_hash,"scenario_hash":scenario_hash,
        "policy":policy,"forecast_assumption":scenario,"hypothetical_intent_expires_at":expiry,
        "levels":{"YES":yes,"NO":no},"hypothetical_selected_side":selection,
        "paper_order_eligible":false,"eligibility_reason_codes":["UNVERIFIED_ASOF_FEE_RULES","UNVERIFIED_ASOF_TICK_GRID","UNCALIBRATED_PROBABILITY_BOUNDS"],
        "interpretation":"Conditional math under the disclosed assumptions, not a committed forecast, recommendation, order or fill. Assumptions are not historical observations.",
        "source_visibility":"Only the assigned opening packet at its original cutoff; no later books or outcomes"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assets::Asset,
        checkpoint::Phase,
        decision::tests::{at, fixture, open},
    };
    fn packet(asset: Asset) -> Value {
        decision::packet(
            &fixture(asset, Phase::Opening, 5, 6, 7),
            asset,
            open(),
            at(8),
            Phase::Opening,
        )
        .unwrap()
    }
    fn args(p: &Value) -> Value {
        json!({"packet_hash":decision::hash(p),"quantity":"1","forecast":{"kind":"hypothetical","p_yes":"0.60","lower":"0.56","upper":"0.64"}})
    }
    fn number(v: &Value) -> Decimal {
        serde_json::from_value(v.clone()).unwrap()
    }
    #[test]
    fn official_rounding_example_and_accumulator_rebates_are_exact() {
        let mut fees = Fees::default();
        assert_eq!(fees.apply_model_fee(d(55, 3), d(363825, 8)), d(5, 3));
        assert_eq!(fees.rounding_accumulator, d(1361, 6));
        let mut fees = Fees::default();
        let mut total = Decimal::ZERO;
        for _ in 0..4 {
            total += fees.fill(d(55, 2), d(1, 1));
        }
        assert_eq!(total, d(20, 3));
        let final_fee = fees.fill(d(50, 2), Decimal::ONE);
        assert_eq!(final_fee, d(1, 2));
        assert!(fees.rounding_accumulator < d(1, 2));
        assert_eq!(Fees::default().fill(d(50, 2), Decimal::ONE), d(2, 2));
        assert_eq!(
            Fees::default().fill(d(50, 2), Decimal::from(100)),
            d(175, 2)
        );
    }
    #[test]
    fn synthetic_acceptance_and_default_execution_allowance() {
        // Earlier plan's 53c/33c fixture has zero residual allowance.
        assert_eq!(cap(d(56, 2), d(1, 2), Decimal::ZERO), Some(d(53, 2)));
        assert_eq!(cap(d(36, 2), d(1, 2), Decimal::ZERO), Some(d(33, 2)));
        for asset in Asset::ALL {
            let p = packet(asset);
            let v = preview(&p, args(&p)).unwrap();
            assert_eq!(number(&v["levels"]["YES"]["buy_up_to_usd"]), d(52, 2));
            assert_eq!(number(&v["levels"]["NO"]["buy_up_to_usd"]), d(32, 2));
            assert_eq!(v["paper_order_eligible"], false);
            assert!(v["levels"]["YES"]["eligible_buy_price_usd"].is_null());
            assert_eq!(v["hypothetical_selected_side"], "YES");
        }
    }
    #[test]
    fn exhaustive_integer_oracle_checks_highest_qualifying_tick_and_monotonicity() {
        let mut last = None;
        for probability_milli in 0..=1000 {
            let p = d(probability_milli, 3);
            // Independent integer-microdollar oracle; no Decimal fee helper.
            let expected = (1i64..100)
                .filter(|c| {
                    let raw = 7 * c * (100 - c); // raw fee in millionths of a dollar
                    let stress = ((raw + 9999) / 10000) * 10000;
                    probability_milli * 1000 - c * 10000 - stress - 5000 >= 10000
                })
                .max()
                .map(|c| d(c, 2));
            let found = cap(p, d(1, 2), d(5, 3));
            assert_eq!(found, expected);
            if let (Some(before), Some(after)) = (last, found) {
                assert!(after >= before);
            }
            last = found;
        }
    }
    #[test]
    fn complements_partial_depth_and_no_offer_do_not_invent_fills() {
        let p = packet(Asset::Btc);
        let mut q = args(&p);
        q["quantity"] = json!("10");
        let v = preview(&p, q).unwrap();
        assert_eq!(
            v["levels"]["YES"]["reason_code"],
            "INSUFFICIENT_DEPTH_UNDER_CAP"
        );
        assert_eq!(
            number(&v["levels"]["YES"]["depth_under_cap"]["covered_contracts"]),
            Decimal::from(7)
        );
        assert_eq!(v["levels"]["NO"]["reason_code"], "OFFER_ABOVE_CAP");
        assert!(v["hypothetical_selected_side"].is_null());
        let mut q = args(&p);
        q["forecast"] = json!({"kind":"midpoint_benchmark"});
        let v = preview(&p, q).unwrap();
        assert!(v["hypothetical_selected_side"].is_null());
        assert_eq!(
            number(&v["forecast_assumption"]["p_yes"]),
            number(&p["yes_midpoint_usd"])
        );
        assert_eq!(
            number(&v["levels"]["NO"]["conservative_probability_assumption"]),
            Decimal::ONE - number(&p["yes_midpoint_usd"])
        );
    }
    #[test]
    fn unverified_precision_is_preserved_as_unsupported() {
        let mut p = packet(Asset::Eth);
        p["book"]["no_dollars"] = json!([["0.555", "12"]]);
        let v = preview(&p, args(&p)).unwrap();
        assert_eq!(
            v["levels"]["YES"]["reason_code"],
            "UNSUPPORTED_ASSUMED_GRID_OR_SIZE_PRECISION"
        );
        assert_eq!(
            number(&v["levels"]["YES"]["best_ask_at_cutoff_usd"]),
            d(445, 3)
        );
        assert!(v["hypothetical_selected_side"].is_null());
    }
    #[test]
    fn invalid_forecasts_overrides_quantities_and_hashes_fail_closed() {
        let p = packet(Asset::Sol);
        for quantity in ["0", "101", "1.5", "-1", "9999999999999999999", "NaN"] {
            let mut q = args(&p);
            q["quantity"] = json!(quantity);
            assert_eq!(preview(&p, q), Err("INVALID_QUANTITY"));
        }
        for (key, value) in [
            ("asset", json!("BTC")),
            ("cutoff", json!(at(45))),
            ("phase", json!("execution_candidate")),
            ("fee_multiplier", json!(0)),
            ("policy_version", json!("free")),
        ] {
            let mut q = args(&p);
            q[key] = value;
            assert_eq!(preview(&p, q), Err("ASOF_DENIED"));
        }
        for lower in ["NaN", "1.1", "-0.1", "1e-1", "0.0000001", "0.70"] {
            let mut q = args(&p);
            q["forecast"]["lower"] = json!(lower);
            assert_eq!(preview(&p, q), Err("INVALID_FORECAST"));
        }
        let mut q = args(&p);
        q["forecast"]["calibrated"] = json!(true);
        assert_eq!(preview(&p, q), Err("ASOF_DENIED"));
        let mut q = args(&p);
        q["forecast"] = json!({"kind":"midpoint_benchmark","p_yes":"1"});
        assert_eq!(preview(&p, q), Err("ASOF_DENIED"));
        let mut q = args(&p);
        q["packet_hash"] = json!("0".repeat(64));
        assert_eq!(preview(&p, q), Err("ASOF_DENIED"));
    }
    #[test]
    fn later_snapshots_and_labels_cannot_change_cutoff_levels_or_hashes() {
        for asset in Asset::ALL {
            let doc = fixture(asset, Phase::Opening, 5, 6, 7);
            let p = decision::packet(&doc, asset, open(), at(8), Phase::Opening).unwrap();
            let original = preview(&p, args(&p)).unwrap();
            let mut changed = doc.clone();
            let mut row: Value = serde_json::from_str(
                changed["fields"]["payload"]["stringValue"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            row["outcome"] = json!("yes");
            row["observation"]["expiration_value"] = json!("999999");
            row["later_book"] = json!({"yes_bids":[["0.99","1000"]]});
            changed["fields"]["payload"]["stringValue"] = json!(row.to_string());
            let frozen = decision::packet(&changed, asset, open(), at(8), Phase::Opening).unwrap();
            assert_eq!(original, preview(&frozen, args(&frozen)).unwrap());
            changed["createTime"] = json!(at(9));
            changed["updateTime"] = json!(at(9));
            assert_eq!(
                decision::packet(&changed, asset, open(), at(8), Phase::Opening),
                Err("not_yet_available")
            );
            let later = fixture(asset, Phase::ExecutionCandidate, 45, 46, 47);
            assert!(decision::packet(&later, asset, open(), at(8), Phase::Opening).is_err());
        }
    }
    #[test]
    fn endpoints_zero_one_bounds_and_maximum_size_are_bounded() {
        let p = packet(Asset::Btc);
        let mut q = args(&p);
        q["quantity"] = json!("100");
        q["forecast"] = json!({"kind":"hypothetical","p_yes":"1","lower":"1","upper":"1"});
        let v = preview(&p, q).unwrap();
        assert!(v["levels"]["NO"]["buy_up_to_usd"].is_null());
        assert!(number(&v["levels"]["YES"]["buy_up_to_usd"]) < Decimal::ONE);
        assert!(serde_json::to_vec(&v).unwrap().len() < 32768);
        let mut q = args(&p);
        q["forecast"] = json!({"kind":"hypothetical","p_yes":"0.5","lower":"0","upper":"1"});
        let v = preview(&p, q).unwrap();
        assert!(v["levels"]["YES"]["buy_up_to_usd"].is_null());
        assert!(v["levels"]["NO"]["buy_up_to_usd"].is_null());
    }
}
