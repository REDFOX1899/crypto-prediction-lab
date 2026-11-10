//! Timing contracts for the two private, minute-scheduled handlers.
use crate::{assets::Asset, checkpoint};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedCycle {
    pub open_time: i64,
    #[serde(default)]
    pub tickers: HashMap<Asset, String>,
}
impl PreparedCycle {
    pub fn valid(&self) -> bool {
        self.open_time.rem_euclid(900) == 0
            && self
                .tickers
                .iter()
                .all(|(a, t)| checkpoint::valid_hint(*a, t))
    }
    pub fn ready_at(&self, now: i64) -> bool {
        self.valid() && now >= self.open_time && now < self.open_time + 55
    }
}
/// Original schedule time remains bound to its quarter through retries.
pub fn intended_open(scheduled: i64) -> Option<i64> {
    let open = (scheduled + 60).div_euclid(900) * 900;
    (scheduled >= open - 60 && scheduled < open + 60).then_some(open)
}
pub fn phase_capture_at(
    open: i64,
    now: i64,
    phase: checkpoint::Phase,
) -> Result<i64, &'static str> {
    let earliest = match phase {
        checkpoint::Phase::Opening => open - 15,
        checkpoint::Phase::ExecutionCandidate => open + 5,
    };
    if now < earliest {
        return Err("early");
    }
    if now >= open + phase.deadline() {
        return Err("late");
    }
    Ok(open
        + if phase == checkpoint::Phase::Opening {
            5
        } else {
            45
        })
}
pub fn scheduled_open(
    header: Option<&str>,
    now: DateTime<Utc>,
) -> Result<Option<i64>, &'static str> {
    let scheduled = match header {
        Some(v) => DateTime::parse_from_rfc3339(v)
            .map_err(|_| "Invalid schedule timestamp")?
            .timestamp(),
        None => now.timestamp(),
    };
    Ok(intended_open(scheduled))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retries_remain_bound_to_the_original_quarter() {
        for t in [840, 870, 899, 900, 930, 959] {
            assert_eq!(intended_open(t), Some(900));
        }
        assert_eq!(intended_open(800), None);
        assert_eq!(intended_open(960), None);
        let now = DateTime::from_timestamp(1805, 0).unwrap();
        assert_eq!(
            scheduled_open(Some("1970-01-01T00:14:00Z"), now),
            Ok(Some(900))
        );
        assert_eq!(scheduled_open(None, now), Ok(Some(1800)));
        assert!(scheduled_open(Some("bad"), now).is_err());
        let p = PreparedCycle {
            open_time: 900,
            ..Default::default()
        };
        assert!(!p.ready_at(899));
        assert!(p.ready_at(905));
        assert!(!p.ready_at(955));
        assert!(!p.ready_at(1805));
    }
    #[test]
    fn admission_keeps_waits_inside_existing_request_budget() {
        use checkpoint::Phase::{ExecutionCandidate, Opening};
        assert_eq!(phase_capture_at(900, 884, Opening), Err("early"));
        assert_eq!(phase_capture_at(900, 885, Opening), Ok(905));
        assert_eq!(phase_capture_at(900, 930, Opening), Err("late"));
        assert_eq!(phase_capture_at(900, 904, ExecutionCandidate), Err("early"));
        assert_eq!(phase_capture_at(900, 905, ExecutionCandidate), Ok(945));
        assert_eq!(phase_capture_at(900, 955, ExecutionCandidate), Err("late"));
        assert_eq!(phase_capture_at(900, 1805, Opening), Err("late"));
    }

    #[test]
    fn hints_cannot_select_other_assets_or_caller_cutoffs() {
        let mut p = PreparedCycle {
            open_time: 900,
            ..Default::default()
        };
        p.tickers.insert(Asset::Btc, "KXETH15M-OTHER".into());
        assert!(!p.valid());
        assert!(
            serde_json::from_value::<PreparedCycle>(
                serde_json::json!({"open_time":900,"cutoff":950})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<PreparedCycle>(
                serde_json::json!({"open_time":900,"tickers":{"DOGE":"OTHER"}})
            )
            .is_err()
        );
    }
}
