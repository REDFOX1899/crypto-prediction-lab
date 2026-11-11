//! Fixed-protocol, network-free numerical forecasting. Labels enter only the
//! offline fitter/evaluator; inference accepts an exact opening projection.
use crate::{assets::Asset, decision};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
const N: usize = 7;
const PROTOCOL: &str = "opening_logistic_sigmoid_v1";
const TRAIN: &str = "2026-11-11T06:00:00Z";
const CAL: &str = "2026-11-11T11:00:00Z";
const GAP: i64 = 1800;
const EPS: f64 = 0.000001;
const FEATURES: [&str; 6] = [
    "midpoint_logit",
    "spread_usd",
    "time_remaining_fraction",
    "native_bid_size_imbalance_top10",
    "btc_intercept",
    "eth_intercept",
];
pub fn methodology() -> Value {
    json!({"protocol":PROTOCOL,"features":FEATURES,"fit":"Pooled logistic with BTC/ETH intercepts; training-only mean/std (floor .01), standardized inputs clipped [-10,10], 800 full-batch steps, learning rate .1, L2 .1 except intercept",
        "calibration":"Disjoint later sigmoid calibration; 800 steps, learning rate .05, L2 .1 toward identity; slope [.05,10]",
        "training_before":TRAIN,"calibration_before":CAL,"embargo_seconds":GAP,"log_loss_clip":EPS,
        "availability":"Only original immutable opening packets; labels available by each fitting cutoff; model GCS commit must precede packet cutoff; no retroactive forecast attachment",
        "status":"Two-date engineering pilot; whole-day and prospective validation remain required",
        "paper_order_eligible":false,"probability_interval":null,"automatic_retraining":false})
}
fn instant(s: &str) -> Result<DateTime<Utc>, &'static str> {
    decision::time(&json!(s))
}
fn asset_index(a: Asset) -> usize {
    match a {
        Asset::Btc => 0,
        Asset::Eth => 1,
        Asset::Sol => 2,
    }
}
fn number(v: &Value) -> Result<f64, &'static str> {
    let x = v
        .as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .ok_or("invalid_number")?;
    if x.is_finite() {
        Ok(x)
    } else {
        Err("invalid_number")
    }
}
fn clipped(p: f64) -> f64 {
    p.clamp(EPS, 1. - EPS)
}
fn logit(p: f64) -> f64 {
    let p = clipped(p);
    (p / (1. - p)).ln()
}
fn sigmoid(x: f64) -> f64 {
    1. / (1. + (-x.clamp(-40., 40.)).exp())
}
fn features(p: &Value) -> Result<[f64; N], &'static str> {
    decision::validate_projection(p)?;
    let asset: Asset = serde_json::from_value(p["asset"].clone()).map_err(|_| "invalid_asset")?;
    let sum = |side: &str| -> Result<f64, &'static str> {
        p["book"][side]
            .as_array()
            .ok_or("missing_book")?
            .iter()
            .map(|l| number(&l[1]))
            .sum()
    };
    let yes = sum("yes_dollars")?;
    let no = sum("no_dollars")?;
    let remaining = ((p["open_ts"].as_i64().ok_or("missing_open")? + 900) as f64
        - decision::time(&p["cutoff"])?.timestamp_millis() as f64 / 1000.)
        / 900.;
    Ok([
        1.,
        logit(number(&p["yes_midpoint_usd"])?),
        number(&p["spread_usd"])?,
        remaining,
        (yes - no) / (yes + no),
        f64::from(asset == Asset::Btc),
        f64::from(asset == Asset::Eth),
    ])
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Support {
    pub rows: [usize; 3],
    pub cycles: usize,
    pub yes: usize,
    pub no: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Model {
    schema_version: u8,
    protocol: String,
    feature_names: Vec<String>,
    train_before: DateTime<Utc>,
    calibrate_before: DateTime<Utc>,
    embargo_seconds: i64,
    training_max_label_available_at: DateTime<Utc>,
    calibration_max_label_available_at: DateTime<Utc>,
    training_manifest_hash: String,
    calibration_manifest_hash: String,
    mean: [f64; N],
    scale: [f64; N],
    coefficients: [f64; N],
    calibration: [f64; 2],
    historical_yes_rate: [f64; 3],
    training: Support,
    calibration_support: Support,
}
fn dot(a: &[f64; N], b: &[f64; N]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
impl Model {
    fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.protocol != PROTOCOL
            || self.feature_names != FEATURES
            || self.train_before != instant(TRAIN)?
            || self.calibrate_before != instant(CAL)?
            || self.embargo_seconds != GAP
            || self.training_max_label_available_at > self.train_before
            || self.calibration_max_label_available_at > self.calibrate_before
            || self.training_max_label_available_at > self.calibration_max_label_available_at
            || !supported(&self.training, 24, 8)
            || !supported(&self.calibration_support, 12, 6)
            || self.scale.iter().any(|v| !v.is_finite() || *v <= 0.)
            || self
                .mean
                .iter()
                .chain(self.coefficients.iter())
                .chain(self.calibration.iter())
                .any(|v| !v.is_finite() || v.abs() > 1000.)
            || self.calibration[1] < 0.05
            || self.calibration[1] > 10.
            || self
                .historical_yes_rate
                .iter()
                .any(|p| !p.is_finite() || *p <= 0. || *p >= 1.)
            || [
                &self.training_manifest_hash,
                &self.calibration_manifest_hash,
            ]
            .iter()
            .any(|s| !valid_hash(s))
        {
            return Err("invalid_model");
        }
        Ok(())
    }
    fn standardize(&self, x: &[f64; N]) -> [f64; N] {
        std::array::from_fn(|j| {
            if j == 0 {
                1.
            } else {
                ((x[j] - self.mean[j]) / self.scale[j]).clamp(-10., 10.)
            }
        })
    }
    fn predict(&self, x: &[f64; N]) -> (f64, f64) {
        let z = dot(&self.coefficients, &self.standardize(x));
        (
            clipped(sigmoid(z)),
            clipped(sigmoid(self.calibration[0] + self.calibration[1] * z)),
        )
    }
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn object_name(hash: &str) -> String {
    format!("forecast_models/v1/{hash}.json")
}
pub fn valid_object(name: &str) -> bool {
    name.strip_prefix("forecast_models/v1/")
        .and_then(|s| s.strip_suffix(".json"))
        .is_some_and(valid_hash)
}

/// This proof is frozen inside a timely decision and subsequently its MCP scope.
/// The model's availability is authoritative GCS metadata, supplied by the host.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frozen {
    model: Model,
    object_name: String,
    generation: String,
    model_committed_at: DateTime<Utc>,
    packet_hash: String,
}
impl Frozen {
    pub fn new(
        value: Value,
        name: String,
        generation: String,
        committed: DateTime<Utc>,
        packet: &Value,
    ) -> Result<Self, &'static str> {
        let model: Model = serde_json::from_value(value.clone()).map_err(|_| "invalid_model")?;
        // Hash the normalized typed artifact, which is also checked at publication.
        if value != serde_json::to_value(&model).map_err(|_| "invalid_model")? {
            return Err("invalid_model");
        }
        let result = Self {
            model,
            object_name: name,
            generation,
            model_committed_at: committed,
            packet_hash: decision::hash(packet),
        };
        result.validate(packet)?;
        Ok(result)
    }
    pub fn validate(&self, packet: &Value) -> Result<(), &'static str> {
        self.model.validate()?;
        features(packet)?;
        if self.object_name
            != object_name(&decision::hash(
                &serde_json::to_value(&self.model).map_err(|_| "invalid_model")?,
            ))
            || self.generation.is_empty()
            || !self.generation.bytes().all(|b| b.is_ascii_digit())
            || self.packet_hash != decision::hash(packet)
            || self.model_committed_at > decision::time(&packet["cutoff"])?
            || self.model_committed_at < self.model.calibrate_before
        {
            return Err("model_not_available_at_cutoff");
        }
        Ok(())
    }
    pub fn response(&self, packet: &Value) -> Result<Value, &'static str> {
        self.validate(packet)?;
        let x = features(packet)?;
        let (raw, calibrated) = self.model.predict(&x);
        let a: Asset =
            serde_json::from_value(packet["asset"].clone()).map_err(|_| "invalid_asset")?;
        Ok(
            json!({"status":"experimental_forecast","model_id":self.object_name,"model_generation":self.generation,
            "model_committed_at":self.model_committed_at,"protocol":PROTOCOL,"p_yes_raw":raw,"p_yes_calibrated":calibrated,
            "baselines":{"constant_half":0.5,"historical_yes_rate":self.model.historical_yes_rate[asset_index(a)],"market_midpoint":packet["yes_midpoint_usd"]},
            "training":self.model.training,"calibration_support":self.model.calibration_support,
            "calibration_status":"fitted_diagnostic_only","probability_interval":null,"paper_order_eligible":false,
            "limitation":"Two-date chronological pilot; no validated predictive edge or actionable uncertainty bounds"}),
        )
    }
}

