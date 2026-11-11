//! Label-bearing evaluator, used only by the trusted operator/collector.
//! Forecast extraction has no label argument; this module never enters MCP.
use crate::{assets::Asset, decision, forecast};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub const STRATEGIES: [&str; 5] = [
    "constant_half",
    "historical_yes_rate",
    "market_midpoint",
    "logistic_raw",
    "logistic_calibrated",
];
pub const ROOT: &str = "prospective";
fn at(s: &str) -> DateTime<Utc> {
    decision::time(&json!(s)).expect("fixed protocol timestamp")
}
pub fn day_start(day: NaiveDate) -> i64 {
    day.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp()
}
pub fn asset_index(asset: Asset) -> usize {
    match asset {
        Asset::Btc => 0,
        Asset::Eth => 1,
        Asset::Sol => 2,
    }
}
pub fn symbol(asset: Asset) -> String {
    serde_json::to_value(asset)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}
pub fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u8,
    pub protocol: String,
    pub model_object: String,
    pub model_generation: String,
    pub model_committed_at: DateTime<Utc>,
    pub engineering_start: DateTime<Utc>,
    pub confirmatory_start: DateTime<Utc>,
    pub strategies: Vec<String>,
    pub primary_contrast: String,
    pub log_loss_clip_millionths: u32,
    pub minimum_complete_days: usize,
    pub minimum_coverage_basis_points: usize,
    pub bootstrap_replicates: usize,
    pub bootstrap_seed: u64,
}
impl Manifest {
    pub fn v1() -> Self {
        Self {
            schema_version: 1,
            protocol: "prospective_opening_eval_v1".into(),
            model_object: forecast::object_name(
                "661dc54e09ee9f3665a5a5863c9de53247aa34e92502009367b514c4737a7bd5",
            ),
            model_generation: "1794412357049817".into(),
            model_committed_at: at("2026-11-11T15:52:37.057Z"),
            engineering_start: at("2026-11-11T16:15:00Z"),
            confirmatory_start: at("2026-11-12T00:00:00Z"),
            strategies: STRATEGIES.iter().map(|s| s.to_string()).collect(),
            primary_contrast: "brier_calibrated_minus_midpoint".into(),
            log_loss_clip_millionths: 1,
            minimum_complete_days: 30,
            minimum_coverage_basis_points: 9500,
            bootstrap_replicates: 2000,
            bootstrap_seed: 17,
        }
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        let baseline = Self::v1();
        let mut protocol = self.clone();
        protocol.model_object = baseline.model_object.clone();
        protocol.model_generation = baseline.model_generation.clone();
        protocol.model_committed_at = baseline.model_committed_at;
        if protocol != baseline
            || !forecast::valid_object(&self.model_object)
            || !digits(&self.model_generation)
            || self.model_committed_at > self.engineering_start
        {
            return Err("invalid_experiment_protocol");
        }
        Ok(())
    }
    pub fn id(&self) -> String {
        decision::hash(&serde_json::to_value(self).unwrap())
    }
    pub fn object(&self) -> String {
        format!("{ROOT}/experiments/v1/{}.json", self.id())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ObjectRef {
    pub name: String,
    pub generation: String,
    pub committed_at: DateTime<Utc>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Registered {
    pub manifest: Manifest,
    pub object: ObjectRef,
}
impl Registered {
    pub fn validate(&self, asof: DateTime<Utc>) -> Result<(), &'static str> {
        self.manifest.validate()?;
        if self.object.name != self.manifest.object()
            || !digits(&self.object.generation)
            || self.object.committed_at > asof
            || self.object.committed_at >= self.manifest.confirmatory_start
            || self.object.committed_at < self.manifest.model_committed_at
        {
            return Err("invalid_experiment_registration");
        }
        Ok(())
    }
}
fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit())
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    pub object: ObjectRef,
    pub record: Value,
}
/// Only these original decision fields cross into the evaluator. Rule proofs,
/// review records, later links and other provider fields are deliberately dropped.
pub fn decision_projection(record: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for key in [
        "schema_version",
        "run_id",
        "strategy",
        "asset",
        "open_ts",
        "cutoff",
        "produced_at",
        "status",
        "reason",
        "packet",
        "packet_hash",
        "p_yes",
        "forecast_hash",
        "forecast_proof",
        "forecast_reason",
        "runtime_revision",
    ] {
        if let Some(value) = record.get(key) {
            result.insert(key.into(), value.clone());
        }
    }
    Value::Object(result)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Label {
    pub asset: Asset,
    pub ticker: String,
    pub open_ts: i64,
    pub close_ts: i64,
    pub result: String,
    pub source_id: String,
    pub source_version: String,
    pub captured_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
}
impl Label {
    pub fn id(&self) -> String {
        decision::hash(&serde_json::to_value(self).unwrap())
    }
    fn validate(&self) -> Result<(), &'static str> {
        let version = decision::time(&json!(self.source_version))?;
        if self.open_ts.rem_euclid(900) != 0
            || self.close_ts != self.open_ts + 900
            || version.timestamp() < self.close_ts
            || self.available_at != version.max(self.captured_at)
            || self.source_id
                != format!(
                    "projects/gitunderstand/databases/crypto-prediction-lab/documents/contracts_{}/{}",
                    symbol(self.asset).to_ascii_lowercase(),
                    self.ticker
                )
            || !crate::outcomes::valid_ticker(&self.ticker, self.asset)
            || self.result.len() > 32
        {
            return Err("invalid_official_label");
        }
        Ok(())
    }
}
pub fn label_projection(doc: &Value, asset: Asset, day: NaiveDate) -> Result<Label, &'static str> {
    let raw = doc
        .pointer("/fields/summary/stringValue")
        .and_then(Value::as_str)
        .ok_or("missing_summary")?;
    let s: Value = serde_json::from_str(raw).map_err(|_| "invalid_summary")?;
    let source_version = doc["updateTime"]
        .as_str()
        .ok_or("missing_label_version")?
        .to_owned();
    let updated = decision::time(&doc["updateTime"])?;
    let captured_at = decision::time(&s["captured_at"])?;
    let label = Label {
        asset,
        ticker: s["ticker"].as_str().ok_or("invalid_ticker")?.into(),
        open_ts: decision::time(&s["open_time"])?.timestamp(),
        close_ts: decision::time(&s["close_time"])?.timestamp(),
        result: s["result"].as_str().ok_or("missing_result")?.into(),
        source_id: doc["name"].as_str().ok_or("missing_source")?.into(),
        source_version,
        captured_at,
        available_at: updated.max(captured_at),
    };
    label.validate()?;
    if s["asset"] != serde_json::to_value(asset).unwrap()
        || label.open_ts < day_start(day)
        || label.open_ts >= day_start(day) + 86400
        || decision::time(&doc["fields"]["close_time"]["timestampValue"])?.timestamp()
            != label.close_ts
        || decision::time(&doc["createTime"])? > updated
    {
        return Err("label_identity");
    }
    Ok(label)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub schema_version: u8,
    pub experiment: Registered,
    pub day: NaiveDate,
    pub asof: DateTime<Utc>,
    pub decisions: Vec<Saved>,
    pub labels: Vec<Label>,
    pub previous_asof: Option<DateTime<Utc>>,
    pub previous_report: Option<ObjectRef>,
    pub previous_labels: Vec<Label>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub asset: Asset,
    pub open_ts: i64,
    pub forecast_status: String,
    pub probabilities: Option<[f64; 5]>,
    pub decision: Option<ObjectRef>,
    pub label_id: Option<String>,
    pub official_result: Option<String>,
    pub status: String,
}
/// Reads only one saved decision plus immutable experiment metadata. No labels,
/// evaluator scores, candidate books, mutable clock or training interface.
fn probabilities(
    saved: &Saved,
    asset: Asset,
    open: i64,
    m: &Manifest,
) -> Result<[f64; 5], &'static str> {
    let r = &saved.record;
    if saved.object.name != format!("decisions/v1/{}.json", decision::run_id(asset, open))
        || !digits(&saved.object.generation)
        || r["schema_version"] != 1
        || r["run_id"] != decision::run_id(asset, open)
        || r["strategy"] != decision::STRATEGY
        || r["asset"] != serde_json::to_value(asset).unwrap()
        || r["open_ts"] != open
    {
        return Err("decision_identity");
    }
    let cutoff = decision::time(&r["cutoff"])?;
    let produced = decision::time(&r["produced_at"])?;
    if cutoff > produced
        || produced > saved.object.committed_at
        || cutoff.timestamp() < open
        || saved.object.committed_at.timestamp() >= open + 40
    {
        return Err("late_or_invalid_decision");
    }
    if r["status"] != "forecast" {
        return Err("decision_abstained");
    }
    let p = &r["packet"];
    decision::validate_projection(p)?;
    if p["cutoff"] != r["cutoff"]
        || p["asset"] != r["asset"]
        || p["open_ts"] != open
        || r["packet_hash"] != decision::hash(p)
        || r["p_yes"] != p["yes_midpoint_usd"]
    {
        return Err("packet_integrity");
    }
    if r["forecast_proof"].is_null() {
        return Err("no_committed_forecast");
    }
    if r["forecast_hash"] != decision::hash(&r["forecast_proof"]) {
        return Err("forecast_integrity");
    }
    let proof: forecast::Frozen =
        serde_json::from_value(r["forecast_proof"].clone()).map_err(|_| "invalid_forecast")?;
    let f = proof.response(p)?;
    if f["model_id"] != m.model_object
        || f["model_generation"] != m.model_generation
        || decision::time(&f["model_committed_at"])? != m.model_committed_at
    {
        return Err("different_model_version");
    }
    let number = |v: &Value| -> Result<f64, &'static str> {
        let x = v
            .as_f64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            .ok_or("invalid_probability")?;
        if x.is_finite() && (0. ..=1.).contains(&x) {
            Ok(x)
        } else {
            Err("invalid_probability")
        }
    };
    Ok([
        0.5,
        number(&f["baselines"]["historical_yes_rate"])?,
        number(&p["yes_midpoint_usd"])?,
        number(&f["p_yes_raw"])?,
        number(&f["p_yes_calibrated"])?,
    ])
}
/// Normalize visibility BEFORE hashing: adding a future result/version or object
/// cannot alter an older report's scores, reason codes or input identity.
pub fn visible(mut input: Input) -> Result<Input, &'static str> {
    input.experiment.validate(input.asof)?;
    if input.schema_version != 1
        || input.decisions.len() > 576
        || input.labels.len() > 1152
        || input.previous_labels.len() > 576
        || input.previous_asof.is_some_and(|t| t > input.asof)
        || input
            .previous_report
            .as_ref()
            .is_some_and(|r| r.committed_at > input.asof)
        || input.previous_report.is_some() != input.previous_asof.is_some()
    {
        return Err("invalid_evaluation_input");
    }
    if let Some(previous) = &input.previous_report {
        let prefix = format!(
            "prospective/reports/v1/{}/days/{}/",
            input.experiment.manifest.id(),
            input.day
        );
        if !previous
            .name
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(".json"))
            .is_some_and(valid_hash)
            || !digits(&previous.generation)
            || input
                .previous_asof
                .is_some_and(|t| t > previous.committed_at)
        {
            return Err("invalid_previous_report");
        }
    }
    let left = day_start(input.day);
    let right = left + 86400;
    input
        .decisions
        .retain(|s| s.object.committed_at <= input.asof);
    input
        .decisions
        .sort_by(|a, b| a.object.name.cmp(&b.object.name));
    let mut names = BTreeSet::new();
    for d in &input.decisions {
        if !names.insert(&d.object.name) || d.record != decision_projection(&d.record) {
            return Err("duplicate_or_unprojected_decision");
        }
    }
    for labels in [&mut input.labels, &mut input.previous_labels] {
        labels.retain(|l| l.available_at <= input.asof);
        for l in labels.iter() {
            l.validate()?;
            if l.open_ts < left || l.open_ts >= right {
                return Err("label_outside_day");
            }
        }
        labels.sort_by_key(|l| {
            (
                l.open_ts,
                asset_index(l.asset),
                l.available_at,
                l.source_version.clone(),
                l.id(),
            )
        });
        labels.dedup();
    }
    if input.previous_asof.is_none() && !input.previous_labels.is_empty() {
        return Err("invalid_previous_labels");
    }
    if let Some(asof) = input.previous_asof {
        input.previous_labels.retain(|l| l.available_at <= asof);
    }
    Ok(input)
}
fn latest<'a>(
    labels: impl Iterator<Item = &'a Label>,
    asset: Asset,
    open: i64,
) -> Result<Option<&'a Label>, &'static str> {
    let mut candidates: Vec<_> = labels
        .filter(|l| l.asset == asset && l.open_ts == open)
        .collect();
    candidates.sort_by_key(|l| {
        (
            l.available_at,
            decision::time(&json!(l.source_version)).unwrap(),
        )
    });
    let Some(last) = candidates.last().copied() else {
        return Ok(None);
    };
    if candidates.iter().any(|l| {
        l.available_at == last.available_at && l.source_version == last.source_version && *l != last
    }) {
        return Err("ambiguous_official_result");
    }
    Ok(Some(last))
}
fn scores(rows: &[&Row]) -> Value {
    let paired: Vec<_> = rows.iter().filter(|r| r.status == "scored").collect();
    let n = paired.len();
    let mut strategies = serde_json::Map::new();
    for (j, name) in STRATEGIES.iter().enumerate() {
        let ps: Vec<_> = paired
            .iter()
            .map(|r| {
                (
                    r.probabilities.unwrap()[j],
                    f64::from(r.official_result.as_deref() == Some("yes")),
                )
            })
            .collect();
        let mut metric = forecast::metrics(&ps);
        metric["accuracy_at_half_ties_yes"] = if n > 0 {
            json!(
                ps.iter()
                    .filter(|(p, y)| f64::from(*p >= 0.5) == *y)
                    .count() as f64
                    / n as f64
            )
        } else {
            Value::Null
        };
        strategies.insert((*name).into(), metric);
    }
    let delta_sum: f64 = paired
        .iter()
        .map(|r| {
            let p = r.probabilities.unwrap();
            let y = f64::from(r.official_result.as_deref() == Some("yes"));
            (p[4] - y).powi(2) - (p[2] - y).powi(2)
        })
        .sum();
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for r in rows {
        *reasons.entry(r.status.clone()).or_default() += 1;
    }
    let timely = rows.iter().filter(|r| r.probabilities.is_some()).count();
    let expected = rows.len();
    let missing = expected - n;
    json!({"expected":expected,"timely_forecasts":timely,"scored":n,"unscored":missing,"reason_counts":reasons,
        "forecast_coverage":if expected>0 {Some(timely as f64/expected as f64)}else{None},"scored_coverage":if expected>0{Some(n as f64/expected as f64)}else{None},
        "strategies":strategies,"paired_brier_delta_sum":delta_sum,"paired_brier_delta_calibrated_minus_midpoint":if n>0{Some(delta_sum/n as f64)}else{None},
        "full_scheduled_delta_bounds":if expected>0{json!({"lower":(delta_sum-missing as f64)/expected as f64,"upper":(delta_sum+missing as f64)/expected as f64,"assumption":"Unscored opportunities have unknown paired loss in [-1,1]; never imputed as zero"})}else{Value::Null}})
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DayStats {
    pub day: NaiveDate,
    pub asof: DateTime<Utc>,
    pub phase: String,
    pub full_utc_day: bool,
    pub expected: usize,
    pub timely: [usize; 3],
    pub scored: [usize; 3],
    pub paired_delta_sum: f64,
}
pub fn evaluate(input: Input) -> Result<(Input, Value, Vec<Row>), &'static str> {
    let input = visible(input)?;
    let m = &input.experiment.manifest;
    let left = day_start(input.day).max(m.engineering_start.timestamp());
    let right = (day_start(input.day) + 86400).min(input.asof.timestamp().div_euclid(900) * 900);
    if right < left || left >= day_start(input.day) + 86400 {
        return Err("day_not_available");
    }
    let mut expected_names = BTreeSet::new();
    let mut rows = Vec::new();
    let mut corrections = Vec::new();
    let mut revisions = 0;
    for open in (left..right).step_by(900) {
        for asset in Asset::ALL {
            let name = format!("decisions/v1/{}.json", decision::run_id(asset, open));
            expected_names.insert(name.clone());
            let saved = input.decisions.iter().find(|d| d.object.name == name);
            let prediction = saved
                .ok_or("missing_decision")
                .and_then(|s| probabilities(s, asset, open, m));
            let forecast_status = prediction
                .as_ref()
                .map(|_| "available")
                .unwrap_or_else(|e| *e)
                .to_string();
            // Earlier report versions are audit evidence, not a substitute for
            // a currently unavailable official result.
            let label = latest(input.labels.iter(), asset, open)?;
            let previous = latest(input.previous_labels.iter(), asset, open)?;
            if let (Some(old), Some(new)) = (previous, label)
                && old.id() != new.id()
            {
                revisions += 1;
                if old.result != new.result {
                    corrections.push(json!({"asset":asset,"open_ts":open,"previous_label_id":old.id(),"current_label_id":new.id()}));
                }
            }
            let result = label.map(|l| l.result.clone());
            let status = match (&prediction, label) {
                (Err(e), _) => e.to_string(),
                (Ok(_), None) => "official_result_unavailable".into(),
                (Ok(_), Some(l)) if l.result != "yes" && l.result != "no" => {
                    "non_binary_official_result".into()
                }
                (Ok(_), Some(l)) if saved.unwrap().record["packet"]["ticker"] != l.ticker => {
                    "label_ticker_mismatch".into()
                }
                _ => "scored".into(),
            };
            rows.push(Row {
                asset,
                open_ts: open,
                forecast_status,
                probabilities: prediction.ok(),
                decision: saved.map(|s| s.object.clone()),
                label_id: label.map(Label::id),
                official_result: result,
                status,
            });
        }
    }
    if input
        .decisions
        .iter()
        .any(|d| !expected_names.contains(&d.object.name))
    {
        return Err("decision_outside_completed_range");
    }
    let all: Vec<_> = rows.iter().collect();
    let mut evaluation = serde_json::Map::new();
    evaluation.insert("pooled".into(), scores(&all));
    let mut timely = [0; 3];
    let mut scored = [0; 3];
    for asset in Asset::ALL {
        let selected: Vec<_> = rows.iter().filter(|r| r.asset == asset).collect();
        let s = scores(&selected);
        timely[asset_index(asset)] = s["timely_forecasts"].as_u64().unwrap() as usize;
        scored[asset_index(asset)] = s["scored"].as_u64().unwrap() as usize;
        evaluation.insert(symbol(asset), s);
    }
    let phase = if day_start(input.day) >= m.confirmatory_start.timestamp() {
        "confirmatory"
    } else {
        "engineering"
    };
    let full = left == day_start(input.day) && right == day_start(input.day) + 86400;
    let stats = DayStats {
        day: input.day,
        asof: input.asof,
        phase: phase.into(),
        full_utc_day: full,
        expected: rows.len(),
        timely,
        scored,
        paired_delta_sum: evaluation["pooled"]["paired_brier_delta_sum"]
            .as_f64()
            .unwrap(),
    };
    let forecast_identity:Vec<_>=rows.iter().map(|r|json!({"asset":r.asset,"open_ts":r.open_ts,"status":r.forecast_status,"probabilities":r.probabilities,"decision":r.decision})).collect();
    let report = json!({"schema_version":1,"protocol":m.protocol,"experiment_id":m.id(),"experiment_registration":input.experiment.object,"model_object":m.model_object,"asof":input.asof,"day":input.day,"start_ts":left,"end_ts_exclusive":right,"phase":phase,"full_utc_day":full,
        "input_hash":decision::hash(&serde_json::to_value(&input).unwrap()),"rows_hash":decision::hash(&serde_json::to_value(&rows).unwrap()),"forecast_set_hash":decision::hash(&json!(forecast_identity)),
        "previous_report":input.previous_report,"source_version_revisions":revisions,"official_outcome_corrections":corrections.len(),"corrections_hash":decision::hash(&json!(corrections)),
        "evaluation":evaluation,"stats":stats,"status":"insufficient_evidence","probability_interval":null,"paper_order_eligible":false,
        "interpretation":"Scores evaluate saved committed forecasts; no fitting, retrospective forecast generation, fills or profitability claim",
        "limitations":["Current mutable official versions are available no sooner than durable update and capture","Only versions observed by evaluations are preserved; overwritten earlier versions cannot be reconstructed","Partial/engineering days are excluded from confirmatory day-block uncertainty","Unknown or skipped rows remain in coverage and adverse bounds","No actionable calibration bounds or trading eligibility"]});
    Ok((input, report, rows))
}
/// Day blocks preserve aligned assets/cycles. This diagnostic does not feed
/// calibration, price policies or blinded agents and cannot promote an order.
pub fn summarize(
    reg: &Registered,
    asof: DateTime<Utc>,
    from: NaiveDate,
    through: NaiveDate,
    stats: &[DayStats],
) -> Result<Value, &'static str> {
    reg.validate(asof)?;
    if through < from || (through - from).num_days() > 30 || through > asof.date_naive() {
        return Err("invalid_summary_range");
    }
    let mut seen = BTreeSet::new();
    for s in stats {
        if s.day < from
            || s.day > through
            || s.asof > asof
            || !seen.insert(s.day)
            || s.expected > 288
            || (s.full_utc_day
                && (s.expected != 288 || s.asof.timestamp() < day_start(s.day) + 86400))
            || (s.phase == "confirmatory"
                && day_start(s.day) < reg.manifest.confirmatory_start.timestamp())
            || s.scored.iter().zip(s.timely).any(|(n, t)| *n > t)
            || s.timely.iter().any(|n| *n > 96)
            || !s.paired_delta_sum.is_finite()
            || s.paired_delta_sum.abs() > s.scored.iter().sum::<usize>() as f64
        {
            return Err("invalid_day_stats");
        }
    }
    let eligible: Vec<_> = stats
        .iter()
        .filter(|s| {
            s.phase == "confirmatory"
                && s.full_utc_day
                && s.expected == 288
                && day_start(s.day) >= reg.manifest.confirmatory_start.timestamp()
                && day_start(s.day) + 86400 <= asof.timestamp()
                && s.timely
                    .iter()
                    .chain(s.scored.iter())
                    .all(|n| n * 10000 >= 96 * reg.manifest.minimum_coverage_basis_points)
        })
        .collect();
    let mut interval = Value::Null;
    if eligible.len() >= reg.manifest.minimum_complete_days {
        let mut state = reg.manifest.bootstrap_seed;
        let mut samples = Vec::new();
        for _ in 0..reg.manifest.bootstrap_replicates {
            let mut sum = 0.;
            let mut n = 0;
            for _ in 0..eligible.len() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let s = eligible[(state as usize) % eligible.len()];
                sum += s.paired_delta_sum;
                n += s.scored.iter().sum::<usize>();
            }
            samples.push(sum / n as f64);
        }
        samples.sort_by(f64::total_cmp);
        let len = samples.len();
        interval = json!({"lower":samples[(len-1)*25/1000],"upper":samples[(len-1)*975/1000],"level":0.95,"replicates":len,"method":"paired UTC-day block percentile bootstrap; all assets in each day stay together","assumption":"Day blocks treated as exchangeable; longer serial dependence requires a separately frozen evaluation"});
    }
    let sum: f64 = eligible.iter().map(|s| s.paired_delta_sum).sum();
    let n: usize = eligible
        .iter()
        .map(|s| s.scored.iter().sum::<usize>())
        .sum();
    Ok(
        json!({"schema_version":1,"protocol":reg.manifest.protocol,"experiment_id":reg.manifest.id(),"asof":asof,"from_day":from,"through_day":through,"calendar_days":(through-from).num_days()+1,"reported_days":stats.len(),"missing_daily_reports":(through-from).num_days()+1-stats.len() as i64,"eligible_complete_days":eligible.len(),"minimum_complete_days":reg.manifest.minimum_complete_days,"paired_rows":n,"paired_brier_delta_calibrated_minus_midpoint":if n>0{Some(sum/n as f64)}else{None},"day_block_interval":interval,"status":if eligible.len()>=reg.manifest.minimum_complete_days{"diagnostic_review_only"}else{"insufficient_evidence"},"paper_order_eligible":false,"calibration_bounds_available":false,"warning":"No model fitting or market-edge certification; missing/partial/low-coverage days and engineering data cannot qualify"}),
    )
}
pub fn cli() -> Result<(), Box<dyn std::error::Error>> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let role = std::env::var("DASHBOARD_ROLE")?;
    let output = std::env::var("EVALUATION_OUTPUT")?;
    let value = if role == "evaluation-manifest" {
        serde_json::to_value(Manifest::v1())?
    } else {
        let path = std::env::var("EVALUATION_INPUT")?;
        if std::fs::metadata(&path)?.len() > 16 * 1024 * 1024 {
            return Err("Evaluation input too large".into());
        }
        let input: Input = serde_json::from_slice(&std::fs::read(path)?)?;
        let (_, report, _) = evaluate(input)?;
        report
    };
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(output)?;
    f.write_all(serde_json::to_string_pretty(&value)?.as_bytes())?;
    println!(
        "{}",
        json!({"status":"written","protocol":"prospective_opening_eval_v1","output_hash":decision::hash(&value)})
    );
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::forecast::tests::{fitted, packet_at};
    pub(crate) fn fixture() -> Input {
        let model = serde_json::to_value(fitted()).unwrap();
        let mut m = Manifest::v1();
        m.model_object = forecast::object_name(&decision::hash(&model));
        m.model_generation = "123".into();
        let reg = Registered {
            object: ObjectRef {
                name: m.object(),
                generation: "42".into(),
                committed_at: at("2026-11-11T17:00:00Z"),
            },
            manifest: m,
        };
        let day = "2026-11-12".parse().unwrap();
        let mut decisions = Vec::new();
        let mut labels = Vec::new();
        let start = day_start(day);
        for i in 0..4 {
            let open = start + i * 900;
            for asset in Asset::ALL {
                let p = packet_at(asset, open, i as usize);
                let cutoff = decision::time(&p["cutoff"]).unwrap();
                let proof = forecast::Frozen::new(
                    model.clone(),
                    reg.manifest.model_object.clone(),
                    "123".into(),
                    reg.manifest.model_committed_at,
                    &p,
                )
                .unwrap();
                let v = serde_json::to_value(proof).unwrap();
                let record = json!({"schema_version":1,"run_id":decision::run_id(asset,open),"strategy":decision::STRATEGY,"asset":asset,"open_ts":open,"cutoff":cutoff,"produced_at":cutoff,"status":"forecast","packet":p,"packet_hash":decision::hash(&p),"p_yes":p["yes_midpoint_usd"],"forecast_hash":decision::hash(&v),"forecast_proof":v});
                decisions.push(Saved {
                    object: ObjectRef {
                        name: format!("decisions/v1/{}.json", decision::run_id(asset, open)),
                        generation: "17".into(),
                        committed_at: DateTime::from_timestamp(open + 9, 0).unwrap(),
                    },
                    record,
                });
                let available = DateTime::from_timestamp(open + 1020, 0).unwrap();
                let ticker = p["ticker"].as_str().unwrap().to_owned();
                labels.push(Label{asset,ticker:ticker.clone(),open_ts:open,close_ts:open+900,result:if (i+asset_index(asset) as i64).rem_euclid(2)==0{"yes"}else{"no"}.into(),source_id:format!("projects/gitunderstand/databases/crypto-prediction-lab/documents/contracts_{}/{ticker}",symbol(asset).to_ascii_lowercase()),source_version:available.to_rfc3339_opts(chrono::SecondsFormat::AutoSi,true),captured_at:available,available_at:available});
            }
        }
        Input {
            schema_version: 1,
            experiment: reg,
            day,
            asof: DateTime::from_timestamp(start + 4 * 900 + 180, 0).unwrap(),
            decisions,
            labels,
            previous_asof: None,
            previous_report: None,
            previous_labels: Vec::new(),
        }
    }
    #[test]
    fn saved_forecasts_score_same_rows_and_keep_complete_asset_cycles() {
        let data = fixture();
        let (input, report, rows) = evaluate(data.clone()).unwrap();
        assert_eq!(input, data);
        assert_eq!(rows.len(), 12);
        assert_eq!(report["evaluation"]["pooled"]["scored"], 12);
        assert_eq!(
            report["evaluation"]["pooled"]["strategies"]["constant_half"]["brier"],
            0.25
        );
        for a in ["BTC", "ETH", "SOL"] {
            assert_eq!(report["evaluation"][a]["expected"], 4);
            assert_eq!(report["evaluation"][a]["scored"], 4);
        }
        assert_eq!(report["full_utc_day"], false);
        assert_eq!(report["paper_order_eligible"], false);
        assert_eq!(evaluate(data).unwrap().1, report);
    }
    #[test]
    fn future_result_versions_and_unavailable_objects_cannot_change_old_input_hash_or_scores() {
        let mut data = fixture();
        let original = evaluate(data.clone()).unwrap().1;
        let mut future = data.labels[0].clone();
        future.result = "no".into();
        future.available_at = data.asof + chrono::Duration::nanoseconds(1);
        future.captured_at = future.available_at;
        future.source_version = future.available_at.to_rfc3339();
        data.labels.push(future);
        let mut future = data.decisions[0].clone();
        future.object.committed_at = data.asof + chrono::Duration::nanoseconds(1);
        future.record["packet"]["outcome"] = json!("yes");
        data.decisions.push(future);
        assert_eq!(evaluate(data).unwrap().1, original);
    }
    #[test]
    fn official_result_mutations_change_scores_but_not_saved_forecast_identity() {
        let mut data = fixture();
        let original = evaluate(data.clone()).unwrap().1;
        data.labels[0].result = if data.labels[0].result == "yes" {
            "no"
        } else {
            "yes"
        }
        .into();
        let changed = evaluate(data).unwrap().1;
        assert_eq!(original["forecast_set_hash"], changed["forecast_set_hash"]);
        assert_ne!(original["evaluation"], changed["evaluation"]);
        assert_ne!(original["input_hash"], changed["input_hash"]);
    }
    #[test]
    fn missing_late_abstained_invalid_and_pending_rows_remain_in_denominators() {
        let mut data = fixture();
        data.decisions.remove(0);
        data.decisions[0].object.committed_at = DateTime::from_timestamp(
            data.decisions[0].record["open_ts"].as_i64().unwrap() + 40,
            0,
        )
        .unwrap();
        data.decisions[1].record["status"] = json!("abstain");
        data.decisions[2]
            .record
            .as_object_mut()
            .unwrap()
            .remove("forecast_proof");
        data.labels.retain(|l| {
            l.open_ts != data.decisions[3].record["open_ts"]
                || serde_json::to_value(l.asset).unwrap() != data.decisions[3].record["asset"]
        });
        let (_, r, rows) = evaluate(data).unwrap();
        assert_eq!(rows.len(), 12);
        let reasons = &r["evaluation"]["pooled"]["reason_counts"];
        for name in [
            "missing_decision",
            "late_or_invalid_decision",
            "decision_abstained",
            "no_committed_forecast",
            "official_result_unavailable",
        ] {
            assert_eq!(reasons[name], 1, "{name}");
        }
        assert_eq!(r["evaluation"]["pooled"]["expected"], 12);
        assert_eq!(r["evaluation"]["pooled"]["scored"], 7);
        assert!(
            r["evaluation"]["pooled"]["full_scheduled_delta_bounds"]["lower"]
                .as_f64()
                .unwrap()
                < 0.
        );
    }
    #[test]
    fn late_model_wrong_hash_injected_outcome_and_different_model_are_not_scored() {
        for case in 0..4 {
            let mut data = fixture();
            let r = &mut data.decisions[0].record;
            match case {
                0 => {
                    r["forecast_proof"]["model_committed_at"] = json!("2026-11-13T00:00:00Z");
                    r["forecast_hash"] = json!(decision::hash(&r["forecast_proof"]));
                }
                1 => r["forecast_hash"] = json!("0".repeat(64)),
                2 => {
                    r["packet"]["outcome"] = json!("yes");
                    r["packet_hash"] = json!(decision::hash(&r["packet"]));
                }
                _ => {
                    data.experiment.manifest.model_generation = "999".into();
                    data.experiment.object.name = data.experiment.manifest.object();
                }
            }
            let (_, report, rows) = evaluate(data).unwrap();
            assert_ne!(rows[0].status, "scored");
            assert!(report["evaluation"]["pooled"]["scored"].as_u64().unwrap() < 12);
        }
    }
    #[test]
    fn result_corrections_are_versioned_and_voids_are_never_counted_as_no() {
        let mut data = fixture();
        let (_, old, _) = evaluate(data.clone()).unwrap();
        let old_json = old.clone();
        data.previous_asof = Some(data.asof);
        data.previous_report = Some(ObjectRef {
            name: format!(
                "prospective/reports/v1/{}/days/{}/{}.json",
                data.experiment.manifest.id(),
                data.day,
                decision::hash(&old)
            ),
            generation: "99".into(),
            committed_at: data.asof,
        });
        data.previous_labels = data.labels.clone();
        data.asof += chrono::Duration::seconds(60);
        let l = &mut data.labels[0];
        l.result = "void".into();
        l.available_at = data.asof;
        l.captured_at = data.asof;
        l.source_version = data.asof.to_rfc3339();
        let (_, changed, rows) = evaluate(data).unwrap();
        assert_eq!(old, old_json);
        assert_eq!(changed["official_outcome_corrections"], 1);
        assert_eq!(changed["source_version_revisions"], 1);
        assert_eq!(rows[0].status, "non_binary_official_result");
        assert_eq!(changed["evaluation"]["pooled"]["scored"], 11);
        assert_eq!(old["evaluation"]["pooled"]["scored"], 12);
    }
    #[test]
    fn missing_current_result_is_not_filled_from_an_older_report() {
        let mut d = fixture();
        let (_, old, _) = evaluate(d.clone()).unwrap();
        d.previous_asof = Some(d.asof);
        d.previous_report = Some(ObjectRef {
            name: format!(
                "prospective/reports/v1/{}/days/{}/{}.json",
                d.experiment.manifest.id(),
                d.day,
                decision::hash(&old)
            ),
            generation: "99".into(),
            committed_at: d.asof,
        });
        d.previous_labels = d.labels.clone();
        d.labels.remove(0);
        d.asof += chrono::Duration::seconds(1);
        let (_, r, rows) = evaluate(d).unwrap();
        assert_eq!(rows[0].status, "official_result_unavailable");
        assert_eq!(r["evaluation"]["pooled"]["scored"], 11);
    }
    #[test]
    fn ambiguous_versions_wrong_identity_and_duplicate_decisions_fail_closed() {
        let mut d = fixture();
        d.decisions.push(d.decisions[0].clone());
        assert!(evaluate(d).is_err());
        let mut d = fixture();
        d.labels[0].source_id =
            "projects/other/databases/(default)/documents/contracts_btc/X".into();
        assert!(evaluate(d).is_err());
        let mut d = fixture();
        let mut l = d.labels[0].clone();
        l.result = "other".into();
        d.labels.push(l);
        assert!(evaluate(d).is_err());
        let mut d = fixture();
        d.experiment.object.committed_at = d.experiment.manifest.confirmatory_start;
        assert!(evaluate(d).is_err());
    }
    #[test]
    fn current_corrected_label_version_is_available_only_at_last_update_and_capture() {
        let data = fixture();
        let label = &data.labels[0];
        let doc = json!({"name":label.source_id,"createTime":label.source_version,"updateTime":label.source_version,"fields":{"close_time":{"timestampValue":DateTime::from_timestamp(label.close_ts,0)},"summary":{"stringValue":json!({"asset":label.asset,"ticker":label.ticker,"open_time":DateTime::from_timestamp(label.open_ts,0),"close_time":DateTime::from_timestamp(label.close_ts,0),"result":label.result,"captured_at":label.captured_at}).to_string()}}});
        let projected = label_projection(&doc, label.asset, data.day).unwrap();
        assert_eq!(&projected, label);
        let mut new = doc.clone();
        new["updateTime"] = json!(data.asof + chrono::Duration::nanoseconds(1));
        let l = label_projection(&new, label.asset, data.day).unwrap();
        assert!(l.available_at > data.asof);
        let mut d = data.clone();
        d.labels[0] = l;
        let (_, report, _) = evaluate(d).unwrap();
        assert_eq!(
            report["evaluation"]["pooled"]["reason_counts"]["official_result_unavailable"],
            1
        );
    }
    fn days(reg: &Registered, count: usize) -> Vec<DayStats> {
        (0..count)
            .map(|i| {
                let day =
                    reg.manifest.confirmatory_start.date_naive() + chrono::Duration::days(i as i64);
                DayStats {
                    day,
                    asof: DateTime::from_timestamp(day_start(day) + 86500, 0).unwrap(),
                    phase: "confirmatory".into(),
                    full_utc_day: true,
                    expected: 288,
                    timely: [96; 3],
                    scored: [96; 3],
                    paired_delta_sum: -2.,
                }
            })
            .collect()
    }
    #[test]
    fn uncertainty_requires_whole_registered_days_and_preserves_day_blocks() {
        let reg = fixture().experiment;
        let asof = at("2026-12-12T12:00:00Z");
        let from = reg.manifest.confirmatory_start.date_naive();
        let through = from + chrono::Duration::days(30);
        let mut stats = days(&reg, 29);
        let report = summarize(&reg, asof, from, through, &stats).unwrap();
        assert_eq!(report["day_block_interval"], Value::Null);
        assert_eq!(report["status"], "insufficient_evidence");
        stats = days(&reg, 30);
        let report = summarize(&reg, asof, from, through, &stats).unwrap();
        assert!(
            (report["day_block_interval"]["lower"].as_f64().unwrap() + 2. / 288.).abs() < 1e-12
        );
        assert_eq!(
            report,
            summarize(&reg, asof, from, through, &stats).unwrap()
        );
        assert_eq!(report["paper_order_eligible"], false);
        stats[0].scored[0] = 90;
        assert_eq!(
            summarize(&reg, asof, from, through, &stats).unwrap()["day_block_interval"],
            Value::Null
        );
        stats = days(&reg, 30);
        stats[0].asof = asof + chrono::Duration::nanoseconds(1);
        assert!(summarize(&reg, asof, from, through, &stats).is_err());
        stats = days(&reg, 30);
        stats.push(stats[0].clone());
        assert!(summarize(&reg, asof, from, through, &stats).is_err());
    }
}
