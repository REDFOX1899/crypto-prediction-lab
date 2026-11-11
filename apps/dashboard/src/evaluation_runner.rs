//! Trusted, on-demand report publisher. Never constructed in the MCP role.
use crate::{
    assets::Asset,
    decision,
    decision_runner::{BUCKET, Store},
    outcomes::Archive,
    prospective::{self, DayStats, Input, Label, Manifest, ObjectRef, Registered, Saved},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, NaiveDate, Utc};
use futures_util::{StreamExt, TryStreamExt, stream};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
type Failure = Box<dyn std::error::Error + Send + Sync>;
const DOCUMENTS: &str = "https://firestore.googleapis.com/v1/projects/gitunderstand/databases/crypto-prediction-lab/documents";
const PART_LIMIT: usize = 48 * 1024;
pub struct Service {
    archive: Arc<Archive>,
    store: Store,
    experiment_object: String,
    documents: String,
}
impl Service {
    pub fn from_env(archive: Arc<Archive>) -> Result<Option<Arc<Self>>, Failure> {
        let Ok(experiment_object) = std::env::var("EVALUATION_EXPERIMENT_OBJECT") else {
            return Ok(None);
        };
        if experiment_object != Manifest::v1().object()
            || std::env::var("DECISION_BUCKET").as_deref() != Ok(BUCKET)
        {
            return Err("Invalid evaluator configuration".into());
        }
        Ok(Some(Arc::new(Self {
            archive,
            store: Store {
                http: reqwest::Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build()?,
                base: "https://storage.googleapis.com".into(),
                bucket: BUCKET.into(),
            },
            experiment_object,
            documents: DOCUMENTS.into(),
        })))
    }
    async fn registration(&self, token: &str) -> Result<Registered, Failure> {
        let stored = self
            .store
            .get(&self.experiment_object, token)
            .await?
            .ok_or("Experiment not registered")?;
        let r = Registered {
            manifest: serde_json::from_value(stored.value)?,
            object: ObjectRef {
                name: self.experiment_object.clone(),
                generation: stored.generation,
                committed_at: stored.created_at,
            },
        };
        r.validate(Utc::now())?;
        if r.manifest != Manifest::v1() {
            return Err("Unexpected registered model protocol".into());
        }
        Ok(r)
    }
    async fn list(
        &self,
        prefix: &str,
        start: Option<&str>,
        end: Option<&str>,
        limit: usize,
        asof: DateTime<Utc>,
        token: &str,
    ) -> Result<Vec<ObjectRef>, Failure> {
        let mut found = Vec::new();
        let mut page = String::new();
        for _ in 0..(limit / 100 + 1) {
            let mut query = vec![
                ("prefix", prefix),
                ("maxResults", "100"),
                ("fields", "items(name,generation,timeCreated),nextPageToken"),
            ];
            if let Some(start) = start {
                query.push(("startOffset", start));
            }
            if let Some(end) = end {
                query.push(("endOffset", end));
            }
            if !page.is_empty() {
                query.push(("pageToken", &page));
            }
            let value = Store::json(
                self.store
                    .http
                    .get(format!(
                        "{}/storage/v1/b/{}/o",
                        self.store.base, self.store.bucket
                    ))
                    .query(&query)
                    .bearer_auth(token)
                    .send()
                    .await?,
            )
            .await?;
            for item in value["items"].as_array().into_iter().flatten() {
                let name = item["name"].as_str().ok_or("Missing object name")?;
                if !name.starts_with(prefix) || name.len() > 300 {
                    return Err("Invalid listed object".into());
                }
                let committed_at = decision::time(&item["timeCreated"])?;
                if committed_at <= asof {
                    found.push(ObjectRef {
                        name: name.into(),
                        generation: item["generation"]
                            .as_str()
                            .ok_or("Missing generation")?
                            .into(),
                        committed_at,
                    });
                }
                if found.len() > limit {
                    return Err("Report inventory limit exceeded".into());
                }
            }
            page = value["nextPageToken"].as_str().unwrap_or("").into();
            if page.is_empty() {
                return Ok(found);
            }
        }
        Err("Report inventory page limit exceeded".into())
    }
    async fn read(&self, object: &ObjectRef, token: &str) -> Result<Value, Failure> {
        Store::json(
            self.store
                .http
                .get(self.store.url(&object.name)?)
                .query(&[("alt", "media"), ("generation", &object.generation)])
                .bearer_auth(token)
                .send()
                .await?,
        )
        .await
    }
    async fn create(&self, prefix: &str, value: &Value, token: &str) -> Result<ObjectRef, Failure> {
        let name = format!("{prefix}/{}.json", decision::hash(value));
        let stored = self.store.create(&name, value, token).await?;
        if stored.value != *value {
            return Err("Immutable report conflict".into());
        }
        Ok(ObjectRef {
            name,
            generation: stored.generation,
            committed_at: stored.created_at,
        })
    }
    fn day_prefix(reg: &Registered, day: NaiveDate) -> String {
        format!("prospective/reports/v1/{}/days/{day}/", reg.manifest.id())
    }
    fn verified_hash(name: &str, value: &Value, prefix: &str) -> Result<(), Failure> {
        let expected = format!("{prefix}{}.json", decision::hash(value));
        if name != expected {
            return Err("Report content hash mismatch".into());
        }
        Ok(())
    }
    async fn latest(
        &self,
        reg: &Registered,
        day: NaiveDate,
        asof: DateTime<Utc>,
        token: &str,
    ) -> Result<Option<(ObjectRef, Value)>, Failure> {
        let prefix = Self::day_prefix(reg, day);
        let objects = self.list(&prefix, None, None, 256, asof, token).await?;
        let Some(object) = objects
            .into_iter()
            .max_by(|a, b| (a.committed_at, &a.generation).cmp(&(b.committed_at, &b.generation)))
        else {
            return Ok(None);
        };
        let report = self.read(&object, token).await?;
        Self::verified_hash(&object.name, &report, &prefix)?;
        if report["schema_version"] != 1
            || report["experiment_id"] != reg.manifest.id()
            || report["model_object"] != reg.manifest.model_object
            || report["day"] != day.to_string()
            || decision::time(&report["asof"])? > object.committed_at
        {
            return Err("Invalid saved evaluation".into());
        }
        Ok(Some((object, report)))
    }
    async fn previous_labels(
        &self,
        reg: &Registered,
        report: &Value,
        token: &str,
    ) -> Result<Vec<Label>, Failure> {
        let root: ObjectRef = serde_json::from_value(report["input_manifest"].clone())?;
        let prefix = format!("prospective/manifests/v1/{}/", reg.manifest.id());
        let manifest = self.read(&root, token).await?;
        Self::verified_hash(&root.name, &manifest, &prefix)?;
        if manifest["input_hash"] != report["input_hash"]
            || manifest["experiment_id"] != reg.manifest.id()
        {
            return Err("Invalid input manifest".into());
        }
        let mut labels = Vec::new();
        let parts = manifest["parts"].as_array().ok_or("Missing input parts")?;
        if parts.len() > 128 {
            return Err("Too many input parts".into());
        }
        for part in parts.iter().filter(|p| p["kind"] == "labels") {
            let object: ObjectRef = serde_json::from_value(part["object"].clone())?;
            let value = self.read(&object, token).await?;
            let prefix = format!("prospective/inputs/v1/{}/", reg.manifest.id());
            Self::verified_hash(&object.name, &value, &prefix)?;
            labels.extend(serde_json::from_value::<Vec<Label>>(
                value["items"].clone(),
            )?);
            if labels.len() > 576 {
                return Err("Too many archived labels".into());
            }
        }
        Ok(labels)
    }
    async fn firestore(&self, body: Value, token: &str) -> Result<Value, Failure> {
        let mut response = self
            .store
            .http
            .post(format!("{}:runQuery", self.documents))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > 262144 {
                return Err("Outcome query exceeds limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    async fn load(&self, reg: Registered, day: NaiveDate, token: &str) -> Result<Input, Failure> {
        let anchor=self.firestore(json!({"structuredQuery":{"from":[{"collectionId":"contracts_btc"}],"select":{"fields":[]},"limit":1}}),token).await?;
        let rows = anchor.as_array().ok_or("Missing read anchor")?;
        let asof = decision::time(&rows.last().ok_or("Empty read anchor")?["readTime"])?;
        reg.validate(asof)?;
        let left = prospective::day_start(day).max(reg.manifest.engineering_start.timestamp());
        let right =
            (prospective::day_start(day) + 86400).min(asof.timestamp().div_euclid(900) * 900);
        if right < left || left >= prospective::day_start(day) + 86400 {
            return Err("Day not available".into());
        }
        let old = self.latest(&reg, day, asof, token).await?;
        let previous_labels = if let Some((_, report)) = &old {
            self.previous_labels(&reg, report, token).await?
        } else {
            Vec::new()
        };
        let start = format!("decisions/v1/s{left}_");
        let end = format!("decisions/v1/s{right}_");
        let inventory = self
            .list("decisions/v1/", Some(&start), Some(&end), 288, asof, token)
            .await?;
        let decisions: Vec<Saved> = stream::iter(inventory.into_iter().map(|object| async move {
            let record = self.read(&object, token).await?;
            Ok::<_, Failure>(Saved {
                object,
                record: prospective::decision_projection(&record),
            })
        }))
        .buffer_unordered(8)
        .try_collect()
        .await?;
        let labels:Vec<Vec<Label>>=stream::iter(Asset::ALL.into_iter().map(|asset|async move{
            let timestamp=|s|DateTime::from_timestamp(s,0).unwrap();
            let result=self.firestore(json!({"readTime":asof,"structuredQuery":{"from":[{"collectionId":format!("contracts_{}",prospective::symbol(asset).to_ascii_lowercase())}],"select":{"fields":[{"fieldPath":"summary"},{"fieldPath":"close_time"}]},"where":{"compositeFilter":{"op":"AND","filters":[{"fieldFilter":{"field":{"fieldPath":"close_time"},"op":"GREATER_THAN","value":{"timestampValue":timestamp(left)}}},{"fieldFilter":{"field":{"fieldPath":"close_time"},"op":"LESS_THAN_OR_EQUAL","value":{"timestampValue":timestamp(right)}}}]}},"limit":97}}),token).await?;
            let rows=result.as_array().ok_or("Invalid outcome query")?;let docs:Vec<_>=rows.iter().filter_map(|r|r.get("document")).collect();if docs.len()>96{return Err("Outcome row limit exceeded".into());}
            docs.into_iter().map(|d|prospective::label_projection(d,asset,day).map_err(Into::into)).collect::<Result<Vec<_>,Failure>>()
        })).buffer_unordered(3).try_collect().await?;
        Ok(Input {
            schema_version: 1,
            experiment: reg,
            day,
            asof,
            decisions,
            labels: labels.into_iter().flatten().collect(),
            previous_asof: old
                .as_ref()
                .map(|(_, r)| decision::time(&r["asof"]))
                .transpose()?,
            previous_report: old.map(|(r, _)| r),
            previous_labels,
        })
    }
    async fn part(
        &self,
        reg: &Registered,
        kind: &str,
        items: Vec<Value>,
        token: &str,
    ) -> Result<Value, Failure> {
        let value = json!({"items":items});
        let object = self
            .create(
                &format!("prospective/inputs/v1/{}", reg.manifest.id()),
                &value,
                token,
            )
            .await?;
        Ok(json!({"kind":kind,"object":object}))
    }
    async fn persist(
        &self,
        input: &Input,
        rows: &[prospective::Row],
        report: &Value,
        token: &str,
    ) -> Result<Value, Failure> {
        let mut header = serde_json::to_value(input)?;
        let mut parts = Vec::new();
        for kind in ["decisions", "labels", "previous_labels"] {
            let items = header[kind]
                .as_array()
                .ok_or("Missing input array")?
                .clone();
            header[kind] = json!([]);
            let mut chunk = Vec::new();
            let mut size = 32;
            for item in items {
                let len = serde_json::to_vec(&item)?.len() + 1;
                if len > PART_LIMIT {
                    return Err("Input record too large".into());
                }
                if size + len > PART_LIMIT {
                    parts.push(
                        self.part(&input.experiment, kind, std::mem::take(&mut chunk), token)
                            .await?,
                    );
                    size = 32;
                }
                size += len;
                chunk.push(item);
            }
            if !chunk.is_empty() {
                parts.push(self.part(&input.experiment, kind, chunk, token).await?);
            }
        }
        parts.push(
            self.part(&input.experiment, "header", vec![header], token)
                .await?,
        );
        for chunk in rows.chunks(48) {
            parts.push(
                self.part(
                    &input.experiment,
                    "rows",
                    chunk
                        .iter()
                        .map(|r| serde_json::to_value(r).unwrap())
                        .collect(),
                    token,
                )
                .await?,
            );
        }
        let root = json!({"schema_version":1,"experiment_id":input.experiment.manifest.id(),"asof":input.asof,"day":input.day,"input_hash":report["input_hash"],"parts":parts});
        let manifest = self
            .create(
                &format!(
                    "prospective/manifests/v1/{}",
                    input.experiment.manifest.id()
                ),
                &root,
                token,
            )
            .await?;
        let mut report = report.clone();
        report["input_manifest"] = serde_json::to_value(manifest)?;
        let prefix = Self::day_prefix(&input.experiment, input.day);
        let object = self
            .create(prefix.trim_end_matches('/'), &report, token)
            .await?;
        Ok(json!({"object":object,"report":report}))
    }
    async fn evaluate_day(&self, day: NaiveDate) -> Result<Value, Failure> {
        let token = self.archive.token().await?;
        let registered = self.registration(&token).await?;
        let input = self.load(registered, day, &token).await?;
        let (input, report, rows) = prospective::evaluate(input)?;
        self.persist(&input, &rows, &report, &token).await
    }
    async fn latest_day(&self, day: NaiveDate) -> Result<Value, Failure> {
        let token = self.archive.token().await?;
        let reg = self.registration(&token).await?;
        match self.latest(&reg, day, Utc::now(), &token).await? {
            Some((object, report)) => Ok(json!({"object":object,"report":report})),
            None => Ok(json!({"status":"not_evaluated","day":day})),
        }
    }
    async fn summary(&self, through: NaiveDate) -> Result<Value, Failure> {
        let token = self.archive.token().await?;
        let reg = self.registration(&token).await?;
        let asof = Utc::now();
        let from =
            (through - chrono::Duration::days(30)).max(reg.manifest.engineering_start.date_naive());
        let mut days = Vec::new();
        let mut day = from;
        while day <= through {
            days.push(day);
            day = day.succ_opt().ok_or("Invalid day")?;
        }
        let reports: Vec<_> = stream::iter(days.into_iter().map(|day| {
            let reg = &reg;
            let token = &token;
            async move { self.latest(reg, day, asof, token).await }
        }))
        .buffer_unordered(6)
        .try_collect::<Vec<_>>()
        .await?
        .into_iter()
        .flatten()
        .collect();
        let mut stats = Vec::new();
        let mut refs = Vec::new();
        for (object, report) in reports {
            let stat: DayStats = serde_json::from_value(report["stats"].clone())?;
            if report["day"] != stat.day.to_string() || report["asof"] != json!(stat.asof) {
                return Err("Invalid summary source".into());
            }
            stats.push(stat);
            refs.push(object);
        }
        stats.sort_by_key(|s| s.day);
        refs.sort_by(|a, b| a.name.cmp(&b.name));
        let mut report = prospective::summarize(&reg, asof, from, through, &stats)?;
        report["daily_reports"] = json!(refs);
        report["daily_support"] = json!(stats);
        let object = self
            .create(
                &format!("prospective/summaries/v1/{}", reg.manifest.id()),
                &report,
                &token,
            )
            .await?;
        Ok(json!({"object":object,"report":report}))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DayRequest {
    pub day: NaiveDate,
}
impl DayRequest {
    pub fn valid(&self, now: DateTime<Utc>) -> bool {
        self.day >= Manifest::v1().engineering_start.date_naive()
            && self.day <= now.date_naive()
            && (now.date_naive() - self.day).num_days() <= 31
    }
}
/// Long report reads yield to the existing opening, history and minute-ten work.
fn admitted(now: DateTime<Utc>) -> bool {
    let s = now.timestamp().rem_euclid(900);
    (210..480).contains(&s) || (690..720).contains(&s)
}
type App = Option<Arc<Service>>;
fn response(result: Result<Value, Failure>) -> Response {
    match result {
        Ok(v) => ([(header::CACHE_CONTROL, "no-store")], Json(v)).into_response(),
        Err(_) => (
            StatusCode::BAD_GATEWAY,
            "Private evaluation unavailable; no report promoted",
        )
            .into_response(),
    }
}
async fn evaluate(State(s): State<App>, Json(q): Json<DayRequest>) -> Response {
    run(s, q, false).await
}
async fn summary(State(s): State<App>, Json(q): Json<DayRequest>) -> Response {
    run(s, q, true).await
}
async fn run(s: App, q: DayRequest, summary: bool) -> Response {
    let now = Utc::now();
    if !q.valid(now) {
        return (StatusCode::BAD_REQUEST, "Choose a supported UTC day").into_response();
    }
    if !admitted(now) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::RETRY_AFTER, "45")],
            "Research capture reserved; retry evaluation outside capture windows",
        )
            .into_response();
    }
    let Some(s) = s else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Evaluator not configured").into_response();
    };
    match tokio::time::timeout(Duration::from_secs(40), async {
        if summary {
            s.summary(q.day).await
        } else {
            s.evaluate_day(q.day).await
        }
    })
    .await
    {
        Ok(r) => response(r),
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            "Evaluation time limit; incomplete parts are not a report",
        )
            .into_response(),
    }
}
async fn latest(State(s): State<App>, Query(q): Query<DayRequest>) -> Response {
    if !q.valid(Utc::now()) {
        return (StatusCode::BAD_REQUEST, "Choose a supported UTC day").into_response();
    }
    let Some(s) = s else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Evaluator not configured").into_response();
    };
    match tokio::time::timeout(Duration::from_secs(8), s.latest_day(q.day)).await {
        Ok(r) => response(r),
        Err(_) => (StatusCode::GATEWAY_TIMEOUT, "Report read timed out").into_response(),
    }
}
pub fn router(service: App) -> Router {
    Router::new()
        .route("/research/evaluate-day", post(evaluate))
        .route("/research/evaluate-summary", post(summary))
        .route("/research/evaluation", get(latest))
        .layer(DefaultBodyLimit::max(8192))
        .with_state(service)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, method, path, query_param},
    };
    fn service(server: &MockServer) -> Service {
        Service {
            archive: Archive::evaluator_test(),
            store: Store {
                http: reqwest::Client::new(),
                base: server.uri(),
                bucket: "test".into(),
            },
            experiment_object: Manifest::v1().object(),
            documents: format!("{}/documents", server.uri()),
        }
    }
    #[test]
    fn report_requests_cannot_choose_forecasts_cutoffs_sources_or_future_days() {
        let now = DateTime::parse_from_rfc3339("2026-11-12T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        for field in [
            "asof",
            "cutoff",
            "asset",
            "model_object",
            "labels",
            "results",
            "input",
            "bucket",
            "source",
        ] {
            assert!(
                serde_json::from_value::<DayRequest>(json!({"day":"2026-11-11",field:"override"}))
                    .is_err()
            );
        }
        assert!(
            !DayRequest {
                day: "2026-11-13".parse().unwrap()
            }
            .valid(now)
        );
        assert!(
            !DayRequest {
                day: "2026-11-10".parse().unwrap()
            }
            .valid(now)
        );
        for offset in [0, 90, 209, 480, 600, 689, 720, 780, 899] {
            assert!(!admitted(
                DateTime::from_timestamp(1794484800 + offset, 0).unwrap()
            ));
        }
        for offset in [210, 300, 479, 690, 719] {
            assert!(admitted(
                DateTime::from_timestamp(1794484800 + offset, 0).unwrap()
            ));
        }
        assert!(Service::verified_hash("other/path", &json!({}), "prospective/reports/").is_err());
    }
    #[tokio::test]
    async fn extraction_pins_generations_and_one_firestore_read_time_with_no_provider_payload() {
        let data = prospective::tests::fixture();
        let server = MockServer::start().await;
        let service = service(&server);
        Mock::given(method("POST"))
            .and(path("/documents:runQuery"))
            .and(body_partial_json(
                json!({"structuredQuery":{"select":{"fields":[]},"limit":1}}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"readTime":data.asof}])))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(query_param(
                "prefix",
                Service::day_prefix(&data.experiment, data.day),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[]})))
            .expect(1)
            .mount(&server)
            .await;
        let metadata:Vec<_>=data.decisions.iter().map(|d|json!({"name":d.object.name,"generation":d.object.generation,"timeCreated":d.object.committed_at})).collect();
        Mock::given(method("GET"))
            .and(query_param("prefix", "decisions/v1/"))
            .and(query_param(
                "startOffset",
                format!("decisions/v1/s{}_", prospective::day_start(data.day)),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":metadata})))
            .expect(1)
            .mount(&server)
            .await;
        for d in &data.decisions {
            Mock::given(method("GET"))
                .and(path(service.store.url(&d.object.name).unwrap().path()))
                .and(query_param("generation", &d.object.generation))
                .and(query_param("alt", "media"))
                .respond_with(ResponseTemplate::new(200).set_body_json(&d.record))
                .expect(1)
                .mount(&server)
                .await;
        }
        for asset in Asset::ALL {
            let docs:Vec<_>=data.labels.iter().filter(|l|l.asset==asset).map(|l|json!({"readTime":data.asof,"document":{"name":l.source_id,"createTime":l.source_version,"updateTime":l.source_version,"fields":{"close_time":{"timestampValue":DateTime::from_timestamp(l.close_ts,0)},"summary":{"stringValue":json!({"asset":l.asset,"ticker":l.ticker,"open_time":DateTime::from_timestamp(l.open_ts,0),"close_time":DateTime::from_timestamp(l.close_ts,0),"result":l.result,"captured_at":l.captured_at}).to_string()}}}})).collect();
            Mock::given(method("POST")).and(path("/documents:runQuery")).and(body_partial_json(json!({"readTime":data.asof,"structuredQuery":{"from":[{"collectionId":format!("contracts_{}",prospective::symbol(asset).to_ascii_lowercase())}],"select":{"fields":[{"fieldPath":"summary"},{"fieldPath":"close_time"}]},"limit":97}})))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!(docs))).expect(1).mount(&server).await;
        }
        let loaded = service
            .load(data.experiment.clone(), data.day, "local-test")
            .await
            .unwrap();
        let (_, actual, _) = prospective::evaluate(loaded).unwrap();
        let (_, expected, _) = prospective::evaluate(data).unwrap();
        assert_eq!(actual, expected);
    }
    #[tokio::test]
    async fn immutable_shards_reconstruct_exact_report_and_keep_label_versions_separate() {
        use std::sync::{
            Mutex,
            atomic::{AtomicU64, Ordering},
        };
        let data = prospective::tests::fixture();
        let (input, report, rows) = prospective::evaluate(data).unwrap();
        let server = MockServer::start().await;
        let service = service(&server);
        let writes = Arc::new(Mutex::new(BTreeMap::<String, Value>::new()));
        let captured = writes.clone();
        let generation = AtomicU64::new(100);
        let committed = input.asof + chrono::Duration::seconds(1);
        Mock::given(method("POST")).and(path("/upload/storage/v1/b/test/o")).and(query_param("ifGenerationMatch","0"))
            .respond_with(move|r:&wiremock::Request|{let name=r.url.query_pairs().find(|(k,_)|k=="name").unwrap().1.into_owned();let value:Value=serde_json::from_slice(&r.body).unwrap();assert!(r.body.len()<65536);captured.lock().unwrap().insert(name.clone(),value);ResponseTemplate::new(200).set_body_json(json!({"name":name,"generation":generation.fetch_add(1,Ordering::SeqCst).to_string(),"timeCreated":committed}))}).mount(&server).await;
        let result = service
            .persist(&input, &rows, &report, "local-test")
            .await
            .unwrap();
        let saved = writes.lock().unwrap();
        let report_name = result["object"]["name"].as_str().unwrap();
        assert_eq!(saved[report_name], result["report"]);
        let manifest = &saved[result["report"]["input_manifest"]["name"].as_str().unwrap()];
        let mut rebuilt = Value::Null;
        for part in manifest["parts"].as_array().unwrap() {
            if part["kind"] == "header" {
                rebuilt = saved[part["object"]["name"].as_str().unwrap()]["items"][0].clone();
            }
        }
        for kind in ["decisions", "labels", "previous_labels"] {
            let values: Vec<_> = manifest["parts"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["kind"] == kind)
                .flat_map(|p| {
                    saved[p["object"]["name"].as_str().unwrap()]["items"]
                        .as_array()
                        .unwrap()
                        .clone()
                })
                .collect();
            rebuilt[kind] = json!(values);
        }
        let (_, replayed, _) =
            prospective::evaluate(serde_json::from_value(rebuilt).unwrap()).unwrap();
        assert_eq!(replayed, report);
        assert!(saved.keys().all(|k| k.starts_with("prospective/inputs/")
            || k.starts_with("prospective/manifests/")
            || k.starts_with("prospective/reports/")));
    }
    #[tokio::test]
    async fn routes_reject_unknown_fields_and_large_payloads_without_cloud_calls() {
        let app = router(None);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let http = reqwest::Client::new();
        for route in ["evaluate-day", "evaluate-summary"] {
            let response = http
                .post(format!("http://{addr}/research/{route}"))
                .json(&json!({"day":"2026-11-11","cutoff":"future"}))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
        let response = http
            .post(format!("http://{addr}/research/evaluate-day"))
            .header("Content-Type", "application/json")
            .body("x".repeat(9000))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let response = http
            .get(format!(
                "http://{addr}/research/evaluation?day=2026-11-11&asset=BTC"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        task.abort();
    }
}
