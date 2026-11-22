//! Shared news collection runs only on the private collector.
//! Supports optional single-pass Gemini Decision 1 batch relevance & catalyst scoring.
use crate::{assets::Asset, outcomes::Archive};
use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const TTL: i64 = 14 * 86400;
const COINDESK: &str = "https://www.coindesk.com/arc/outboundfeeds/rss";
const FED: &str = "https://www.federalreserve.gov/feeds/press_monetary.xml";
const DECISIONS_URL: &str = "https://generativelanguage.googleapis.com/v1beta/decisions";

type Failure = &'static str;

#[derive(Clone, Serialize, Deserialize)]
struct Story {
    title: String,
    url: String,
    publisher: String,
    published_at: Option<i64>,
    via: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct NewsCatalyst {
    pub net_tone: f64,
    pub relevant_count: usize,
    pub bullish_count: usize,
    pub bearish_count: usize,
    pub avg_impact: f64,
    pub checked_at: Option<i64>,
    pub scored_by: String,
    pub top_headlines: Vec<String>,
}

fn is_seo_spam(url: &str, title: &str) -> bool {
    let lower_url = url.to_lowercase();
    let lower_title = title.to_lowercase();
    [
        "/convert/",
        "/converter",
        "/calculator",
        "/price/",
        "/prices/",
        "/glossary/",
        "/learn/",
    ]
    .iter()
    .any(|p| lower_url.contains(p))
        || [
            "converter",
            "price today",
            "live price",
            "price prediction",
            "by 2030",
            "by 2028",
            "could be worth",
        ]
        .iter()
        .any(|w| lower_title.contains(w))
}

fn normalized(raw: &str, title: &str, date: Option<&str>, via: &str, now: i64) -> Option<Story> {
    let mut url = Url::parse(raw.trim()).ok()?;
    if url.scheme() != "https"
        || url.domain().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || raw.len() > 1024
    {
        return None;
    }
    url.set_fragment(None);
    let publisher = url.domain()?.to_lowercase();
    let params: Vec<_> = url
        .query_pairs()
        .filter(|(k, _)| !k.starts_with("utm_") && !matches!(k.as_ref(), "fbclid" | "gclid"))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.set_query(None);
    if !params.is_empty() {
        url.query_pairs_mut().extend_pairs(params);
    }
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() || title.chars().count() > 300 {
        return None;
    }
    let clean_url: String = url.into();
    if is_seo_spam(&clean_url, &title) {
        return None;
    }
    let published_at = date
        .and_then(|s| {
            DateTime::parse_from_rfc2822(s)
                .or_else(|_| DateTime::parse_from_rfc3339(s))
                .ok()
        })
        .map(|t| t.timestamp());
    if published_at.is_some_and(|t| t > now || t < now - TTL) {
        return None;
    }
    Some(Story {
        title,
        url: clean_url,
        publisher,
        published_at,
        via: via.to_owned(),
    })
}

async fn body(request: reqwest::RequestBuilder, limit: usize) -> Result<Vec<u8>, Failure> {
    let mut response = request.send().await.map_err(|_| "provider_unavailable")?;
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 | 403 => "access_denied",
            429 | 432 | 433 => "quota_or_rate_limit",
            _ => "provider_unavailable",
        });
    }
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "provider_unavailable")? {
        if data.len() + chunk.len() > limit {
            return Err("response_too_large");
        }
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}

async fn rss(client: &Client, endpoint: &str, via: &str) -> Result<Vec<Story>, Failure> {
    let data = body(client.get(endpoint), 512 * 1024).await?;
    let xml = std::str::from_utf8(&data)
        .map_err(|_| "invalid_response")?
        .trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse(xml).map_err(|_| "invalid_response")?;
    if !doc.descendants().any(|n| n.has_tag_name("channel")) {
        return Err("invalid_response");
    }
    let now = Utc::now().timestamp();
    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("item"))
        .take(20)
        .filter_map(|n| {
            let field = |name: &str| {
                n.children()
                    .find(|c| c.has_tag_name(name))
                    .and_then(|c| c.text())
            };
            normalized(field("link")?, field("title")?, field("pubDate"), via, now)
        })
        .collect())
}

#[derive(Deserialize)]
struct SearchResult {
    url: String,
    title: String,
    published_date: Option<String>,
}
#[derive(Deserialize)]
struct Search {
    results: Vec<SearchResult>,
}

