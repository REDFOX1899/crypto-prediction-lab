//! Real-time 15-minute Quant Signal, Entry-Timing Advisor & Google Gemini Decision 1 Client.
//! Read-only research intelligence: zero execution authority, bounded latency, shared 15s cache.
use crate::{assets::Asset, news::NewsCatalyst};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    env, fs,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const DECISIONS_ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta/decisions";
const GEMINI_FRESH_SECS: u64 = 10;
const GEMINI_STALE_SECS: u64 = 30;
const LATENCY_RING_CAP: usize = 128;

/// Frozen 6-factor symmetric residual weights fit on 1,614 historical BTC 15-min windows
/// (first 80% chronological train set, L2=250, zero directional intercept).
/// Features: [xasset_lead, trade_press, mom_1m, book_imb10, cmc_1h, news_tone]
struct MinuteWeights {
    scales: [f64; 6],
    weights: [f64; 6],
    avg_spread_c: f64,
    avg_taker_fee_c: f64,
    fav_acc: f64,
}

const WEIGHTS: [MinuteWeights; 14] = [
    MinuteWeights {
        scales: [0.250832, 0.005083, 0.136267, 1.0, 0.004729, 0.378515],
        weights: [0.05316, 0.016908, -0.014852, 0.0, 0.012429, 0.033964],
        avg_spread_c: 1.00,
        avg_taker_fee_c: 1.62,
        fav_acc: 0.605,
    },
    MinuteWeights {
        scales: [0.334349, 0.004998, 0.114472, 1.0, 0.004793, 0.378515],
        weights: [0.038493, -0.012112, 0.035535, 0.0, 0.029128, 0.025316],
        avg_spread_c: 0.99,
        avg_taker_fee_c: 1.53,
        fav_acc: 0.647,
    },
    MinuteWeights {
        scales: [0.416003, 0.004970, 0.102220, 1.0, 0.004792, 0.378515],
        weights: [0.014136, -0.010390, 0.027364, 0.0, 0.028826, 0.013657],
        avg_spread_c: 0.98,
        avg_taker_fee_c: 1.45,
        fav_acc: 0.673,
    },
    MinuteWeights {
        scales: [0.476751, 0.004920, 0.100873, 1.0, 0.004792, 0.379000],
        weights: [0.042539, -0.053258, 0.026023, 0.0, 0.033820, 0.013922],
        avg_spread_c: 0.95,
        avg_taker_fee_c: 1.38,
        fav_acc: 0.673,
    },
    MinuteWeights {
        scales: [0.550427, 0.004825, 0.098011, 1.0, 0.004792, 0.379000],
        weights: [0.061489, 0.041250, 0.003821, 0.0, 0.052278, 0.014295],
        avg_spread_c: 0.92,
        avg_taker_fee_c: 1.32,
        fav_acc: 0.693,
    },
    MinuteWeights {
        scales: [0.637912, 0.004698, 0.106582, 1.0, 0.004715, 0.379000],
        weights: [0.073666, 0.015356, 0.009466, 0.0, 0.033648, 0.021225],
        avg_spread_c: 0.87,
        avg_taker_fee_c: 1.24,
        fav_acc: 0.728,
    },
    MinuteWeights {
        scales: [0.739267, 0.004494, 0.109711, 1.0, 0.004715, 0.379000],
        weights: [0.091847, 0.063581, -0.032632, 0.0, 0.031847, 0.012630],
        avg_spread_c: 0.81,
        avg_taker_fee_c: 1.15,
        fav_acc: 0.750,
    },
    MinuteWeights {
        scales: [0.835781, 0.004313, 0.113767, 1.0, 0.004715, 0.379112],
        weights: [0.083915, -0.018408, -0.040836, 0.0, 0.020027, 0.006054],
        avg_spread_c: 0.75,
        avg_taker_fee_c: 1.06,
        fav_acc: 0.780,
    },
    MinuteWeights {
        scales: [0.975330, 0.004085, 0.108877, 1.0, 0.004715, 0.379112],
        weights: [0.069999, -0.010757, 0.010439, 0.0, 0.008694, 0.016293],
        avg_spread_c: 0.68,
        avg_taker_fee_c: 0.95,
        fav_acc: 0.803,
    },
    MinuteWeights {
        scales: [1.119832, 0.003929, 0.112750, 1.0, 0.004715, 0.379112],
        weights: [0.039909, -0.017930, 0.057261, 0.0, 0.014421, 0.025377],
        avg_spread_c: 0.61,
        avg_taker_fee_c: 0.86,
        fav_acc: 0.815,
    },
    MinuteWeights {
        scales: [1.308445, 0.003533, 0.130318, 1.0, 0.004666, 0.379215],
        weights: [0.054729, -0.069767, -0.005905, 0.0, 0.018005, 0.015777],
        avg_spread_c: 0.53,
        avg_taker_fee_c: 0.73,
        fav_acc: 0.835,
    },
    MinuteWeights {
        scales: [1.590287, 0.003326, 0.136965, 1.0, 0.004688, 0.379406],
        weights: [0.078442, -0.075910, 0.013308, 0.0, 0.034222, 0.012404],
        avg_spread_c: 0.46,
        avg_taker_fee_c: 0.61,
        fav_acc: 0.866,
    },
    MinuteWeights {
        scales: [2.072862, 0.003102, 0.152311, 1.0, 0.004631, 0.378475],
        weights: [0.061887, -0.036809, -0.001961, 0.0, 0.011564, -0.002955],
        avg_spread_c: 0.38,
        avg_taker_fee_c: 0.50,
        fav_acc: 0.890,
    },
    MinuteWeights {
        scales: [3.217335, 0.005423, 0.251398, 1.0, 0.004025, 0.382540],
        weights: [-0.001308, 0.022290, 0.002969, 0.0, 0.005605, -0.005785],
        avg_spread_c: 0.40,
        avg_taker_fee_c: 0.45,
        fav_acc: 0.906,
    },
];

