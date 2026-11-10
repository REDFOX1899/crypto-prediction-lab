//! Public projection of a saved three-day study. The operator retains the row-level partitions.
use crate::assets::Asset;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::{io::Read, net::IpAddr, path::Path};

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Board {
    schema_version: u8,
    start_day: NaiveDate,
    end_day: NaiveDate,
    series: [Series; 3],
}

#[derive(Clone, Deserialize, Serialize)]
struct Series {
    asset: Asset,
    cutoffs: [Cutoff; 3],
}

#[derive(Clone, Deserialize, Serialize)]
struct Cutoff {
    minute: u8,
    expected: u16,
    settled: u16,
    bins: [Bin; 5],
}

#[derive(Clone, Deserialize, Serialize)]
struct Bin {
    n: u16,
    yes: u16,
    sum_ask_bps: u32,
}

impl Board {
    pub(crate) fn local_preview(address: IpAddr, path: &Path) -> Option<Self> {
        if !address.is_loopback() {
            return None;
        }
        let file = std::fs::File::open(path).ok()?;
        let mut bytes = Vec::new();
        file.take(32_769).read_to_end(&mut bytes).ok()?;
        if bytes.len() > 32_768 {
            return None;
        }
        serde_json::from_slice::<Self>(&bytes)
            .ok()
            .filter(Self::valid)
    }

    pub(crate) fn valid(&self) -> bool {
        self.schema_version == 1
            && (self.end_day - self.start_day).num_days() == 2
            && self.series.iter().zip(Asset::ALL).all(|(s, asset)| {
                s.asset == asset
                    && s.cutoffs.iter().zip([5, 10, 14]).all(|(c, minute)| {
                        c.minute == minute
                            && c.expected == 288
                            && c.settled <= c.expected
                            && c.bins.iter().map(|b| u32::from(b.n)).sum::<u32>()
                                <= u32::from(c.settled)
                            && c.bins.iter().enumerate().all(|(i, b)| {
                                let lower = (i as u32 * 2000).max(1);
                                let upper = (i as u32 + 1) * 2000 - 1;
                                b.yes <= b.n
                                    && (lower * u32::from(b.n)..=upper * u32::from(b.n))
                                        .contains(&b.sum_ask_bps)
                            })
                    })
            })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::{Value, json};

    pub(crate) fn example() -> Value {
        let bins = vec![json!({"n":0,"yes":0,"sum_ask_bps":0}); 5];
        let cutoffs = [5, 10, 14]
            .map(|minute| json!({"minute":minute,"expected":288,"settled":3,"bins":bins}));
        let series = Asset::ALL.map(|asset| json!({"asset":asset,"cutoffs":cutoffs}));
        json!({"schema_version":1,"start_day":"2026-11-07","end_day":"2026-11-09","series":series})
    }

    #[test]
    fn rejects_inconsistent_counts_prices_dates_and_cutoffs() {
        let data = example();
        assert!(
            serde_json::from_value::<Board>(data.clone())
                .unwrap()
                .valid()
        );
        for (path, bad) in [
            ("/end_day", json!("2026-11-10")),
            ("/series/1/asset", json!("BTC")),
            ("/series/0/cutoffs/0/minute", json!(3)),
            (
                "/series/0/cutoffs/0/bins/0",
                json!({"n":1,"yes":2,"sum_ask_bps":50}),
            ),
            (
                "/series/0/cutoffs/0/bins/0",
                json!({"n":4,"yes":1,"sum_ask_bps":250}),
            ),
            (
                "/series/0/cutoffs/0/bins/1",
                json!({"n":1,"yes":1,"sum_ask_bps":1999}),
            ),
        ] {
            let mut updated = data.clone();
            *updated.pointer_mut(path).unwrap() = bad;
            assert!(!serde_json::from_value::<Board>(updated).unwrap().valid());
        }
    }
}
