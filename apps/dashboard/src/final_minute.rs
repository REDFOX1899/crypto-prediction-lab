//! Bounded, per-asset observations; never substitutes for official settlement data.
use crate::assets::Asset;
use kalshi_client::Market;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub type Shared = Arc<Mutex<Recorder>>;
#[derive(Default)]
pub struct Recorder {
    samples: BTreeMap<i64, (i64, Decimal)>,
    market: Option<Market>,
    checked_at: i64,
}
#[derive(Serialize)]
pub struct Summary {
    pub asset: Asset,
    pub server_time_ms: i64,
    pub phase: &'static str,
    pub ticker: Option<String>,
    pub close_time_ms: Option<i64>,
    pub target: Option<Decimal>,
    pub latest_value: Option<Decimal>,
    pub source_time_ms: Option<i64>,
    pub source_state: String,
    pub distance_usd: Option<Decimal>,
    pub distance_percent: Option<Decimal>,
    pub observed_average: Option<Decimal>,
    pub observed_seconds: usize,
    pub elapsed_seconds: usize,
    pub coverage: String,
    pub required_remaining_average: Option<Decimal>,
    pub remaining_seconds: usize,
    pub threshold_status: &'static str,
}
/// Validate before advancing a feed's monotonic timestamp marker.
pub fn valid_time(now: i64, source: i64) -> bool {
    (-2_000..=10_000).contains(&now.saturating_sub(source))
}
impl Recorder {
    pub fn shared() -> Shared {
        Arc::new(Mutex::new(Self::default()))
    }
    pub fn observe(&mut self, now: i64, source: i64, value: Decimal) {
        if value <= Decimal::ZERO || !valid_time(now, source) {
            return;
        }
        self.samples
            .retain(|sec, _| *sec >= now.div_euclid(1000) - 180);
        let entry = self
            .samples
            .entry(source.div_euclid(1000))
            .or_insert((source, value));
        if source >= entry.0 {
            *entry = (source, value);
        }
    }
    pub fn market(&mut self, asset: Asset, market: Option<&Market>, now: i64) {
        let Some(m) = market else {
            return;
        };
        if m.status != "active"
            || !m.ticker.starts_with(&format!("{}-", asset.info().series))
            || (m.close_time - m.open_time).num_milliseconds() != 900_000
            || now < m.open_time.timestamp_millis()
            || now >= m.close_time.timestamp_millis()
        {
            return;
        }
        self.market = Some(m.clone());
        self.checked_at = now;
    }
    pub fn summary(&self, asset: Asset, now: i64, state: &str) -> Summary {
        let mut s = Summary {
            asset,
            server_time_ms: now,
            phase: "waiting_market",
            ticker: None,
            close_time_ms: None,
            target: None,
            latest_value: None,
            source_time_ms: None,
            source_state: state.to_owned(),
            distance_usd: None,
            distance_percent: None,
            observed_average: None,
            observed_seconds: 0,
            elapsed_seconds: 0,
            coverage: "-".repeat(60),
            required_remaining_average: None,
            remaining_seconds: 60,
            threshold_status: "waiting_market",
        };
        let Some(m) = &self.market else {
            return s;
        };
        let close = m.close_time.timestamp_millis();
        let start = close - 60_000;
        s.ticker = Some(m.ticker.clone());
        s.close_time_ms = Some(close);
        s.target = m
            .floor_strike
            .filter(|v| *v > Decimal::ZERO && m.strike_type.as_deref() == Some("greater_or_equal"));
        s.phase = if now >= close {
            "closed"
        } else if now.saturating_sub(self.checked_at) > 45_000 {
            "metadata_stale"
        } else if now >= start {
            "final_minute"
        } else {
            "before_final"
        };
        s.threshold_status = s.phase;
        if s.phase == "metadata_stale" {
            return s;
        }
        let latest = self
            .samples
            .values()
            .rev()
            .find(|(t, _)| *t >= m.open_time.timestamp_millis() && *t < close && *t <= now);
        if let Some((t, v)) = latest {
            s.latest_value = Some(*v);
            s.source_time_ms = Some(*t);
            if state == "live" && valid_time(now, *t) && now < close {
                s.distance_usd = s.target.map(|target| *v - target);
                s.distance_percent = s
                    .distance_usd
                    .zip(s.target)
                    .map(|(d, t)| (d / t * Decimal::ONE_HUNDRED).round_dp(6));
            }
        }
        s.elapsed_seconds = ((now.min(close) - start).max(0) / 1000).min(60) as usize;
        let mut total = Decimal::ZERO;
        s.coverage = (0..60)
            .map(|i| {
                if i >= s.elapsed_seconds {
                    '-'
                } else if let Some((_, v)) = self.samples.get(&(start.div_euclid(1000) + i as i64))
                {
                    total += *v;
                    s.observed_seconds += 1;
                    '1'
                } else {
                    '0'
                }
            })
            .collect();
        if s.observed_seconds > 0 {
            s.observed_average = Some((total / Decimal::from(s.observed_seconds)).round_dp(8));
        }
        s.remaining_seconds = 60 - s.elapsed_seconds;
        if s.phase == "final_minute" {
            s.threshold_status = if s.target.is_none() {
                "invalid_target"
            } else if state != "live" || s.distance_usd.is_none() {
                "stale_reference"
            } else if s.observed_seconds != s.elapsed_seconds {
                "missing_seconds"
            } else {
                "available"
            };
            if s.threshold_status == "available" {
                s.required_remaining_average = s
                    .target
                    .and_then(|target| target.checked_mul(Decimal::from(60)))
                    .and_then(|full| full.checked_sub(total))
                    .and_then(|remaining| remaining.checked_div(Decimal::from(s.remaining_seconds)))
                    .map(|required| {
                        required.round_dp_with_strategy(
                            asset.info().reference_precision,
                            RoundingStrategy::ToPositiveInfinity,
                        )
                    });
                if s.required_remaining_average.is_none() {
                    s.threshold_status = "calculation_unavailable";
                } else if s
                    .required_remaining_average
                    .is_some_and(|n| n <= Decimal::ZERO)
                {
                    s.threshold_status = "any_positive_average";
                }
            }
        }
        s
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn market() -> Market {
        serde_json::from_value(serde_json::json!({"ticker":"KXBTC15M-TEST-00","title":"BTC","status":"active","open_time":"2026-11-09T20:00:00Z","close_time":"2026-11-09T20:15:00Z","floor_strike":"100","strike_type":"greater_or_equal"})).unwrap()
    }
    fn setup() -> (Recorder, i64) {
        let m = market();
        let start = m.close_time.timestamp_millis() - 60_000;
        let mut r = Recorder::default();
        r.market(Asset::Btc, Some(&m), start);
        (r, start)
    }
    #[test]
    fn completed_seconds_equal_weight_latest_sample_and_visible_gaps() {
        let (mut r, t) = setup();
        for (dt, value) in [(100, 100), (800, 102), (500, 500), (2100, 106), (3100, 108)] {
            r.observe(t + dt, t + dt, Decimal::from(value));
        }
        let s = r.summary(Asset::Btc, t + 3500, "live");
        assert_eq!(s.elapsed_seconds, 3);
        assert_eq!(s.observed_seconds, 2);
        assert_eq!(s.observed_average, Some(Decimal::from(104)));
        assert!(s.coverage.starts_with("101-"));
        assert_eq!(s.distance_usd, Some(Decimal::from(8)));
    }
    #[test]
    fn boundaries_late_join_close_and_rollover_do_not_fill_missing_seconds() {
        let (mut r, t) = setup();
        r.observe(t - 1, t - 1, Decimal::from(90));
        assert_eq!(r.summary(Asset::Btc, t - 1, "live").observed_seconds, 0);
        r.observe(t + 59_900, t + 59_900, Decimal::from(110));
        r.observe(t + 60_000, t + 60_000, Decimal::from(999));
        let s = r.summary(Asset::Btc, t + 60_000, "live");
        assert_eq!(s.phase, "closed");
        assert_eq!(s.observed_seconds, 1);
        assert_eq!(s.observed_average, Some(Decimal::from(110)));
        assert_eq!(s.distance_usd, None);
        let mut next = market();
        next.open_time = next.close_time;
        next.close_time += chrono::Duration::minutes(15);
        next.ticker = "KXBTC15M-NEXT-00".into();
        r.market(Asset::Btc, Some(&next), t + 60_000);
        assert_eq!(
            r.summary(Asset::Btc, t + 60_000, "live").observed_seconds,
            0
        );
    }
    #[test]
    fn stale_future_invalid_target_and_asset_metadata_are_rejected() {
        let (mut r, t) = setup();
        assert!(!valid_time(t, t + 2001));
        assert!(!valid_time(t, t - 10001));
        r.observe(t, t + 2001, Decimal::from(900));
        r.observe(t, t, Decimal::ZERO);
        assert!(r.samples.is_empty());
        r.observe(t, t, Decimal::from(110));
        assert_eq!(r.summary(Asset::Btc, t + 1000, "stale").distance_usd, None);
        assert_eq!(
            r.summary(Asset::Btc, t + 46000, "live").phase,
            "metadata_stale"
        );
        let mut wrong = market();
        wrong.strike_type = Some("less".into());
        r.market(Asset::Btc, Some(&wrong), t);
        assert_eq!(r.summary(Asset::Btc, t + 1000, "live").target, None);
        let mut eth = Recorder::default();
        eth.market(Asset::Eth, Some(&market()), t);
        assert_eq!(eth.summary(Asset::Eth, t, "live").phase, "waiting_market");
    }
    #[test]
    fn full_minute_counts_sixty_seconds_once() {
        let (mut r, t) = setup();
        for second in 0..60 {
            let source = t + second * 1000 + 100;
            r.observe(source, source, Decimal::from(100 + second));
        }
        let s = r.summary(Asset::Btc, t + 60_000, "live");
        assert_eq!(s.observed_seconds, 60);
        assert_eq!(s.elapsed_seconds, 60);
        assert_eq!(s.coverage, "1".repeat(60));
        assert_eq!(s.observed_average.unwrap().to_string(), "129.50");
    }
    #[test]
    fn threshold_uses_unrounded_sum_and_all_remaining_seconds() {
        let (mut r, t) = setup();
        for sec in 0..40 {
            let at = t + sec * 1000 + 100;
            r.observe(at, at, Decimal::new(10010, 2));
        }
        r.market(Asset::Btc, Some(&market()), t + 40000);
        let s = r.summary(Asset::Btc, t + 40000, "live");
        assert_eq!(s.threshold_status, "available");
        assert_eq!(s.remaining_seconds, 20);
        assert_eq!(s.required_remaining_average, Some(Decimal::new(9980, 2)));
        r.samples.remove(&(t / 1000 + 3));
        assert_eq!(
            r.summary(Asset::Btc, t + 40000, "live").threshold_status,
            "missing_seconds"
        );
        assert_eq!(
            r.summary(Asset::Btc, t + 40000, "live")
                .required_remaining_average,
            None
        );
    }

    #[test]
    fn threshold_clears_on_disconnect_close_and_before_the_final_minute() {
        let (mut r, t) = setup();
        r.observe(t, t, Decimal::from(100));
        assert_eq!(
            r.summary(Asset::Btc, t, "live").required_remaining_average,
            Some(Decimal::from(100))
        );
        assert_eq!(
            r.summary(Asset::Btc, t, "reconnecting")
                .required_remaining_average,
            None
        );
        assert_eq!(
            r.summary(Asset::Btc, t - 1, "live")
                .required_remaining_average,
            None
        );
        assert_eq!(
            r.summary(Asset::Btc, t + 60000, "live")
                .required_remaining_average,
            None
        );
        let mut invalid = market();
        invalid.strike_type = Some("less".into());
        r.market(Asset::Btc, Some(&invalid), t);
        assert_eq!(
            r.summary(Asset::Btc, t, "live").threshold_status,
            "invalid_target"
        );
    }

    #[test]
    fn threshold_rounds_up_and_handles_nonpositive_requirements() {
        let (mut r, t) = setup();
        r.observe(t, t, Decimal::new(9999, 2));
        assert_eq!(
            r.summary(Asset::Btc, t + 1000, "live")
                .required_remaining_average,
            Some(Decimal::new(10001, 2))
        );
        r.observe(t + 1, t + 1, Decimal::new(100001, 3));
        assert_eq!(
            r.summary(Asset::Btc, t + 1000, "live")
                .required_remaining_average,
            Some(Decimal::from(100))
        );
        r.observe(t + 100, t + 100, Decimal::from(10000));
        assert_eq!(
            r.summary(Asset::Btc, t + 1000, "live").threshold_status,
            "any_positive_average"
        );
    }

    #[test]
    fn memory_is_bounded_without_inventing_observations() {
        let mut r = Recorder::default();
        for sec in 0..1000 {
            r.observe(sec * 1000, sec * 1000, Decimal::ONE);
        }
        assert!(r.samples.len() <= 181);
    }
}