async fn tavily(client: &Client, search: bool) -> Result<Vec<Story>, Failure> {
    if !search {
        return Err("scheduled_skip");
    }
    let key = env::var("TAVILY_API_KEY_FILE")
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .or_else(|| env::var("TAVILY_API_KEY").ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .ok_or("key_unavailable")?;
    let request = client.post("https://api.tavily.com/search").bearer_auth(key).json(&serde_json::json!({
        "query":"Bitcoin BTC Ethereum Solana ETF flows liquidations Federal Reserve crypto market breaking news",
        "topic":"news","search_depth":"basic","time_range":"day","max_results":14,
        "include_domains":["reuters.com","coindesk.com","blockworks.co","blockworks.com","theblock.co","decrypt.co","cointelegraph.com"],
        "include_answer":false,"include_raw_content":false,"auto_parameters":false
    }));
    let rows: Search = serde_json::from_slice(&body(request, 128 * 1024).await?)
        .map_err(|_| "invalid_response")?;
    let now = Utc::now().timestamp();
    Ok(rows
        .results
        .into_iter()
        .take(14)
        .filter_map(|r| normalized(&r.url, &r.title, r.published_date.as_deref(), "Tavily", now))
        .collect())
}

fn append(
    rows: Result<Vec<Story>, Failure>,
    stories: &mut Vec<Story>,
    seen: &mut HashSet<String>,
) -> String {
    match rows {
        Err(reason) => reason.into(),
        Ok(rows) => {
            stories.extend(rows.into_iter().filter(|r| seen.insert(r.url.clone())));
            "ok".into()
        }
    }
}

fn tagged(story: Story) -> Value {
    let title = story.title.to_lowercase();
    let positive = [
        "inflow",
        "approv",
        "rally",
        "rallies",
        "surge",
        "gain",
        "record high",
        "adoption",
        "upgrade",
        "launch",
        "rebound",
    ];
    let negative = [
        "outflow",
        "hack",
        "exploit",
        "plunge",
        "drop",
        "selloff",
        "sell-off",
        "loss",
        "declin",
        "lawsuit",
        "outage",
        "liquidat",
        "shut down",
        "ban",
    ];
    let up: Vec<_> = positive.into_iter().filter(|w| title.contains(w)).collect();
    let down: Vec<_> = negative.into_iter().filter(|w| title.contains(w)).collect();
    let (tone, reason) = match (up.is_empty(), down.is_empty()) {
        (false, true) => (
            "positive",
            format!("Positive headline cues: {}.", up.join(", ")),
        ),
        (true, false) => (
            "negative",
            format!("Negative headline cues: {}.", down.join(", ")),
        ),
        (false, false) => (
            "mixed",
            "Both positive and negative headline cues appear.".into(),
        ),
        _ => ("mixed", "Direction is unclear from the headline.".into()),
    };
    let assets: Vec<_> = [
        ("BTC", ["bitcoin", "btc"]),
        ("ETH", ["ethereum", "ether"]),
        ("SOL", ["solana", "sol "]),
    ]
    .into_iter()
    .filter(|(_, words)| words.iter().any(|w| title.contains(w)))
    .map(|(a, _)| a)
    .collect();
    let mut row = serde_json::to_value(story).expect("story serializes");
    row["tone"] = json!(tone);
    row["reason"] = json!(reason);
    row["assets"] = json!(assets);
    row["scored_by"] = json!("keyword_fallback");
    row
}

fn load_gemini_key() -> Option<String> {
    env::var("GEMINI_API_KEY_FILE")
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .or_else(|| env::var("GEMINI_API_KEY").ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.contains('\n'))
}

/// Scores up to 16 headlines in ONE single-pass Gemini Decision 1 call (~300-450ms, 0 output tokens).
async fn score_with_gemini_decisions(
    client: &Client,
    endpoint: &str,
    rows: &mut [Value],
) -> Option<(u64, usize)> {
    let key = load_gemini_key()?;
    if rows.is_empty() {
        return None;
    }
    let count = rows.len().min(16);
    let mut lines = Vec::with_capacity(count);
    let mut questions = serde_json::Map::new();
    for (i, r) in rows.iter().take(count).enumerate() {
        let title = r["title"].as_str().unwrap_or("");
        let publisher = r["publisher"].as_str().unwrap_or("");
        let via = r["via"].as_str().unwrap_or("");
        lines.push(format!("[{i}] ({publisher}, via {via}) {title}"));
        questions.insert(
            format!("s{i}_rel"),
            json!({
                "type": "noul",
                "instructions": format!("Is headline [{i}] genuine crypto/macro market news relevant to short-term BTC, ETH, or SOL price action (and NOT an SEO converter, glossary, or unrelated general news)?")
            }),
        );
        questions.insert(
            format!("s{i}_dir"),
            json!({
                "type": "choice",
                "instructions": format!("What is the immediate short-term price catalyst direction of headline [{i}] for crypto markets?"),
                "criteria": {
                    "bullish": "Positive catalyst: ETF inflows, institutional buying, dovish macro, upgrade, regulatory approval.",
                    "bearish": "Negative catalyst: outflows, exploit, liquidations, hawkish macro, regulatory enforcement.",
                    "neutral": "Informational, mixed, off-topic, or no clear short-term directional catalyst."
                }
            }),
        );
        questions.insert(
            format!("s{i}_imp"),
            json!({
                "type": "score",
                "instructions": format!("Rate headline [{i}]'s potential to move 15-minute crypto markets."),
                "criteria": [
                    "Noise: off-topic, SEO spam, or zero short-term impact.",
                    "Context: background market news with modest impact.",
                    "Catalyst: breaking macro, flow, or liquidation event with strong short-term impact."
                ]
            }),
        );
    }
    let payload = json!({
        "model": "gemini-decision-1",
        "input": [{"type": "text", "text": lines.join("\n")}],
        "questions": questions
    });
    let started = Instant::now();
    let resp = client
        .post(endpoint)
        .header("x-goog-api-key", key)
        .timeout(Duration::from_secs(4))
        .json(&payload)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed: Value = resp.json().await.ok()?;
    let answers = parsed.get("answers")?.as_object()?;
    let latency_ms = started.elapsed().as_millis() as u64;
    let mut filtered_noise = 0usize;
    for (i, row) in rows.iter_mut().take(count).enumerate() {
        let rel = answers
            .get(&format!("s{i}_rel"))
            .and_then(|v| v["noul"].as_f64());
        let dir = answers
            .get(&format!("s{i}_dir"))
            .and_then(|v| v["choice"].as_str());
        let imp = answers
            .get(&format!("s{i}_imp"))
            .and_then(|v| v["score"].as_f64());
        if let (Some(rel), Some(dir), Some(imp)) = (rel, dir, imp) {
            let tone = match dir {
                "bullish" => "positive",
                "bearish" => "negative",
                _ => "mixed",
            };
            let is_noise = rel < 0.45;
            if is_noise {
                filtered_noise += 1;
            }
            row["tone"] = json!(tone);
            row["relevance"] = json!((rel * 1000.0).round() / 1000.0);
            row["impact_score"] = json!((imp * 100.0).round() / 100.0);
            row["catalyst_direction"] = json!(dir);
            row["is_noise"] = json!(is_noise);
            row["scored_by"] = json!("gemini-decision-1");
            row["reason"] = json!(format!(
                "Gemini Decision 1: {} · relevance {:.0}% · impact {:.2}/2.0.",
                dir,
                rel * 100.0,
                imp
            ));
        }
    }
    Some((latency_ms, filtered_noise))
}

pub struct Service {
    archive: Option<Arc<Archive>>,
    cache: Mutex<Option<(Instant, Value)>>,
}

impl Service {
    pub fn new(archive: Option<Arc<Archive>>) -> Arc<Self> {
        Arc::new(Self {
            archive,
            cache: Mutex::new(None),
        })
    }

    pub(crate) async fn board(&self) -> Value {
        let mut cache = self.cache.lock().await;
        if let Some((at, value)) = &*cache
            && at.elapsed() < Duration::from_secs(60)
        {
            return value.clone();
        }
        let data = match &self.archive {
            Some(a) => match a.news_document("latest").await {
                Ok(Some(d)) => {
                    let mut payload = d
                        .get("payload")
                        .cloned()
                        .unwrap_or(json!({"status":"unavailable"}));
                    // Filter legacy SEO converter URLs on read too so live UI stays clean immediately.
                    if let Some(stories) = payload.get_mut("stories").and_then(Value::as_array_mut)
                    {
                        let before = stories.len();
                        stories.retain(|r| {
                            let url = r["url"].as_str().unwrap_or("");
                            let title = r["title"].as_str().unwrap_or("");
                            !is_seo_spam(url, title) && !r["is_noise"].as_bool().unwrap_or(false)
                        });
                        let dropped = before.saturating_sub(stories.len());
                        if dropped > 0 && payload.get("filtered_noise").is_none() {
                            payload["filtered_noise"] = json!(dropped);
                        }
                    }
                    payload
                }
                _ => json!({"status":"unavailable"}),
            },
            None => json!({"status":"not_configured"}),
        };
        *cache = Some((Instant::now(), data.clone()));
        data
    }

    pub(crate) async fn latest_catalyst(&self, asset: Asset) -> NewsCatalyst {
        let board = self.board().await;
        let sym = asset.code();
        let checked_at = board["checked_at"].as_i64();
        let scored_by = board["news_scorer"]
            .as_str()
            .unwrap_or("headline_cues")
            .to_owned();
        let Some(stories) = board["stories"].as_array() else {
            return NewsCatalyst {
                checked_at,
                scored_by,
                ..Default::default()
            };
        };
        let mut bullish = 0usize;
        let mut bearish = 0usize;
        let mut relevant = 0usize;
        let mut impact_sum = 0.0_f64;
        let mut headlines = Vec::new();
        for st in stories {
            if st["is_noise"].as_bool().unwrap_or(false) {
                continue;
            }
            let assets = st["assets"].as_array();
            let matches_asset =
                assets.is_none_or(|a| a.is_empty() || a.iter().any(|x| x.as_str() == Some(sym)));
            if !matches_asset {
                continue;
            }
            relevant += 1;
            let tone = st["tone"].as_str().unwrap_or("mixed");
            if tone == "positive" {
                bullish += 1;
            } else if tone == "negative" {
                bearish += 1;
            }
            impact_sum += st["impact_score"].as_f64().unwrap_or(0.6);
            if headlines.len() < 4
                && let Some(t) = st["title"].as_str()
            {
                headlines.push(t.to_owned());
            }
        }
        let denom = (bullish + bearish).max(1) as f64;
        let net_tone = (bullish as f64 - bearish as f64) / denom;
        let avg_impact = if relevant > 0 {
            impact_sum / relevant as f64
        } else {
            0.0
        };
        NewsCatalyst {
            net_tone,
            relevant_count: relevant,
            bullish_count: bullish,
            bearish_count: bearish,
            avg_impact,
            checked_at,
            scored_by,
            top_headlines: headlines,
        }
    }

    async fn collect(&self) -> Result<Value, Failure> {
        let a = self.archive.as_ref().ok_or("news_not_configured")?;
        let config = a
            .news_document("pilot")
            .await
            .map_err(|_| "pilot_read_failed")?
            .ok_or("pilot_disabled")?;
        let start = config["payload"]["start"].as_i64().ok_or("invalid_pilot")?;
        let now = Utc::now().timestamp();
        let slot = now.div_euclid(900) * 900;
        if start <= 0 || start % 900 != 0 || !(start..start + TTL).contains(&slot) {
            return Ok(json!({"status":"pilot_finished"}));
        }
        if !a.news_claim(slot).await.map_err(|_| "claim_failed")? {
            return Ok(json!({"status":"already_attempted"}));
        }
        let search = ((slot - start) / 900) % 3 != 2;
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(8))
            .user_agent("YesNoTape/0.2 (market news)")
            .build()
            .map_err(|_| "http_config")?;
        let (news, desk, monetary) = tokio::join!(
            tavily(&client, search),
            rss(&client, COINDESK, "CoinDesk RSS"),
            rss(&client, FED, "Federal Reserve RSS")
        );
        let mut stories = Vec::new();
        let mut seen = HashSet::new();
        let tavily = append(news, &mut stories, &mut seen);
        let coindesk = append(desk, &mut stories, &mut seen);
        let fed = append(monetary, &mut stories, &mut seen);
        let prior = a
            .news_document("latest")
            .await
            .map_err(|_| "prior_read_failed")?;
        let mut rows: Vec<Value> = stories
            .into_iter()
            .map(tagged)
            .map(|mut r| {
                r["first_seen_at"] = json!(now);
                r
            })
            .collect();
        let mut prior_by_url: HashMap<String, Value> = HashMap::new();
        if let Some(p) = &prior
            && let Some(old) = p["payload"]["stories"].as_array()
        {
            for item in old {
                if let Some(u) = item["url"].as_str() {
                    prior_by_url.insert(u.to_owned(), item.clone());
                }
            }
        }
        let urls: HashSet<_> = rows
            .iter()
            .filter_map(|r| r["url"].as_str().map(str::to_owned))
            .collect();
        for (url, item) in prior_by_url {
            if !urls.contains(&url) {
                rows.push(item);
            }
        }
        rows.retain(|r| {
            let url = r["url"].as_str().unwrap_or("");
            let title = r["title"].as_str().unwrap_or("");
            r["first_seen_at"]
                .as_i64()
                .is_some_and(|t| t >= now - 86400)
                && r["published_at"].as_i64().is_none_or(|t| t >= now - 86400)
                && !is_seo_spam(url, title)
        });
        rows.sort_by_key(|r| std::cmp::Reverse(r["published_at"].as_i64().unwrap_or(0)));
        rows.truncate(16);

        // Single-pass Gemini Decision 1 scoring across all headlines when key is present!
        let mut news_scorer = "keyword_fallback";
        let mut scoring_latency_ms: Option<u64> = None;
        let mut filtered_noise = 0usize;
        if let Some((lat, dropped)) =
            score_with_gemini_decisions(&client, DECISIONS_URL, &mut rows).await
        {
            news_scorer = "gemini-decision-1";
            scoring_latency_ms = Some(lat);
            filtered_noise = dropped;
            rows.retain(|r| !r["is_noise"].as_bool().unwrap_or(false));
        }

        let report = json!({
            "status": "ok",
            "checked_at": Utc::now().timestamp(),
            "slot": slot,
            "pilot_end": start + TTL,
            "tavily": tavily,
            "coindesk": coindesk,
            "fed": fed,
            "news_scorer": news_scorer,
            "scoring_latency_ms": scoring_latency_ms,
            "filtered_noise": filtered_noise,
            "stories": rows
        });
        a.news_publish(
            slot,
            &report,
            prior.as_ref().and_then(|p| p["updateTime"].as_str()),
        )
        .await
        .map_err(|_| "save_failed")?;
        *self.cache.lock().await = None;
        Ok(json!({
            "status": "saved",
            "slot": slot,
            "tavily": tavily,
            "news_scorer": news_scorer,
            "scoring_latency_ms": scoring_latency_ms,
            "filtered_noise": filtered_noise,
            "stories": rows.len()
        }))
    }
}

