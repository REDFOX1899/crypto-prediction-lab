//! Trusted host. Only the pure projected packet reaches the numerical strategy.
use crate::{assets::Asset, checkpoint::Phase, decision, outcomes::Archive};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
type Failure = Box<dyn std::error::Error + Send + Sync>;
pub const BUCKET: &str = "crypto-prediction-lab-research-308289525742";

pub struct Stored {
    pub value: Value,
    pub created_at: DateTime<Utc>,
    pub generation: String,
}
pub(crate) struct Store {
    pub(crate) http: reqwest::Client,
    pub(crate) base: String,
    pub(crate) bucket: String,
}
impl Store {
    fn url(&self, name: &str) -> Result<reqwest::Url, Failure> {
        let mut url =
            reqwest::Url::parse(&format!("{}/storage/v1/b/{}/o", self.base, self.bucket))?;
        url.path_segments_mut()
            .map_err(|_| "Invalid store URL")?
            .push(name);
        Ok(url)
    }
    async fn json(response: reqwest::Response) -> Result<Value, Failure> {
        let mut response = response.error_for_status()?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > 65_536 {
                return Err("Private record exceeds limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    pub(crate) async fn get(&self, name: &str, token: &str) -> Result<Option<Stored>, Failure> {
        let url = self.url(name)?;
        let response = self.http.get(url.clone()).bearer_auth(token).send().await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let meta = Self::json(response).await?;
        let generation = meta["generation"]
            .as_str()
            .ok_or("Missing object generation")?
            .to_owned();
        let created_at = decision::time(&meta["timeCreated"])?;
        let value = Self::json(
            self.http
                .get(url)
                .query(&[("alt", "media"), ("generation", &generation)])
                .bearer_auth(token)
                .send()
                .await?,
        )
        .await?;
        Ok(Some(Stored {
            value,
            created_at,
            generation,
        }))
    }
    async fn create(&self, name: &str, value: &Value, token: &str) -> Result<Stored, Failure> {
        let bytes = serde_json::to_vec(value)?;
        if bytes.len() > 65_536 {
            return Err("Private record exceeds limit".into());
        }
        let response = self
            .http
            .post(format!(
                "{}/upload/storage/v1/b/{}/o",
                self.base, self.bucket
            ))
            .query(&[
                ("uploadType", "media"),
                ("name", name),
                ("ifGenerationMatch", "0"),
            ])
            .header("Content-Type", "application/json")
            .bearer_auth(token)
            .body(bytes)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::PRECONDITION_FAILED {
            return self
                .get(name, token)
                .await?
                .ok_or("Private create conflict without record".into());
        }
        let meta = Self::json(response).await?;
        Ok(Stored {
            value: value.clone(),
            created_at: decision::time(&meta["timeCreated"])?,
            generation: meta["generation"]
                .as_str()
                .ok_or("Missing object generation")?
                .into(),
        })
    }
}

pub struct Runner {
    archive: Arc<Archive>,
    store: Store,
    forecast_object: Option<String>,
    forecast_model: tokio::sync::OnceCell<Option<Stored>>,
}
impl Runner {
    /// Preload once before listening. Failed/unconfigured models remain explicitly
    /// unavailable for this process; collection and the midpoint benchmark continue.
    pub async fn warm_forecast(&self) {
        self.forecast_model
            .get_or_init(|| async {
                let name = self.forecast_object.as_deref()?;
                let load = async {
                    let token = self.archive.token().await.ok()?;
                    self.store.get(name, &token).await.ok().flatten()
                };
                tokio::time::timeout(Duration::from_secs(7), load)
                    .await
                    .ok()
                    .flatten()
            })
            .await;
    }
    /// Operator-only issuance. Reconstruct with the original host cutoff, never now.
    pub async fn issue_mcp_scope(&self, asset: Asset, open: i64) -> Result<Value, Failure> {
        let token = self.archive.token().await?;
        let stored = self
            .store
            .get(&Self::name("decisions", asset, open), &token)
            .await?
            .ok_or("No committed decision")?;
        let value = &stored.value;
        let cutoff = decision::time(&value["cutoff"])?;
        if value["status"] != "forecast"
            || value["run_id"] != decision::run_id(asset, open)
            || stored.created_at.timestamp() >= open + 40
            || cutoff > stored.created_at
            || cutoff.timestamp() < open
            || cutoff.timestamp() >= open + 40
        {
            return Err("No timely forecast packet".into());
        }
        let doc = self
            .archive
            .cycle_document(asset, open, Phase::Opening)
            .await?
            .ok_or("Missing opening")?;
        let packet = decision::packet(&doc, asset, open, cutoff, Phase::Opening)?;
        if value["packet"] != packet || value["packet_hash"] != decision::hash(&packet) {
            return Err("Packet integrity failure".into());
        }
        let now = Utc::now();
        let mut scope = crate::research_mcp::Scope::new(packet.clone(), now)?;
        if !value["market_rules"].is_null() {
            let rules = crate::market_rules::project(&doc, &packet)?;
            if value["market_rules"] != rules || value["rules_hash"] != decision::hash(&rules) {
                return Err("Rule integrity failure".into());
            }
            scope.set_rules(rules)?;
        }
        if !value["forecast_proof"].is_null() {
            if value["forecast_hash"] != decision::hash(&value["forecast_proof"]) {
                return Err("Forecast integrity failure".into());
            }
            let proof: crate::forecast::Frozen =
                serde_json::from_value(value["forecast_proof"].clone())?;
            scope.set_forecast(proof)?;
        }
        let mut random = [0u8; 32];
        getrandom::fill(&mut random).map_err(|_| "Capability generation failed")?;
        use base64::Engine;
        let capability = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random);
        let name = crate::research_mcp::scope_name(&capability).ok_or("Invalid capability")?;
        let saved = self
            .store
            .create(&name, &serde_json::to_value(&scope)?, &token)
            .await?;
        if saved.value != serde_json::to_value(&scope)? {
            return Err("Scope collision".into());
        }
        Ok(
            json!({"capability":capability,"expires_at":scope.expires_at,
            "run_id":scope.run_id,"asset":asset,"cutoff":cutoff,"packet_hash":scope.packet_hash,
            "scope":scope,"transport":"private Streamable HTTP; X-Research-Capability header plus Cloud Run IAM"}),
        )
    }
    pub fn from_env(archive: Arc<Archive>) -> Result<Option<Arc<Self>>, Failure> {
        let Ok(bucket) = std::env::var("DECISION_BUCKET") else {
            return Ok(None);
        };
        if bucket != BUCKET {
            return Err("Invalid app-private decision bucket".into());
        }
        let forecast_object = std::env::var("FORECAST_MODEL_OBJECT").ok();
        if forecast_object
            .as_deref()
            .is_some_and(|s| !crate::forecast::valid_object(s))
        {
            return Err("Invalid forecast model object".into());
        }
        Ok(Some(Arc::new(Self {
            archive,
            forecast_object,
            forecast_model: tokio::sync::OnceCell::new(),
            store: Store {
                http: reqwest::Client::builder()
                    .timeout(Duration::from_secs(3))
                    .build()?,
                base: "https://storage.googleapis.com".into(),
                bucket,
            },
        })))
    }
    fn name(kind: &str, asset: Asset, open: i64) -> String {
        format!("{kind}/v1/{}.json", decision::run_id(asset, open))
    }

    async fn decide_asset(&self, asset: Asset, open: i64) -> Result<Value, Failure> {
        let token = self.archive.token().await?;
        let name = Self::name("decisions", asset, open);
        if let Some(existing) = self.store.get(&name, &token).await? {
            return Ok(Self::summary(existing));
        }
        let cutoff = Utc::now();
        let doc = self
            .archive
            .cycle_document(asset, open, Phase::Opening)
            .await?;
        // The pure strategy gets only its allowlisted projection. It cannot
        // access the archive, execution candidate, outcomes, or external tools.
        let mut value = decision::decide(doc.as_ref(), asset, open, cutoff);
        if value["status"] == "forecast" {
            match crate::market_rules::project(
                doc.as_ref().expect("forecast source"),
                &value["packet"],
            ) {
                Ok(rules) => {
                    value["rules_hash"] = json!(decision::hash(&rules));
                    value["market_rules"] = rules;
                }
                Err(reason) => {
                    value["market_rules"] = Value::Null;
                    value["rules_hash"] = Value::Null;
                    value["rules_reason"] = json!(reason);
                }
            }
        }
        if value["status"] == "forecast" {
            let forecast = self
                .forecast_model
                .get()
                .and_then(Option::as_ref)
                .zip(self.forecast_object.as_ref())
                .ok_or("model_unavailable")
                .and_then(|(stored, name)| {
                    crate::forecast::Frozen::new(
                        stored.value.clone(),
                        name.clone(),
                        stored.generation.clone(),
                        stored.created_at,
                        &value["packet"],
                    )
                });
            match forecast {
                Ok(proof) => {
                    let proof = serde_json::to_value(proof)?;
                    value["forecast_hash"] = json!(decision::hash(&proof));
                    value["forecast_proof"] = proof;
                }
                Err(reason) => value["forecast_reason"] = json!(reason),
            }
        }
        value["runtime_revision"] =
            json!(std::env::var("K_REVISION").unwrap_or_else(|_| "local".into()));
        Ok(Self::summary(
            self.store.create(&name, &value, &token).await?,
        ))
    }
    async fn link_asset(&self, asset: Asset, open: i64) -> Result<Value, Failure> {
        let token = self.archive.token().await?;
        let name = Self::name("links", asset, open);
        if let Some(existing) = self.store.get(&name, &token).await? {
            return Ok(Self::summary(existing));
        }
        let Some(decision) = self
            .store
            .get(&Self::name("decisions", asset, open), &token)
            .await?
        else {
            return Ok(json!({"asset":asset,"status":"pending","reason":"missing_decision"}));
        };
        let candidate = self
            .archive
            .cycle_document(asset, open, Phase::ExecutionCandidate)
            .await?;
        if candidate.is_none() && Utc::now().timestamp() < open + 55 {
            return Ok(json!({"asset":asset,"status":"pending","reason":"candidate_not_due"}));
        }
        let mut value = decision::link(
            &decision.value,
            decision.created_at,
            candidate.as_ref(),
            asset,
            open,
            Utc::now(),
        );
        value["decision_generation"] = json!(decision.generation);
        Ok(Self::summary(
            self.store.create(&name, &value, &token).await?,
        ))
    }
    fn summary(stored: Stored) -> Value {
        let late = stored.value["status"] == "forecast"
            && stored.created_at.timestamp() >= stored.value["open_ts"].as_i64().unwrap_or(0) + 40;
        json!({"run_id":stored.value["run_id"],"status":if late {json!("late_forecast")}else{stored.value["status"].clone()},
            "reason":stored.value["reason"],"committed_at":stored.created_at,"generation":stored.generation})
    }
    pub async fn decide_cycle(&self, open: i64) -> Value {
        let results =
            futures_util::future::join_all(Asset::ALL.into_iter().map(|asset| async move {
                match self.decide_asset(asset, open).await {
                    Ok(v) => v,
                    Err(_) => json!({"asset":asset,"status":"runner_error"}),
                }
            }))
            .await;
        json!({"decisions":results})
    }
    pub async fn link_cycle(&self, open: i64) -> Value {
        let results =
            futures_util::future::join_all(Asset::ALL.into_iter().map(|asset| async move {
                match self.link_asset(asset, open).await {
                    Ok(v) => v,
                    Err(_) => json!({"asset":asset,"status":"link_error"}),
                }
            }))
            .await;
        json!({"links":results})
    }
    pub async fn review(&self, asset: Asset, open: i64) -> Result<Value, Failure> {
        let token = self.archive.token().await?;
        let mut result = json!({"asset":asset,"open_ts":open,"decision":null,"link":null});
        for (kind, field) in [("decisions", "decision"), ("links", "link")] {
            if let Some(record) = self
                .store
                .get(&Self::name(kind, asset, open), &token)
                .await?
            {
                result[field] = json!({"record":record.value,"committed_at":record.created_at,"generation":record.generation});
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };
    #[tokio::test]
    async fn conditional_create_retains_first_decision_and_pins_generation_on_read() {
        let server = MockServer::start().await;
        let first = json!({"p_yes":"0.4250","packet_hash":"test"});
        let meta = json!({"generation":"123","timeCreated":"2026-11-10T20:00:27Z"});
        Mock::given(method("POST"))
            .and(path("/upload/storage/v1/b/test/o"))
            .and(query_param("ifGenerationMatch", "0"))
            .and(query_param("name", "decisions/v1/test.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&meta))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/upload/storage/v1/b/test/o"))
            .and(query_param("ifGenerationMatch", "0"))
            .respond_with(ResponseTemplate::new(412))
            .with_priority(10)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(|r: &wiremock::Request| r.url.query().is_none())
            .respond_with(ResponseTemplate::new(200).set_body_json(&meta))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(query_param("alt", "media"))
            .and(query_param("generation", "123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&first))
            .expect(1)
            .mount(&server)
            .await;
        let store = Store {
            http: reqwest::Client::new(),
            base: server.uri(),
            bucket: "test".into(),
        };
        let created = store
            .create("decisions/v1/test.json", &first, "local-test")
            .await
            .unwrap();
        let duplicate = store
            .create(
                "decisions/v1/test.json",
                &json!({"p_yes":"1"}),
                "local-test",
            )
            .await
            .unwrap();
        assert_eq!(created.value, duplicate.value);
        assert_eq!(created.generation, duplicate.generation);
        assert_eq!(created.created_at, duplicate.created_at);
    }
    #[tokio::test]
    async fn oversized_private_record_is_rejected_before_network_write() {
        let server = MockServer::start().await;
        let store = Store {
            http: reqwest::Client::new(),
            base: server.uri(),
            bucket: "test".into(),
        };
        assert!(
            store
                .create("test", &json!({"data":"x".repeat(65_536)}), "local-test")
                .await
                .is_err()
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
