//! Real-time market-viewing latency telemetry (Kalshi WS, CF Reference wire lag, /api/feeds).
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

const RING_CAP: usize = 64;

#[derive(Clone, Debug, Serialize)]
pub struct RingStats {
    pub latest_ms: Option<f64>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub samples: usize,
    pub recent_ms: Vec<f64>,
}

#[derive(Default)]
struct FloatRing {
    values: VecDeque<f64>,
}

impl FloatRing {
    fn push(&mut self, v: f64) {
        if !v.is_finite() || v < 0.0 {
            return;
        }
        if self.values.len() >= RING_CAP {
            self.values.pop_front();
        }
        self.values.push_back((v * 100.0).round() / 100.0);
    }

    fn stats(&self) -> RingStats {
        let samples = self.values.len();
        if samples == 0 {
            return RingStats {
                latest_ms: None,
                p50_ms: None,
                p95_ms: None,
                samples: 0,
                recent_ms: Vec::new(),
            };
        }
        let mut sorted: Vec<f64> = self.values.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        let pct = |q: f64| -> f64 {
            let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
            sorted[idx.min(sorted.len() - 1)]
        };
        RingStats {
            latest_ms: self.values.back().copied(),
            p50_ms: Some(pct(0.50)),
            p95_ms: Some(pct(0.95)),
            samples,
            recent_ms: self.values.iter().rev().take(32).copied().rev().collect(),
        }
    }
}

pub struct Monitor {
    feeds_srv: Mutex<FloatRing>,
    cf_wire: Mutex<FloatRing>,
    ws_ticks: AtomicU64,
    cf_ticks: AtomicU64,
}

impl Monitor {
    pub fn new() -> Self {
        Self {
            feeds_srv: Mutex::new(FloatRing::default()),
            cf_wire: Mutex::new(FloatRing::default()),
            ws_ticks: AtomicU64::new(0),
            cf_ticks: AtomicU64::new(0),
        }
    }

    pub fn record_feeds_ms(&self, ms: f64) {
        self.feeds_srv
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(ms);
    }

    pub fn record_cf_wire_ms(&self, ms: i64) {
        self.cf_ticks.fetch_add(1, Ordering::Relaxed);
        if (0..=15_000).contains(&ms) {
            self.cf_wire
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(ms as f64);
        }
    }

    pub fn record_ws_tick(&self) {
        self.ws_ticks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn feeds_stats(&self) -> RingStats {
        self.feeds_srv
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stats()
    }

    pub fn cf_wire_stats(&self) -> RingStats {
        self.cf_wire
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stats()
    }

    pub fn counts(&self) -> (u64, u64) {
        (
            self.ws_ticks.load(Ordering::Relaxed),
            self.cf_ticks.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_computes_percentiles_and_bounds_capacity() {
        let mon = Monitor::new();
        for i in 1..=100 {
            mon.record_feeds_ms(i as f64);
        }
        let s = mon.feeds_stats();
        assert_eq!(s.samples, RING_CAP);
        assert_eq!(s.latest_ms, Some(100.0));
        assert!(s.p50_ms.unwrap() >= 65.0 && s.p50_ms.unwrap() <= 72.0);
        assert!(s.p95_ms.unwrap() >= 95.0);
        assert_eq!(s.recent_ms.len(), 32);

        mon.record_cf_wire_ms(145);
        mon.record_cf_wire_ms(-5); // ignored clock skew
        mon.record_ws_tick();
        assert_eq!(mon.cf_wire_stats().latest_ms, Some(145.0));
        assert_eq!(mon.counts(), (1, 2));
    }
}