pub fn public_router(service: Arc<Service>) -> Router {
    Router::new()
        .route(
            "/api/news",
            get(|State(s): State<Arc<Service>>| async move {
                (
                    [(header::CACHE_CONTROL, "public, max-age=60")],
                    Json(s.board().await),
                )
            }),
        )
        .with_state(service)
}

pub fn private_router(service: Arc<Service>) -> Router {
    Router::new()
        .route(
            "/collect-news",
            post(|State(s): State<Arc<Service>>| async move {
                match tokio::time::timeout(Duration::from_secs(25), s.collect()).await {
                    Ok(Ok(v)) => Json(v).into_response(),
                    _ => (StatusCode::SERVICE_UNAVAILABLE, "News check failed").into_response(),
                }
            }),
        )
        .with_state(service)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn public_router_has_no_collection_or_write_surface() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, public_router(Service::new(None)))
                .await
                .unwrap()
        });
        let client = Client::new();
        assert_eq!(
            client
                .post(format!("{base}/api/news"))
                .send()
                .await
                .unwrap()
                .status(),
            405
        );
        assert_eq!(
            client
                .post(format!("{base}/collect-news"))
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
        assert_eq!(
            client
                .get(format!("{base}/api/news"))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()["status"],
            "not_configured"
        );
        task.abort();
    }

    #[test]
    fn credit_bound() {
        assert_eq!((0..1344).filter(|n| n % 3 != 2).count(), 896);
    }

    #[test]
    fn seo_converter_pages_are_rejected_at_normalization() {
        let now = 1_795_308_000;
        assert!(
            normalized(
                "https://www.theblock.co/convert/eth-usd",
                "ETH to USD Converter",
                None,
                "Tavily",
                now
            )
            .is_none()
        );
        assert!(
            normalized(
                "https://www.coindesk.com/markets/btc-etf-inflows",
                "Bitcoin ETF inflows accelerate into New York close",
                None,
                "Tavily",
                now
            )
            .is_some()
        );
    }

    #[test]
    fn cues_are_explicit() {
        for (title, tone) in [
            ("Bitcoin ETF inflows rise", "positive"),
            ("Ethereum hack causes losses", "negative"),
            ("Bitcoin gains after hack", "mixed"),
            ("Fed publishes minutes", "mixed"),
        ] {
            let story = Story {
                title: title.into(),
                url: "https://example.com/".into(),
                publisher: "example.com".into(),
                published_at: None,
                via: "Tavily".into(),
            };
            assert_eq!(tagged(story)["tone"], tone);
        }
    }
}