// The label-bearing types below never cross into live inference or MCP tools.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionRow {
    object_name: String,
    generation: String,
    committed_at: DateTime<Utc>,
    run_id: String,
    asset: Asset,
    open_ts: i64,
    status: String,
    packet: Value,
    packet_hash: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Label {
    asset: Asset,
    ticker: String,
    open_ts: i64,
    close_ts: i64,
    yes: bool,
    available_at: DateTime<Utc>,
    source_id: String,
    source_version: DateTime<Utc>,
    version_hash: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Dataset {
    schema_version: u8,
    read_time: DateTime<Utc>,
    start_ts: i64,
    end_ts: i64,
    expected_completed_asset_windows: usize,
    decisions: Vec<DecisionRow>,
    labels: Vec<Label>,
}
#[derive(Clone)]
struct Row {
    open: i64,
    asset: Asset,
    x: [f64; N],
    midpoint: f64,
    y: f64,
    available: DateTime<Utc>,
    provenance: Value,
}
fn support(rows: &[Row]) -> Support {
    let mut s = Support {
        rows: [0; 3],
        cycles: rows.iter().map(|r| r.open).collect::<BTreeSet<_>>().len(),
        yes: 0,
        no: 0,
    };
    for r in rows {
        s.rows[asset_index(r.asset)] += 1;
        if r.y == 1. { s.yes += 1 } else { s.no += 1 }
    }
    s
}
fn supported(s: &Support, cycles: usize, classes: usize) -> bool {
    s.cycles >= cycles
        && s.rows.iter().all(|n| *n >= classes)
        && s.yes >= classes
        && s.no >= classes
}
fn manifest(rows: &[Row]) -> String {
    decision::hash(&json!(
        rows.iter().map(|r| &r.provenance).collect::<Vec<_>>()
    ))
}
fn joined(data: &Dataset) -> Result<(Vec<Row>, usize), &'static str> {
    if data.schema_version != 1
        || data.start_ts >= data.end_ts
        || data.end_ts > data.read_time.timestamp()
        || data.end_ts - data.start_ts > 7 * 86400
        || data.decisions.len() > 2016
        || data.labels.len() > 2016
    {
        return Err("invalid_dataset");
    }
    let mut labels = BTreeMap::new();
    for label in &data.labels {
        let key = (label.open_ts, asset_index(label.asset));
        if labels.insert(key, label).is_some() {
            return Err("duplicate_label");
        }
        if label.close_ts != label.open_ts + 900
            || label.open_ts.rem_euclid(900) != 0
            || label.available_at.timestamp() < label.close_ts
            || label.source_version > label.available_at
            || label.available_at > data.read_time
            || !label
                .ticker
                .starts_with(&format!("{}-", label.asset.info().series))
            || label.version_hash
                != decision::hash(
                    &json!({"result":if label.yes {"yes"} else {"no"},"source_id":label.source_id,"version":label.source_version}),
                )
            || !valid_hash(&label.version_hash)
            || label.source_id
                != format!(
                    "projects/gitunderstand/databases/crypto-prediction-lab/documents/contracts_{}/{}",
                    serde_json::to_value(label.asset)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_ascii_lowercase(),
                    label.ticker
                )
        {
            return Err("invalid_label");
        }
    }
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    let mut excluded = 0;
    for d in &data.decisions {
        if !seen.insert((d.open_ts, asset_index(d.asset))) {
            return Err("duplicate_decision");
        }
        if d.status != "forecast" {
            excluded += 1;
            continue;
        }
        let check = || -> Result<Row, &'static str> {
            let x = features(&d.packet)?;
            let cutoff = decision::time(&d.packet["cutoff"])?;
            if d.open_ts < data.start_ts
                || d.open_ts >= data.end_ts
                || d.committed_at > data.read_time
                || d.committed_at < cutoff
                || d.committed_at.timestamp() >= d.open_ts + 40
                || d.run_id != decision::run_id(d.asset, d.open_ts)
                || d.object_name != format!("decisions/v1/{}.json", d.run_id)
                || d.generation.is_empty()
                || !d.generation.bytes().all(|b| b.is_ascii_digit())
                || d.packet["asset"] != serde_json::to_value(d.asset).unwrap()
                || d.packet["open_ts"] != d.open_ts
                || d.packet_hash.as_deref() != Some(decision::hash(&d.packet).as_str())
            {
                return Err("invalid_decision");
            }
            let l = labels
                .get(&(d.open_ts, asset_index(d.asset)))
                .ok_or("missing_label")?;
            if d.packet["ticker"] != l.ticker {
                return Err("label_identity");
            }
            Ok(Row {
                open: d.open_ts,
                asset: d.asset,
                x,
                midpoint: number(&d.packet["yes_midpoint_usd"])?,
                y: f64::from(l.yes),
                available: l.available_at,
                provenance: json!({"object":d.object_name,"generation":d.generation,"packet_hash":d.packet_hash,"label":l}),
            })
        };
        match check() {
            Ok(row) => rows.push(row),
            Err("missing_label") => excluded += 1,
            Err(e) => return Err(e),
        }
    }
    rows.sort_by_key(|r| (r.open, asset_index(r.asset)));
    Ok((rows, excluded))
}
// Only completed training rows enter both normalization and gradient fitting.
fn train(rows: &[Row], calibration: &[Row]) -> Result<Model, &'static str> {
    let tr = support(rows);
    let ca = support(calibration);
    if !supported(&tr, 24, 8) || !supported(&ca, 12, 6) {
        return Err("insufficient_support");
    }
    let mut model = Model {
        schema_version: 1,
        protocol: PROTOCOL.into(),
        feature_names: FEATURES.iter().map(|s| s.to_string()).collect(),
        train_before: instant(TRAIN)?,
        calibrate_before: instant(CAL)?,
        embargo_seconds: GAP,
        training_max_label_available_at: rows
            .iter()
            .map(|r| r.available)
            .max()
            .ok_or("missing_train")?,
        calibration_max_label_available_at: calibration
            .iter()
            .map(|r| r.available)
            .max()
            .ok_or("missing_calibration")?,
        training_manifest_hash: manifest(rows),
        calibration_manifest_hash: manifest(calibration),
        mean: [0.; N],
        scale: [1.; N],
        coefficients: [0.; N],
        calibration: [0., 1.],
        historical_yes_rate: [0.; 3],
        training: tr,
        calibration_support: ca,
    };
    for j in 1..N {
        model.mean[j] = rows.iter().map(|r| r.x[j]).sum::<f64>() / rows.len() as f64;
        model.scale[j] = (rows
            .iter()
            .map(|r| (r.x[j] - model.mean[j]).powi(2))
            .sum::<f64>()
            / rows.len() as f64)
            .sqrt()
            .max(0.01);
    }
    for a in Asset::ALL {
        let rs: Vec<_> = rows.iter().filter(|r| r.asset == a).collect();
        model.historical_yes_rate[asset_index(a)] =
            (1. + rs.iter().map(|r| r.y).sum::<f64>()) / (2. + rs.len() as f64);
    }
    // Predeclared full-batch gradient descent: 800 steps, lr .1, L2 .1.
    let xs: Vec<_> = rows.iter().map(|r| model.standardize(&r.x)).collect();
    for _ in 0..800 {
        let mut grad = [0.; N];
        for (r, x) in rows.iter().zip(&xs) {
            let residual = sigmoid(dot(&model.coefficients, x)) - r.y;
            for j in 0..N {
                grad[j] += residual * x[j] / rows.len() as f64;
            }
        }
        for (j, g) in grad.iter().enumerate() {
            model.coefficients[j] -= 0.1
                * (g + if j == 0 {
                    0.
                } else {
                    0.1 * model.coefficients[j]
                });
        }
    }
    // Disjoint later calibration segment, ridge toward identity, positive slope.
    let zs: Vec<_> = calibration
        .iter()
        .map(|r| dot(&model.coefficients, &model.standardize(&r.x)))
        .collect();
    for _ in 0..800 {
        let mut grad = [0.; 2];
        for (r, z) in calibration.iter().zip(&zs) {
            let residual = sigmoid(model.calibration[0] + model.calibration[1] * z) - r.y;
            grad[0] += residual / calibration.len() as f64;
            grad[1] += residual * z / calibration.len() as f64;
        }
        model.calibration[0] -= 0.05 * (grad[0] + 0.1 * model.calibration[0]);
        model.calibration[1] = (model.calibration[1]
            - 0.05 * (grad[1] + 0.1 * (model.calibration[1] - 1.)))
            .clamp(0.05, 10.);
    }
    model.validate()?;
    Ok(model)
}
fn metrics(pairs: &[(f64, f64)]) -> Value {
    let n = pairs.len();
    if n == 0 {
        return json!({"rows":0,"brier":null,"log_loss":null,"reliability_bins":[]});
    }
    let bins: Vec<_> = (0..5).map(|i| { let ps: Vec<_> = pairs.iter().filter(|(p,_)| ((*p*5.).floor() as usize).min(4)==i).collect(); let k=ps.len();
        json!({"lower":i as f64/5.,"upper":(i+1) as f64/5.,"rows":k,"mean_probability":if k>0 {Some(ps.iter().map(|(p,_)|p).sum::<f64>()/k as f64)} else {None},"observed_yes_rate":if k>0 {Some(ps.iter().map(|(_,y)|y).sum::<f64>()/k as f64)} else {None}}) }).collect();
    json!({"rows":n,"brier":pairs.iter().map(|(p,y)|(p-y).powi(2)).sum::<f64>()/n as f64,
        "log_loss":-pairs.iter().map(|(p,y)|y*clipped(*p).ln()+(1.-y)*(1.-clipped(*p)).ln()).sum::<f64>()/n as f64,"reliability_bins":bins})
}
fn evaluation(model: &Model, rows: &[Row]) -> Value {
    let mut result = serde_json::Map::new();
    for asset in [None, Some(Asset::Btc), Some(Asset::Eth), Some(Asset::Sol)] {
        let rs: Vec<_> = rows
            .iter()
            .filter(|r| asset.is_none_or(|a| r.asset == a))
            .collect();
        let mut strategies = serde_json::Map::new();
        for strategy in [
            "constant_half",
            "historical_yes_rate",
            "market_midpoint",
            "logistic_raw",
            "logistic_calibrated",
        ] {
            let pairs: Vec<_> = rs
                .iter()
                .map(|r| {
                    let (raw, cal) = model.predict(&r.x);
                    let p = match strategy {
                        "constant_half" => 0.5,
                        "historical_yes_rate" => model.historical_yes_rate[asset_index(r.asset)],
                        "market_midpoint" => r.midpoint,
                        "logistic_raw" => raw,
                        _ => cal,
                    };
                    (p, r.y)
                })
                .collect();
            strategies.insert(strategy.into(), metrics(&pairs));
        }
        let delta = if rs.is_empty() {
            None
        } else {
            Some(
                rs.iter()
                    .map(|r| {
                        let (_, p) = model.predict(&r.x);
                        (p - r.y).powi(2) - (r.midpoint - r.y).powi(2)
                    })
                    .sum::<f64>()
                    / rs.len() as f64,
            )
        };
        result.insert(
            asset
                .map(|a| {
                    serde_json::to_value(a)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned()
                })
                .unwrap_or_else(|| "pooled".into()),
            json!({"strategies":strategies,"paired_brier_delta_calibrated_minus_midpoint":delta}),
        );
    }
    Value::Object(result)
}
fn fit(data: &Dataset) -> Result<(Model, Value), &'static str> {
    let (rows, excluded) = joined(data)?;
    let t = instant(TRAIN)?;
    let c = instant(CAL)?;
    let train_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.open < t.timestamp() - GAP && r.available <= t)
        .cloned()
        .collect();
    let cal_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.open >= t.timestamp() && r.open < c.timestamp() - GAP && r.available <= c)
        .cloned()
        .collect();
    let test: Vec<_> = rows
        .iter()
        .filter(|r| r.open >= c.timestamp())
        .cloned()
        .collect();
    let model = train(&train_rows, &cal_rows)?;
    let report = json!({"schema_version":1,"protocol":PROTOCOL,"read_time":data.read_time,"train_before":t,"calibrate_before":c,"embargo_seconds":GAP,
        "expected_completed_asset_windows":data.expected_completed_asset_windows,"decisions":data.decisions.len(),"joined":rows.len(),"excluded":excluded,"excluded_from_splits":rows.len()-train_rows.len()-cal_rows.len()-test.len(),
        "training":support(&train_rows),"calibration":support(&cal_rows),"test":support(&test),"heldout_manifest_hash":manifest(&test),"evaluation":evaluation(&model,&test),
        "evaluation_status":if test.iter().map(|r|r.open).collect::<BTreeSet<_>>().len()>=8 {"small_chronological_pilot"} else {"insufficient_test_support"},
        "simulation":"Retrospective hypothetical forecasts, not historically committed decisions or fills",
        "limitations":["Only two UTC dates; no whole-day independent validation","Related BTC ETH SOL observations share cycles","Calibration is a fitted diagnostic, not proven probability accuracy","No confidence bound, validated market edge, buy recommendation or eligible order","No CF-relative distance/volatility model: entitled as-of reference features unavailable"],"paper_order_eligible":false});
    Ok((model, report))
}
pub fn fit_cli() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::var("FORECAST_INPUT")?;
    let out = std::path::PathBuf::from(std::env::var("FORECAST_OUTPUT_DIR")?);
    let bytes = std::fs::read(input)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Dataset too large".into());
    }
    let data: Dataset = serde_json::from_slice(&bytes)?;
    let (model, mut report) = fit(&data)?;
    let value = serde_json::to_value(model)?;
    let hash = decision::hash(&value);
    report["model_id"] = json!(object_name(&hash));
    std::fs::create_dir_all(&out)?;
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    for (name, v) in [("model.json", value), ("report.json", report.clone())] {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(out.join(name))?;
        f.write_all(serde_json::to_string_pretty(&v)?.as_bytes())?;
    }
    println!(
        "{}",
        json!({"model_object":object_name(&hash),"training":report["training"],"calibration":report["calibration"],"test":report["test"],"private_outputs_written":true})
    );
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        checkpoint::Phase,
        decision::tests::{fixture, open},
    };
    fn shift(v: &mut Value, delta: i64) {
        match v {
            Value::String(s) if s.starts_with("2026-") && s.contains('T') => {
                if let Ok(t) = instant(s) {
                    *v = json!(t + chrono::Duration::seconds(delta));
                }
            }
            Value::Array(a) => {
                for x in a {
                    shift(x, delta)
                }
            }
            Value::Object(o) => {
                for x in o.values_mut() {
                    shift(x, delta)
                }
            }
            _ => {}
        }
    }
    pub fn packet_at(a: Asset, ts: i64, index: usize) -> Value {
        let mut doc = fixture(a, Phase::Opening, 5, 6, 7);
        let mut row: Value =
            serde_json::from_str(doc["fields"]["payload"]["stringValue"].as_str().unwrap())
                .unwrap();
        shift(&mut row, ts - open());
        row["observation"]["yes_bids"][0][0] = json!(format!("0.{:02}", 20 + index % 30));
        row["observation"]["no_bids"][0][0] = json!("0.40");
        row["observation"]["yes_bids"][1][0] = json!("0.10");
        doc["fields"]["payload"]["stringValue"] = json!(row.to_string());
        doc["name"] = json!(format!(
            "projects/gitunderstand/databases/crypto-prediction-lab/documents/{}",
            Phase::Opening.path(a, ts)
        ));
        for key in ["createTime", "updateTime"] {
            doc[key] = json!(DateTime::from_timestamp(ts + 7, 0).unwrap());
        }
        decision::packet(
            &doc,
            a,
            ts,
            DateTime::from_timestamp(ts + 8, 0).unwrap(),
            Phase::Opening,
        )
        .unwrap()
    }
    fn refresh_hash(l: &mut Label) {
        l.version_hash = decision::hash(
            &json!({"result":if l.yes {"yes"} else {"no"},"source_id":l.source_id,"version":l.source_version}),
        );
    }
    fn dataset() -> Dataset {
        let mut data = Dataset {
            schema_version: 1,
            read_time: instant("2026-11-11T15:00:00Z").unwrap(),
            start_ts: instant("2026-11-10T00:00:00Z").unwrap().timestamp(),
            end_ts: instant("2026-11-11T15:00:00Z").unwrap().timestamp(),
            expected_completed_asset_windows: 0,
            decisions: vec![],
            labels: vec![],
        };
        let opens = (0..48)
            .map(|i| instant("2026-11-10T17:00:00Z").unwrap().timestamp() + i * 900)
            .chain((0..18).map(|i| instant(TRAIN).unwrap().timestamp() + i * 900))
            .chain((0..12).map(|i| instant(CAL).unwrap().timestamp() + i * 900));
        for (i, ts) in opens.enumerate() {
            for a in Asset::ALL {
                let packet = packet_at(a, ts, i);
                let run_id = decision::run_id(a, ts);
                let ticker = packet["ticker"].as_str().unwrap().to_owned();
                data.decisions.push(DecisionRow {
                    object_name: format!("decisions/v1/{run_id}.json"),
                    generation: "123".into(),
                    committed_at: DateTime::from_timestamp(ts + 9, 0).unwrap(),
                    run_id,
                    asset: a,
                    open_ts: ts,
                    status: "forecast".into(),
                    packet_hash: Some(decision::hash(&packet)),
                    packet,
                });
                let available = DateTime::from_timestamp(ts + 1020, 0).unwrap();
                let mut l = Label {
                    asset: a,
                    ticker: ticker.clone(),
                    open_ts: ts,
                    close_ts: ts + 900,
                    yes: (i + asset_index(a)).is_multiple_of(2),
                    available_at: available,
                    source_id: format!(
                        "projects/gitunderstand/databases/crypto-prediction-lab/documents/contracts_{}/{}",
                        serde_json::to_value(a)
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_ascii_lowercase(),
                        ticker
                    ),
                    source_version: available,
                    version_hash: String::new(),
                };
                refresh_hash(&mut l);
                data.labels.push(l);
            }
        }
        data
    }
    pub fn fitted() -> Model {
        fit(&dataset()).unwrap().0
    }
    #[test]
    fn chronological_fit_is_deterministic_and_heldout_labels_do_not_enter_artifact() {
        let mut data = dataset();
        let (model, report) = fit(&data).unwrap();
        assert_eq!(model, fit(&data).unwrap().0);
        assert_eq!(report["test"]["cycles"], 12);
        for l in &mut data.labels {
            if l.open_ts >= instant(CAL).unwrap().timestamp() {
                l.yes = !l.yes;
                refresh_hash(l);
            }
        }
        let (other, changed) = fit(&data).unwrap();
        assert_eq!(model, other);
        assert_ne!(
            report["heldout_manifest_hash"],
            changed["heldout_manifest_hash"]
        );
        assert_ne!(report["evaluation"], changed["evaluation"]);
        for d in &mut data.decisions {
            if d.open_ts >= instant(CAL).unwrap().timestamp() {
                d.packet["target_usd"] = json!("12345");
                d.packet_hash = Some(decision::hash(&d.packet));
            }
        }
        assert_eq!(model, fit(&data).unwrap().0);
        assert_eq!(report["training"]["rows"], json!([48, 48, 48]));
        assert_eq!(report["calibration"]["rows"], json!([18, 18, 18]));
    }
    #[test]
    fn label_versions_after_fit_or_calibration_cutoffs_are_excluded() {
        for (boundary, index) in [(TRAIN, 0), (CAL, 48 * 3)] {
            let mut data = dataset();
            data.labels[index].available_at =
                instant(boundary).unwrap() + chrono::Duration::nanoseconds(1);
            data.labels[index].source_version = data.labels[index].available_at;
            refresh_hash(&mut data.labels[index]);
            let delayed = fit(&data).unwrap().0;
            let key = (data.labels[index].open_ts, data.labels[index].asset);
            data.labels.remove(index);
            data.decisions.retain(|d| (d.open_ts, d.asset) != key);
            assert_eq!(delayed, fit(&data).unwrap().0);
        }
    }
    #[test]
    fn model_commit_after_cutoff_cannot_be_replayed_onto_old_packet() {
        let model = fitted();
        let value = serde_json::to_value(model).unwrap();
        let name = object_name(&decision::hash(&value));
        let ts = instant("2026-11-11T16:00:00Z").unwrap().timestamp();
        let p = packet_at(Asset::Btc, ts, 0);
        let proof = Frozen::new(
            value.clone(),
            name.clone(),
            "12".into(),
            DateTime::from_timestamp(ts - 60, 0).unwrap(),
            &p,
        )
        .unwrap();
        let proof: Frozen = serde_json::from_str(&serde_json::to_string(&proof).unwrap()).unwrap();
        let response = proof.response(&p).unwrap();
        assert_eq!(response["paper_order_eligible"], false);
        assert_eq!(response["probability_interval"], Value::Null);
        assert!(
            Frozen::new(
                value.clone(),
                name.clone(),
                "12".into(),
                decision::time(&p["cutoff"]).unwrap() + chrono::Duration::nanoseconds(1),
                &p
            )
            .is_err()
        );
        let old = packet_at(
            Asset::Btc,
            instant("2026-11-11T10:00:00Z").unwrap().timestamp(),
            0,
        );
        assert!(
            Frozen::new(
                value,
                name,
                "12".into(),
                instant("2026-11-11T15:00:00Z").unwrap(),
                &old
            )
            .is_err()
        );
        let mut bad = serde_json::to_value(&proof).unwrap();
        bad["model"]["calibration_max_label_available_at"] = json!("2026-11-11T12:00:00Z");
        assert!(
            serde_json::from_value::<Frozen>(bad)
                .unwrap()
                .validate(&p)
                .is_err()
        );
        let different = packet_at(Asset::Eth, ts, 0);
        assert!(proof.validate(&different).is_err());
    }
    #[test]
    fn injected_future_fields_changed_books_and_late_commits_are_denied() {
        for field in [
            "outcome",
            "future_snapshot",
            "expiration_value",
            "candidate",
            "p_yes",
        ] {
            let mut data = dataset();
            data.decisions[0].packet[field] = json!("yes");
            data.decisions[0].packet_hash = Some(decision::hash(&data.decisions[0].packet));
            assert!(fit(&data).is_err());
        }
        let mut d = dataset();
        d.decisions[0].committed_at =
            DateTime::from_timestamp(d.decisions[0].open_ts + 40, 0).unwrap();
        assert!(fit(&d).is_err());
        let mut d = dataset();
        d.decisions[0].packet["yes_midpoint_usd"] = json!("0.99");
        d.decisions[0].packet_hash = Some(decision::hash(&d.decisions[0].packet));
        assert!(fit(&d).is_err());
        let mut d = dataset();
        d.labels[0].ticker = "KXETH15M-WRONG".into();
        assert!(fit(&d).is_err());
        let mut d = dataset();
        d.labels.push(d.labels[0].clone());
        assert!(fit(&d).is_err());
    }
    #[test]
    fn metrics_use_proper_scores_and_show_empty_bins() {
        let m = metrics(&[(0.5, 0.), (0.5, 1.)]);
        assert_eq!(m["brier"], 0.25);
        assert!((m["log_loss"].as_f64().unwrap() - 2f64.ln()).abs() < 1e-12);
        assert_eq!(m["reliability_bins"][0]["rows"], 0);
        assert_eq!(m["reliability_bins"][0]["mean_probability"], Value::Null);
        assert_eq!(metrics(&[])["brier"], Value::Null);
        assert!(
            metrics(&[(0., 1.)])["log_loss"]
                .as_f64()
                .unwrap()
                .is_finite()
        );
        assert!(valid_object(&object_name(&"a".repeat(64))));
        assert!(!valid_object("forecast_models/v1/../outcomes.json"));
    }
    #[test]
    fn insufficient_support_cannot_be_promoted_to_a_model() {
        let mut d = dataset();
        d.decisions.truncate(12);
        assert!(matches!(fit(&d), Err("insufficient_support")));
    }
}
