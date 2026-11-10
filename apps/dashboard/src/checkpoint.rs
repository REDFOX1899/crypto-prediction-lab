//! A bounded scheduled observation with actual receipt timestamps.
use crate::assets::Asset;
use chrono::{DateTime, Utc};
use kalshi_client::{Client, current_market};
use serde_json::{Value, json};

pub fn slot(now: i64) -> Option<i64> {
    let open = now.div_euclid(900) * 900;
    (600..660).contains(&(now - open)).then_some(open)
}

pub async fn capture(client: &Client, asset: Asset, open: i64) -> Value {
    let started = Utc::now();
    let result = async {
        let markets = client.open_markets(asset.info().series).await.ok()?;
        let market = current_market(&markets, Utc::now())?;
        if market.open_time.timestamp() != open || market.close_time.timestamp()-open != 900 {
            return None;
        }
        let book = client.order_book(&market.ticker).await.ok()?;
        Some(json!({"ticker":market.ticker,"target":market.floor_strike,"target_text":market.yes_sub_title,
            "yes_bids":book.yes_dollars,"no_bids":book.no_dollars}))
    }.await;
    let received = Utc::now();
    let status = if result.is_none() {
        "unavailable"
    } else if received.timestamp() >= open + 660 {
        "late"
    } else {
        "available"
    };
    json!({"schema_version":1,"asset":asset,"open_time":DateTime::from_timestamp(open,0),
        "cutoff":DateTime::from_timestamp(open+600,0),"request_started_at":started,"received_at":received,
        "seconds_after_cutoff":(received.timestamp_millis()-(open+600)*1000) as f64/1000.,
        "status":status,"observation":result,
        "source":"Kalshi public market and order-book snapshots; sequential requests, non-atomic",
        "timing":"Captured during minute 10; actual receipt follows the cutoff. Never assume availability at the exact cutoff."})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_tenth_minute_is_eligible() {
        for seconds in [0, 599, 660, 899] {
            assert_eq!(slot(1800 + seconds), None);
        }
        for seconds in [600, 615, 659] {
            assert_eq!(slot(1800 + seconds), Some(1800));
        }
    }
}
