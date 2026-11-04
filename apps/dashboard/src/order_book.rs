//! Deterministic in-memory Kalshi book. Prices and quantities remain decimal values.
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
pub struct BookView {
    pub yes_dollars: Vec<(Decimal, Decimal)>,
    pub no_dollars: Vec<(Decimal, Decimal)>,
}

#[derive(Debug, Deserialize)]
pub struct Snapshot {
    pub market_ticker: String,
    pub yes_dollars_fp: Vec<(Decimal, Decimal)>,
    pub no_dollars_fp: Vec<(Decimal, Decimal)>,
}

#[derive(Debug, Deserialize)]
pub struct Delta {
    pub market_ticker: String,
    pub price_dollars: Decimal,
    pub delta_fp: Decimal,
    pub side: String,
}

#[derive(Default)]
pub struct Book {
    yes: BTreeMap<Decimal, Decimal>,
    no: BTreeMap<Decimal, Decimal>,
    subscription: Option<u64>,
    sequence: Option<u64>,
}

impl Book {
    pub fn snapshot(&mut self, snapshot: Snapshot, sid: u64, sequence: u64) -> Result<(), ()> {
        fn levels(items: Vec<(Decimal, Decimal)>) -> Result<BTreeMap<Decimal, Decimal>, ()> {
            let mut result = BTreeMap::new();
            for (price, quantity) in items {
                if !(Decimal::ZERO..=Decimal::ONE).contains(&price)
                    || quantity <= Decimal::ZERO
                    || result.insert(price, quantity).is_some()
                {
                    return Err(());
                }
            }
            Ok(result)
        }
        // Build both sides before replacing the previous state.
        let yes = levels(snapshot.yes_dollars_fp)?;
        let no = levels(snapshot.no_dollars_fp)?;
        self.yes = yes;
        self.no = no;
        self.subscription = Some(sid);
        self.sequence = Some(sequence);
        Ok(())
    }

    pub fn delta(&mut self, delta: Delta, sid: u64, sequence: u64) -> Result<(), ()> {
        if self.subscription != Some(sid)
            || self.sequence.and_then(|s| s.checked_add(1)) != Some(sequence)
            || !(Decimal::ZERO..=Decimal::ONE).contains(&delta.price_dollars)
        {
            return Err(());
        }
        let side = match delta.side.as_str() {
            "yes" => &mut self.yes,
            "no" => &mut self.no,
            _ => return Err(()),
        };
        let quantity = side
            .get(&delta.price_dollars)
            .copied()
            .unwrap_or(Decimal::ZERO)
            .checked_add(delta.delta_fp)
            .ok_or(())?;
        if quantity < Decimal::ZERO {
            return Err(());
        }
        if quantity == Decimal::ZERO {
            side.remove(&delta.price_dollars);
        } else {
            side.insert(delta.price_dollars, quantity);
        }
        self.sequence = Some(sequence);
        Ok(())
    }

    pub fn view(&self) -> BookView {
        fn top(side: &BTreeMap<Decimal, Decimal>) -> Vec<(Decimal, Decimal)> {
            side.iter().rev().take(10).map(|(&p, &q)| (p, q)).collect()
        }
        BookView {
            yes_dollars: top(&self.yes),
            no_dollars: top(&self.no),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn decimal(s: &str) -> Decimal {
        s.parse().unwrap()
    }
    fn delta(side: &str, price: &str, quantity: &str) -> Delta {
        Delta {
            market_ticker: "TEST".into(),
            side: side.into(),
            price_dollars: decimal(price),
            delta_fp: decimal(quantity),
        }
    }
    fn book() -> Book {
        let mut b = Book::default();
        b.snapshot(
            Snapshot {
                market_ticker: "TEST".into(),
                yes_dollars_fp: vec![(decimal("0.42"), decimal("10.5"))],
                no_dollars_fp: vec![],
            },
            8,
            20,
        )
        .unwrap();
        b
    }
    #[test]
    fn updates_remove_and_sort_levels() {
        let mut b = book();
        b.delta(delta("yes", "0.42", "-2.5"), 8, 21).unwrap();
        b.delta(delta("yes", "0.43", "1.25"), 8, 22).unwrap();
        b.delta(delta("yes", "0.42", "-8"), 8, 23).unwrap();
        assert_eq!(
            b.view().yes_dollars,
            vec![(decimal("0.43"), decimal("1.25"))]
        );
    }
    #[test]
    fn rejects_gap_wrong_subscription_negative_quantity_and_missing_snapshot() {
        let mut b = book();
        assert!(b.delta(delta("yes", "0.42", "1"), 8, 22).is_err());
        assert!(b.delta(delta("yes", "0.42", "1"), 9, 21).is_err());
        assert!(b.delta(delta("yes", "0.42", "-11"), 8, 21).is_err());
        assert!(
            Book::default()
                .delta(delta("no", "0.5", "1"), 8, 1)
                .is_err()
        );
        assert_eq!(b.view().yes_dollars[0].1, decimal("10.5"));
    }
    #[test]
    fn later_snapshot_replaces_book_and_sequence() {
        let mut b = book();
        b.snapshot(
            Snapshot {
                market_ticker: "TEST".into(),
                yes_dollars_fp: vec![],
                no_dollars_fp: vec![(decimal("0.3"), decimal("2"))],
            },
            9,
            50,
        )
        .unwrap();
        assert!(b.view().yes_dollars.is_empty());
        b.delta(delta("no", "0.3", "1"), 9, 51).unwrap();
        assert_eq!(b.view().no_dollars[0].1, decimal("3"));
    }
}