pub fn logit(p: f64) -> f64 {
    let c = p.clamp(0.005, 0.995);
    (c / (1.0 - c)).ln()
}

pub fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z.clamp(-20.0, 20.0)).exp())
}

pub fn kalshi_taker_fee(p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    (0.07 * p * (1.0 - p) * 1_000_000.0).ceil() / 1_000_000.0
}

pub fn kalshi_maker_fee(p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    (0.0175 * p * (1.0 - p) * 1_000_000.0).ceil() / 1_000_000.0
}

#[derive(Clone, Debug)]
pub struct LiveMarketInput {
    pub asset: Asset,
    pub ticker: String,
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
    pub now: DateTime<Utc>,
    pub yes_bid: f64,
    pub yes_ask: f64,
    pub target_usd: Option<f64>,
    pub reference_usd: Option<f64>,
    pub book_imbalance_top10: Option<f64>,
    pub peer_mid_a: Option<f64>,
    pub peer_mid_b: Option<f64>,
    pub peer_labels: (&'static str, &'static str),
    pub cmc_1h_pct: Option<f64>,
    pub news: NewsCatalyst,
    pub quote_source: &'static str,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeminiAnswer {
    pub status: String,
    pub model: String,
    pub latency_ms: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: u64,
    pub settle_yes_prob: Option<f64>,
    pub settle_yes_confidence: Option<f64>,
    pub action_choice: Option<String>,
    pub action_probabilities: Option<Value>,
    pub timing_score: Option<f64>,
    pub catalyst_alignment: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FactorAttribution {
    pub name: &'static str,
    pub label: &'static str,
    pub raw_display: String,
    pub logit_delta: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LatencyTelemetry {
    pub data_fetch_ms: f64,
    pub quant_compute_us: u64,
    pub gemini_ms: Option<u64>,
    pub total_server_ms: f64,
    pub cache_status: &'static str,
    pub server_p50_ms: f64,
    pub server_p95_ms: f64,
    pub gemini_p50_ms: Option<u64>,
    pub gemini_p95_ms: Option<u64>,
    pub sample_count: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EvalOptions {
    pub fast_only: bool,
    pub force_refresh: bool,
    pub data_fetch_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SignalReport {
    pub schema_version: u8,
    pub asset: Asset,
    pub ticker: String,
    pub computed_at: DateTime<Utc>,
    pub quote_source: &'static str,
    pub elapsed_seconds: i64,
    pub elapsed_minute: usize,
    pub remaining_seconds: i64,
    pub window_phase: &'static str,
    pub window_phase_label: &'static str,
    pub optimal_window_active: bool,
    pub market: Value,
    pub fees: Value,
    pub quant: Value,
    pub gemini: GeminiAnswer,
    pub hybrid: Value,
    pub factors: Vec<FactorAttribution>,
    pub news_catalyst: NewsCatalyst,
    pub blinded_packet: Value,
    pub latency: LatencyTelemetry,
    pub cached: bool,
}

struct GeminiCacheEntry {
    at: Instant,
    ticker: String,
    answer: GeminiAnswer,
}

#[derive(Default)]
struct LatencyRing {
    server_ms: VecDeque<f64>,
    gemini_ms: VecDeque<u64>,
}

impl LatencyRing {
    fn record(&mut self, server_ms: f64, gemini_ms: Option<u64>) {
        if self.server_ms.len() >= LATENCY_RING_CAP {
            self.server_ms.pop_front();
        }
        self.server_ms.push_back(server_ms);
        if let Some(g) = gemini_ms {
            if self.gemini_ms.len() >= LATENCY_RING_CAP {
                self.gemini_ms.pop_front();
            }
            self.gemini_ms.push_back(g);
        }
    }

    fn stats(&self) -> (f64, f64, Option<u64>, Option<u64>, usize) {
        let mut sv: Vec<f64> = self.server_ms.iter().copied().collect();
        sv.sort_by(f64::total_cmp);
        let pct_f = |arr: &[f64], q: f64| -> f64 {
            if arr.is_empty() {
                0.0
            } else {
                let idx = ((arr.len() - 1) as f64 * q).round() as usize;
                round2(arr[idx.min(arr.len() - 1)])
            }
        };
        let mut gv: Vec<u64> = self.gemini_ms.iter().copied().collect();
        gv.sort_unstable();
        let pct_u = |arr: &[u64], q: f64| -> Option<u64> {
            if arr.is_empty() {
                None
            } else {
                let idx = ((arr.len() - 1) as f64 * q).round() as usize;
                Some(arr[idx.min(arr.len() - 1)])
            }
        };
        (
            pct_f(&sv, 0.50),
            pct_f(&sv, 0.95),
            pct_u(&gv, 0.50),
            pct_u(&gv, 0.95),
            sv.len(),
        )
    }
}

pub struct Engine {
    http: Client,
    endpoint: String,
    primary_key: Option<String>,
    secondary_key: Option<String>,
    gemini_cache: Mutex<HashMap<Asset, GeminiCacheEntry>>,
    in_flight: Mutex<HashSet<Asset>>,
    latency_ring: std::sync::Mutex<LatencyRing>,
}

fn read_secret(file_var: &str, env_var: &str) -> Option<String> {
    env::var(file_var)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .or_else(|| env::var(env_var).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.contains('\n'))
}

impl Engine {
    pub fn from_env() -> Arc<Self> {
        let endpoint = env::var("GEMINI_DECISIONS_ENDPOINT")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DECISIONS_ENDPOINT.to_owned());
        Self::new(
            endpoint,
            read_secret("GEMINI_API_KEY_FILE", "GEMINI_API_KEY"),
            read_secret("GEMINI_API_KEY_SECONDARY_FILE", "GEMINI_API_KEY_SECONDARY"),
        )
    }

    pub fn new(
        endpoint: String,
        primary_key: Option<String>,
        secondary_key: Option<String>,
    ) -> Arc<Self> {
        let http = Client::builder()
            .tcp_nodelay(true)
            .tcp_keepalive(Duration::from_secs(30))
            .connect_timeout(Duration::from_millis(800))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(4)
            .timeout(Duration::from_millis(1600))
            .user_agent("YesNoTape/0.3 (gemini-decision-1-fast)")
            .build()
            .unwrap_or_else(|_| Client::new());
        Arc::new(Self {
            http,
            endpoint,
            primary_key,
            secondary_key,
            gemini_cache: Mutex::new(HashMap::new()),
            in_flight: Mutex::new(HashSet::new()),
            latency_ring: std::sync::Mutex::new(LatencyRing::default()),
        })
    }

    pub fn telemetry_snapshot(&self) -> Value {
        let ring = self.latency_ring.lock().unwrap_or_else(|e| e.into_inner());
        let (server_p50_ms, server_p95_ms, gemini_p50_ms, gemini_p95_ms, sample_count) =
            ring.stats();
        let recent_server_ms: Vec<f64> = ring
            .server_ms
            .iter()
            .rev()
            .take(32)
            .copied()
            .rev()
            .collect();
        let recent_gemini_ms: Vec<u64> = ring
            .gemini_ms
            .iter()
            .rev()
            .take(32)
            .copied()
            .rev()
            .collect();
        json!({
            "server_p50_ms": server_p50_ms,
            "server_p95_ms": server_p95_ms,
            "gemini_p50_ms": gemini_p50_ms,
            "gemini_p95_ms": gemini_p95_ms,
            "sample_count": sample_count,
            "recent_server_ms": recent_server_ms,
            "recent_gemini_ms": recent_gemini_ms,
        })
    }

    #[cfg(test)]
    pub async fn evaluate(self: &Arc<Self>, input: LiveMarketInput) -> SignalReport {
        self.evaluate_with_options(input, EvalOptions::default())
            .await
    }

    /// Low-latency two-tier evaluator:
    /// 1. Always computes deterministic 6-factor Rust quant + exact Kalshi fees fresh on the live tick (<200us).
    /// 2. Reuses warm Gemini Decision 1 overlay (<10s) or returns stale overlay immediately (10-30s) while
    ///    spawning a singleflight background refresh (SWR) so quote latency stays sub-2ms on warm traffic.
    pub async fn evaluate_with_options(
        self: &Arc<Self>,
        input: LiveMarketInput,
        opts: EvalOptions,
    ) -> SignalReport {
        let eval_started = Instant::now();
        let quant_started = Instant::now();
        let elapsed_secs = (input.now - input.open_time).num_seconds().clamp(0, 900);
        let remaining_secs = (input.close_time - input.now).num_seconds().clamp(0, 900);
        let minute_idx = ((elapsed_secs / 60) as usize).clamp(0, 13);
        let elapsed_minute = minute_idx + 1;
        let w = &WEIGHTS[minute_idx];

        let (phase, phase_label, optimal_active) = match elapsed_minute {
            1..=5 => (
                "early_high_friction",
                "Early Window (Min 1–5) · Wide spread & peak quadratic fee",
                false,
            ),
            6..=9 => (
                "mid_discovery",
                "Mid Discovery (Min 6–9) · Directional price discovery",
                false,
            ),
            10..=13 => (
                "compression_optimal",
                "Compression Window (Min 10–13) · Sub-cent spread & lowest fee drag",
                true,
            ),
            _ => (
                "final_minute_avg",
                "Final Minute (Min 14–15) · 60-second CF settlement averaging",
                false,
            ),
        };

        let yes_bid = input.yes_bid.clamp(0.005, 0.995);
        let yes_ask = input.yes_ask.clamp(yes_bid, 0.995);
        let no_ask = (1.0 - yes_bid).clamp(0.005, 0.995);
        let mid = 0.5 * (yes_bid + yes_ask);
        let spread = (yes_ask - yes_bid).max(0.0);
        let lm = logit(mid);

        let peer_mids: Vec<f64> = [input.peer_mid_a, input.peer_mid_b]
            .into_iter()
            .flatten()
            .filter(|v| *v > 0.005 && *v < 0.995)
            .collect();
        let peer_consensus_logit = if peer_mids.is_empty() {
            lm
        } else {
            peer_mids.iter().map(|m| logit(*m)).sum::<f64>() / peer_mids.len() as f64
        };
        let xasset_lead = peer_consensus_logit - lm;
        let book_imb = input.book_imbalance_top10.unwrap_or(0.0).clamp(-1.0, 1.0);
        let cmc_1h = input.cmc_1h_pct.unwrap_or(0.0) / 100.0;
        let news_tone = input.news.net_tone.clamp(-1.0, 1.0);

        let raw_feats = [xasset_lead, 0.0, 0.0, book_imb, cmc_1h, news_tone];
        let mut residual_logit = 0.0_f64;
        for (i, feat) in raw_feats.iter().enumerate() {
            let z = (feat / w.scales[i]).clamp(-4.0, 4.0);
            residual_logit += z * w.weights[i];
        }
        residual_logit = residual_logit.clamp(-0.65, 0.65);
        let quant_p_yes = sigmoid(lm + residual_logit);

        let fee_tkr_yes = kalshi_taker_fee(yes_ask);
        let fee_tkr_no = kalshi_taker_fee(no_ask);
        let fee_mkr_yes = kalshi_maker_fee(mid);
        let fee_mkr_no = kalshi_maker_fee(1.0 - mid);

        let ref_gap_bps = match (input.reference_usd, input.target_usd) {
            (Some(r), Some(t)) if t > 0.0 => Some(((r - t) / t) * 10_000.0),
            _ => None,
        };

        // Human-readable blinded audit packet for UI inspector (NO ticker, NO calendar date, NO USD strike!)
        let blinded_packet = json!({
            "asset": input.asset.info().symbol,
            "contract": "15-minute binary finish at or above opening reference target",
            "elapsed_minute": elapsed_minute,
            "remaining_seconds": remaining_secs,
            "window_phase": phase,
            "yes_bid_cents": round2(yes_bid * 100.0),
            "yes_ask_cents": round2(yes_ask * 100.0),
            "yes_midpoint_cents": round2(mid * 100.0),
            "spread_cents": round2(spread * 100.0),
            "taker_fee_yes_cents": round2(fee_tkr_yes * 100.0),
            "taker_fee_no_cents": round2(fee_tkr_no * 100.0),
            "reference_minus_target_bps": ref_gap_bps.map(round2),
            "peer_assets_consensus_minus_asset_logit": round3(xasset_lead),
            "l2_top10_book_imbalance_pct": round2(book_imb * 100.0),
            "cmc_1h_return_pct": input.cmc_1h_pct.map(round3),
            "quant_model_p_yes_pct": round2(quant_p_yes * 100.0),
            "news_catalyst_net_tone": round2(news_tone),
            "news_headlines": input.news.top_headlines,
        });

        // Ultra-compact blinded feature line (~260 tokens total vs ~770 tokens -> lower Gemini prefill latency!)
        let compact_prompt = format!(
            "asset={} min={}/15 rem={}s phase={} bid={:.1}c ask={:.1}c mid={:.1}c sprd={:.2}c feeY={:.2}c feeN={:.2}c refGap={}bps peerDLogit={:+.3} imb10={:+.1}% cmc1h={}% qPyes={:.2}% news={:+.2} heads={}",
            input.asset.code(),
            elapsed_minute,
            remaining_secs,
            phase,
            yes_bid * 100.0,
            yes_ask * 100.0,
            mid * 100.0,
            spread * 100.0,
            fee_tkr_yes * 100.0,
            fee_tkr_no * 100.0,
            ref_gap_bps
                .map(|v| format!("{v:+.2}"))
                .unwrap_or_else(|| "na".into()),
            xasset_lead,
            book_imb * 100.0,
            input
                .cmc_1h_pct
                .map(|v| format!("{v:+.2}"))
                .unwrap_or_else(|| "na".into()),
            quant_p_yes * 100.0,
            news_tone,
            if input.news.top_headlines.is_empty() {
                "none".to_owned()
            } else {
                input
                    .news
                    .top_headlines
                    .iter()
                    .take(3)
                    .map(|h| h.chars().take(72).collect::<String>())
                    .collect::<Vec<_>>()
                    .join(" | ")
            }
        );
        let quant_compute_us = quant_started.elapsed().as_micros() as u64;

        // Tier-2 Gemini lookup with Stale-While-Revalidate + singleflight background coalescing
        let (gemini, cache_status, overlay_cached, live_gemini_ms) = self
            .resolve_gemini(input.asset, &input.ticker, compact_prompt, opts)
            .await;

        let hybrid_p_yes = if let Some(gp) = gemini.settle_yes_prob {
            let g_delta = (logit(gp) - lm).clamp(-0.9, 0.9);
            sigmoid(logit(quant_p_yes) + 0.08 * g_delta)
        } else {
            quant_p_yes
        };

        let taker_ev_yes_c = (hybrid_p_yes - yes_ask - fee_tkr_yes) * 100.0;
        let taker_ev_no_c = ((1.0 - hybrid_p_yes) - no_ask - fee_tkr_no) * 100.0;
        let maker_ev_yes_c = (hybrid_p_yes - mid - fee_mkr_yes) * 100.0;
        let maker_ev_no_c = ((1.0 - hybrid_p_yes) - (1.0 - mid) - fee_mkr_no) * 100.0;

        let (posture, posture_side, posture_reason) = decide_posture(
            elapsed_minute,
            spread * 100.0,
            taker_ev_yes_c,
            taker_ev_no_c,
            maker_ev_yes_c,
            maker_ev_no_c,
            gemini.action_choice.as_deref(),
        );

        let factors = vec![
            FactorAttribution {
                name: "xasset_lead",
                label: "Cross-Asset Lead (ETH/SOL)",
                raw_display: format!(
                    "{:+.2} logit ({}/{})",
                    xasset_lead, input.peer_labels.0, input.peer_labels.1
                ),
                logit_delta: round4((raw_feats[0] / w.scales[0]).clamp(-4.0, 4.0) * w.weights[0]),
            },
            FactorAttribution {
                name: "cmc_1h",
                label: "1h Spot Trend (CMC)",
                raw_display: input
                    .cmc_1h_pct
                    .map(|v| format!("{v:+.2}%"))
                    .unwrap_or_else(|| "unavailable".into()),
                logit_delta: round4((raw_feats[4] / w.scales[4]).clamp(-4.0, 4.0) * w.weights[4]),
            },
            FactorAttribution {
                name: "news_tone",
                label: "Tavily + News Catalyst",
                raw_display: format!(
                    "{:+.2} ({} bull / {} bear)",
                    news_tone, input.news.bullish_count, input.news.bearish_count
                ),
                logit_delta: round4((raw_feats[5] / w.scales[5]).clamp(-4.0, 4.0) * w.weights[5]),
            },
            FactorAttribution {
                name: "book_imb10",
                label: "L2 Order-Book Imbalance",
                raw_display: input
                    .book_imbalance_top10
                    .map(|v| format!("{:+.1}% YES", v * 100.0))
                    .unwrap_or_else(|| "unavailable".into()),
                logit_delta: round4(book_imb * 0.04),
            },
            FactorAttribution {
                name: "gemini_residual",
                label: "Gemini Decision 1 Overlay",
                raw_display: gemini
                    .settle_yes_prob
                    .map(|p| {
                        format!(
                            "{:.1}% YES ({}ms)",
                            p * 100.0,
                            gemini.latency_ms.unwrap_or(0)
                        )
                    })
                    .unwrap_or_else(|| gemini.status.clone()),
                logit_delta: gemini
                    .settle_yes_prob
                    .map(|gp| round4(0.08 * (logit(gp) - lm).clamp(-0.9, 0.9)))
                    .unwrap_or(0.0),
            },
        ];

        let total_server_ms =
            round2(opts.data_fetch_ms + eval_started.elapsed().as_secs_f64() * 1000.0);
        let (server_p50_ms, server_p95_ms, gemini_p50_ms, gemini_p95_ms, sample_count) = {
            let mut ring = self.latency_ring.lock().unwrap_or_else(|e| e.into_inner());
            ring.record(total_server_ms, live_gemini_ms);
            ring.stats()
        };

        SignalReport {
            schema_version: 2,
            asset: input.asset,
            ticker: input.ticker,
            computed_at: input.now,
            quote_source: input.quote_source,
            elapsed_seconds: elapsed_secs,
            elapsed_minute,
            remaining_seconds: remaining_secs,
            window_phase: phase,
            window_phase_label: phase_label,
            optimal_window_active: optimal_active,
            market: json!({
                "yes_bid_cents": round2(yes_bid * 100.0),
                "yes_ask_cents": round2(yes_ask * 100.0),
                "no_ask_cents": round2(no_ask * 100.0),
                "midpoint_cents": round2(mid * 100.0),
                "spread_cents": round2(spread * 100.0),
                "target_usd": input.target_usd,
                "reference_usd": input.reference_usd,
                "reference_gap_bps": ref_gap_bps.map(round2),
                "historical_min_spread_cents": w.avg_spread_c,
                "historical_min_fav_acc_pct": round2(w.fav_acc * 100.0),
            }),
            fees: json!({
                "taker_yes_cents": round2(fee_tkr_yes * 100.0),
                "taker_no_cents": round2(fee_tkr_no * 100.0),
                "maker_yes_cents": round2(fee_mkr_yes * 100.0),
                "maker_no_cents": round2(fee_mkr_no * 100.0),
                "historical_min_taker_fee_cents": w.avg_taker_fee_c,
                "breakeven_yes_taker_pct": round2((yes_ask + fee_tkr_yes) * 100.0),
                "breakeven_no_taker_pct": round2((no_ask + fee_tkr_no) * 100.0),
                "breakeven_yes_maker_pct": round2((mid + fee_mkr_yes) * 100.0),
                "breakeven_no_maker_pct": round2(((1.0 - mid) + fee_mkr_no) * 100.0),
            }),
            quant: json!({
                "model_id": "btc15m_residual_l2_v1",
                "p_yes": round4(quant_p_yes),
                "p_yes_pct": round2(quant_p_yes * 100.0),
                "residual_logit": round4(residual_logit),
                "delta_vs_mid_pp": round2((quant_p_yes - mid) * 100.0),
            }),
            gemini,
            hybrid: json!({
                "p_yes": round4(hybrid_p_yes),
                "p_yes_pct": round2(hybrid_p_yes * 100.0),
                "delta_vs_mid_pp": round2((hybrid_p_yes - mid) * 100.0),
                "taker_ev_yes_cents": round2(taker_ev_yes_c),
                "taker_ev_no_cents": round2(taker_ev_no_c),
                "maker_ev_yes_cents": round2(maker_ev_yes_c),
                "maker_ev_no_cents": round2(maker_ev_no_c),
                "posture": posture,
                "side": posture_side,
                "reason": posture_reason,
                "disclosure": "Read-only quantitative research signal. No order execution or profitability guarantee."
            }),
            factors,
            news_catalyst: input.news,
            blinded_packet,
            latency: LatencyTelemetry {
                data_fetch_ms: round2(opts.data_fetch_ms),
                quant_compute_us,
                gemini_ms: live_gemini_ms,
                total_server_ms,
                cache_status,
                server_p50_ms,
                server_p95_ms,
                gemini_p50_ms,
                gemini_p95_ms,
                sample_count,
            },
            cached: overlay_cached,
        }
    }

    async fn resolve_gemini(
        self: &Arc<Self>,
        asset: Asset,
        ticker: &str,
        compact_prompt: String,
        opts: EvalOptions,
    ) -> (GeminiAnswer, &'static str, bool, Option<u64>) {
        if !opts.force_refresh {
            let cached = {
                let guard = self.gemini_cache.lock().await;
                guard.get(&asset).and_then(|e| {
                    if e.ticker == ticker {
                        Some((e.at.elapsed(), e.answer.clone()))
                    } else {
                        None
                    }
                })
            };
            if let Some((age, ans)) = cached {
                if age < Duration::from_secs(GEMINI_FRESH_SECS) {
                    return (ans, "warm_overlay", true, None);
                }
                if age < Duration::from_secs(GEMINI_STALE_SECS) {
                    // Stale-While-Revalidate: return sub-2ms now & coalesce one async refresh!
                    self.spawn_background_refresh(asset, ticker.to_owned(), compact_prompt)
                        .await;
                    return (ans, "swr_refreshing", true, None);
                }
            }
            if opts.fast_only {
                self.spawn_background_refresh(asset, ticker.to_owned(), compact_prompt)
                    .await;
                return (
                    GeminiAnswer {
                        status: "warming_async".into(),
                        model: "gemini-decision-1".into(),
                        latency_ms: None,
                        input_tokens: None,
                        output_tokens: 0,
                        settle_yes_prob: None,
                        settle_yes_confidence: None,
                        action_choice: None,
                        action_probabilities: None,
                        timing_score: None,
                        catalyst_alignment: None,
                    },
                    "fast_path",
                    false,
                    None,
                );
            }
        }

        let ans = self.call_gemini(&compact_prompt).await;
        let lat = ans.latency_ms;
        if ans.status == "live" {
            let mut guard = self.gemini_cache.lock().await;
            guard.insert(
                asset,
                GeminiCacheEntry {
                    at: Instant::now(),
                    ticker: ticker.to_owned(),
                    answer: ans.clone(),
                },
            );
        }
        (ans, "cold_live", false, lat)
    }

    async fn spawn_background_refresh(
        self: &Arc<Self>,
        asset: Asset,
        ticker: String,
        compact_prompt: String,
    ) {
        if self.primary_key.is_none() && self.secondary_key.is_none() {
            return;
        }
        {
            let mut flight = self.in_flight.lock().await;
            if !flight.insert(asset) {
                return;
            }
        }
        let engine = Arc::clone(self);
        tokio::spawn(async move {
            let ans = engine.call_gemini(&compact_prompt).await;
            let lat = ans.latency_ms;
            if ans.status == "live" {
                let mut guard = engine.gemini_cache.lock().await;
                guard.insert(
                    asset,
                    GeminiCacheEntry {
                        at: Instant::now(),
                        ticker,
                        answer: ans,
                    },
                );
            }
            if let Some(ms) = lat {
                let mut ring = engine
                    .latency_ring
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if ring.gemini_ms.len() >= LATENCY_RING_CAP {
                    ring.gemini_ms.pop_front();
                }
                ring.gemini_ms.push_back(ms);
            }
            engine.in_flight.lock().await.remove(&asset);
        });
    }

    async fn call_gemini(&self, compact_prompt: &str) -> GeminiAnswer {
        let keys: Vec<&str> = [self.primary_key.as_deref(), self.secondary_key.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        if keys.is_empty() {
            return GeminiAnswer {
                status: "not_configured".into(),
                model: "gemini-decision-1".into(),
                latency_ms: None,
                input_tokens: None,
                output_tokens: 0,
                settle_yes_prob: None,
                settle_yes_confidence: None,
                action_choice: None,
                action_probabilities: None,
                timing_score: None,
                catalyst_alignment: None,
            };
        }

        // Ultra-compact structured prompt (~280 tokens vs ~770 tokens) for minimum prefill latency
        let body = json!({
            "model": "gemini-decision-1",
            "input": [{"type": "text", "text": compact_prompt}],
            "questions": {
                "settle_yes": {
                    "type": "noul",
                    "instructions": "Will this 15m binary settle YES (>=target)?",
                    "criteria": {
                        "true_description": "Settles YES.",
                        "false_description": "Settles NO."
                    }
                },
                "action": {
                    "type": "choice",
                    "instructions": "Best posture after spread & Kalshi fee?",
                    "criteria": {
                        "bet_yes": "Positive EV buying YES.",
                        "bet_no": "Positive EV buying NO.",
                        "wait_or_pass": "No edge or wait for Min 10-13."
                    }
                },
                "timing_window": {
                    "type": "score",
                    "instructions": "Rate entry timing quality.",
                    "criteria": [
                        "Poor: early noise/wide spread.",
                        "Fair: mid discovery.",
                        "Optimal: Min 10-13 low spread/fee."
                    ]
                },
                "catalyst_alignment": {
                    "type": "choice",
                    "instructions": "Do peer quotes, L2 book & news align?",
                    "criteria": {
                        "aligned_bullish": "Signals lean YES.",
                        "aligned_bearish": "Signals lean NO.",
                        "divergent_or_noisy": "Mixed or neutral."
                    }
                }
            }
        });

        let mut last_status = "unavailable".to_owned();
        for key in keys {
            let started = Instant::now();
            let resp = self
                .http
                .post(&self.endpoint)
                .header("x-goog-api-key", key)
                .json(&body)
                .send()
                .await;
            let latency_ms = started.elapsed().as_millis() as u64;
            match resp {
                Ok(r) if r.status().is_success() => {
                    if let Ok(parsed) = r.json::<Value>().await
                        && let Some(ans) = parsed.get("answers")
                    {
                        return GeminiAnswer {
                            status: "live".into(),
                            model: "gemini-decision-1".into(),
                            latency_ms: Some(latency_ms),
                            input_tokens: parsed["usage"]["total_input_tokens"].as_u64(),
                            output_tokens: parsed["usage"]["total_output_tokens"]
                                .as_u64()
                                .unwrap_or(0),
                            settle_yes_prob: ans["settle_yes"]["noul"].as_f64(),
                            settle_yes_confidence: ans["settle_yes"]["confidence"].as_f64(),
                            action_choice: ans["action"]["choice"].as_str().map(str::to_owned),
                            action_probabilities: ans["action"].get("probabilities").cloned(),
                            timing_score: ans["timing_window"]["score"].as_f64(),
                            catalyst_alignment: ans["catalyst_alignment"]["choice"]
                                .as_str()
                                .map(str::to_owned),
                        };
                    }
                    last_status = "invalid_response".into();
                }
                Ok(r) => {
                    let code = r.status().as_u16();
                    last_status = match code {
                        402 => "credits_depleted".into(),
                        429 => "rate_limited".into(),
                        401 | 403 => "auth_error".into(),
                        _ => format!("http_{code}"),
                    };
                    if code != 429 && code != 402 {
                        break;
                    }
                }
                Err(_) => {
                    last_status = "timeout_or_network".into();
                }
            }
        }
        GeminiAnswer {
            status: last_status,
            model: "gemini-decision-1".into(),
            latency_ms: None,
            input_tokens: None,
            output_tokens: 0,
            settle_yes_prob: None,
            settle_yes_confidence: None,
            action_choice: None,
            action_probabilities: None,
            timing_score: None,
            catalyst_alignment: None,
        }
    }
}

fn decide_posture(
    minute: usize,
    spread_c: f64,
    tkr_yes: f64,
    tkr_no: f64,
    mkr_yes: f64,
    mkr_no: f64,
    gemini_action: Option<&str>,
) -> (&'static str, &'static str, String) {
    if minute <= 5 && tkr_yes < 1.5 && tkr_no < 1.5 {
        return (
            "WAIT_COMPRESSION",
            "NONE",
            format!(
                "Minute {minute} is inside the high-friction opening zone (spread {spread_c:.2}¢). Historical study favors waiting for Min 10–13 spread compression."
            ),
        );
    }
    if tkr_yes >= 0.4 && tkr_yes >= tkr_no && spread_c <= 1.2 {
        let confirm = if gemini_action == Some("bet_yes") {
            " · confirmed by Gemini Decision 1"
        } else {
            ""
        };
        return (
            "YES_TAKER_CANDIDATE",
            "YES",
            format!(
                "Hybrid model clears YES ask + Kalshi taker fee by +{tkr_yes:.2}¢ (spread {spread_c:.2}¢){confirm}."
            ),
        );
    }
    if tkr_no >= 0.4 && tkr_no > tkr_yes && spread_c <= 1.2 {
        let confirm = if gemini_action == Some("bet_no") {
            " · confirmed by Gemini Decision 1"
        } else {
            ""
        };
        return (
            "NO_TAKER_CANDIDATE",
            "NO",
            format!(
                "Hybrid model clears NO ask + Kalshi taker fee by +{tkr_no:.2}¢ (spread {spread_c:.2}¢){confirm}."
            ),
        );
    }
    if (10..=13).contains(&minute) && mkr_yes >= 0.6 && mkr_yes >= mkr_no {
        return (
            "YES_MAKER_WATCH",
            "YES",
            format!(
                "Compression window (Min {minute}): passive midpoint limit clears maker fee by +{mkr_yes:.2}¢ (taker fee still too wide)."
            ),
        );
    }
    if (10..=13).contains(&minute) && mkr_no >= 0.6 && mkr_no > mkr_yes {
        return (
            "NO_MAKER_WATCH",
            "NO",
            format!(
                "Compression window (Min {minute}): passive NO limit clears maker fee by +{mkr_no:.2}¢ (taker fee still too wide)."
            ),
        );
    }
    if minute < 10 {
        (
            "WAIT_COMPRESSION",
            "NONE",
            format!(
                "Market near fair value at Min {minute} (best taker EV {:+.2}¢). Wait for Min 10–13 fee & spread compression.",
                tkr_yes.max(tkr_no)
            ),
        )
    } else {
        (
            "PASS",
            "NONE",
            format!(
                "No edge above Kalshi fee + spread hurdle (best taker EV {:+.2}¢, maker EV {:+.2}¢). Passing preserves capital.",
                tkr_yes.max(tkr_no),
                mkr_yes.max(mkr_no)
            ),
        )
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}
fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    fn sample_input(minute: i64) -> LiveMarketInput {
        let open = DateTime::from_timestamp(1_795_300_200, 0).unwrap();
        LiveMarketInput {
            asset: Asset::Btc,
            ticker: "KXBTC15M-TEST".into(),
            open_time: open,
            close_time: open + chrono::Duration::minutes(15),
            now: open + chrono::Duration::minutes(minute) + chrono::Duration::seconds(10),
            yes_bid: 0.54,
            yes_ask: 0.55,
            target_usd: Some(82_000.0),
            reference_usd: Some(82_045.0),
            book_imbalance_top10: Some(0.22),
            peer_mid_a: Some(0.72),
            peer_mid_b: Some(0.70),
            peer_labels: ("ETH", "SOL"),
            cmc_1h_pct: Some(0.45),
            news: NewsCatalyst {
                net_tone: 0.5,
                relevant_count: 4,
                bullish_count: 3,
                bearish_count: 1,
                avg_impact: 1.1,
                checked_at: Some(1_795_300_000),
                scored_by: "gemini-decision-1".into(),
                top_headlines: vec!["Bitcoin ETF inflows strengthen".into()],
            },
            quote_source: "live_stream",
        }
    }

    #[tokio::test]
    async fn calls_gemini_decisions_with_blinded_packet_and_caches_15s() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/decisions"))
            .and(header("x-goog-api-key", "test-secret-key"))
            .respond_with(|req: &wiremock::Request| {
                let body: Value = serde_json::from_slice(&req.body).unwrap();
                assert_eq!(body["model"], "gemini-decision-1");
                let text = body["input"][0]["text"].as_str().unwrap();
                assert!(!text.contains("KXBTC15M"));
                assert!(!text.contains("82000"));
                ResponseTemplate::new(200).set_body_json(json!({
                    "answers": {
                        "settle_yes": {"type": "noul", "noul": 0.68, "confidence": 0.68},
                        "action": {"type": "choice", "choice": "bet_yes", "confidence": 0.61, "probabilities": {"bet_yes": 0.61, "wait_or_pass": 0.35, "bet_no": 0.04}},
                        "timing_window": {"type": "score", "score": 1.72, "confidence": 0.74, "probabilities": {"0": 0.02, "1": 0.24, "2": 0.74}},
                        "catalyst_alignment": {"type": "choice", "choice": "aligned_bullish", "confidence": 0.81, "probabilities": {"aligned_bullish": 0.81, "divergent_or_noisy": 0.16, "aligned_bearish": 0.03}}
                    },
                    "usage": {"total_input_tokens": 512, "total_output_tokens": 0}
                }))
            })
            .expect(1)
            .mount(&server)
            .await;

        let engine = Engine::new(
            format!("{}/v1beta/decisions", server.uri()),
            Some("test-secret-key".into()),
            None,
        );
        let first = engine.evaluate(sample_input(11)).await;
        assert_eq!(first.gemini.status, "live");
        assert_eq!(first.gemini.output_tokens, 0);
        assert_eq!(first.gemini.settle_yes_prob, Some(0.68));
        assert!(first.optimal_window_active);
        assert!(!first.cached);
        assert!(first.hybrid["p_yes"].as_f64().unwrap() > 0.55);

        let second = engine.evaluate(sample_input(11)).await;
        assert!(second.cached);
        assert_eq!(second.latency.cache_status, "warm_overlay");
        assert!(second.latency.sample_count >= 2);
        assert!(second.latency.total_server_ms < 50.0);
    }

    #[tokio::test]
    async fn fast_path_returns_sub_millisecond_quant_without_blocking_on_gemini() {
        let engine = Engine::new("http://127.0.0.1:1/never-called".into(), None, None);
        let report = engine
            .evaluate_with_options(
                sample_input(12),
                EvalOptions {
                    fast_only: true,
                    force_refresh: false,
                    data_fetch_ms: 0.25,
                },
            )
            .await;
        assert_eq!(report.latency.cache_status, "fast_path");
        assert_eq!(report.gemini.status, "warming_async");
        assert!(report.latency.total_server_ms < 50.0);
        assert!(report.quant["p_yes"].as_f64().unwrap() > 0.50);
    }

    #[tokio::test]
    async fn gracefully_falls_back_to_quant_only_when_gemini_returns_402() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/decisions"))
            .respond_with(ResponseTemplate::new(402))
            .expect(1)
            .mount(&server)
            .await;

        let engine = Engine::new(
            format!("{}/v1beta/decisions", server.uri()),
            Some("depleted-key".into()),
            None,
        );
        let report = engine.evaluate(sample_input(2)).await;
        assert_eq!(report.gemini.status, "credits_depleted");
        assert_eq!(report.window_phase, "early_high_friction");
        assert_eq!(report.hybrid["posture"], "WAIT_COMPRESSION");
        assert_eq!(report.hybrid["p_yes"], report.quant["p_yes"]);
    }

    #[test]
    fn kalshi_fees_match_quadratic_schedule() {
        // At 50c: 0.07 * 0.25 = 0.0175 (1.75c taker), 0.0175 * 0.25 = 0.004375 (0.44c maker)
        assert!((kalshi_taker_fee(0.50) - 0.0175).abs() < 1e-6);
        assert!((kalshi_maker_fee(0.50) - 0.004375).abs() < 1e-6);
        // At 90c: 0.07 * 0.09 = 0.0063 (0.63c taker — 2.8x cheaper than 50c!)
        assert!((kalshi_taker_fee(0.90) - 0.0063).abs() < 1e-6);
    }
}
