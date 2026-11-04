//! Explicit crypto-only allowlist shared by discovery, streams, history and the UI.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum Asset {
    #[default]
    Btc,
    Eth,
    Sol,
}
#[derive(Serialize)]
pub struct AssetInfo {
    pub symbol: Asset,
    pub name: &'static str,
    pub series: &'static str,
    pub index: &'static str,
    pub reference_precision: u32,
}
impl Asset {
    pub const ALL: [Self; 3] = [Self::Btc, Self::Eth, Self::Sol];
    pub fn info(self) -> AssetInfo {
        match self {
            Self::Btc => AssetInfo {
                symbol: self,
                name: "Bitcoin",
                series: "KXBTC15M",
                index: "BRTI",
                reference_precision: 2,
            },
            Self::Eth => AssetInfo {
                symbol: self,
                name: "Ethereum",
                series: "KXETH15M",
                index: "ETHUSD_RTI",
                reference_precision: 2,
            },
            Self::Sol => AssetInfo {
                symbol: self,
                name: "Solana",
                series: "KXSOL15M",
                index: "SOLUSD_RTI",
                reference_precision: 4,
            },
        }
    }
}
#[derive(Default, Deserialize)]
pub struct Selection {
    #[serde(default)]
    pub asset: Asset,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_keeps_market_series_and_index_distinct() {
        assert_eq!(Asset::Eth.info().series, "KXETH15M");
        assert_eq!(Asset::Sol.info().index, "SOLUSD_RTI");
        assert!(serde_json::from_str::<Selection>(r#"{"asset":"SPORTS"}"#).is_err());
        assert_eq!(
            serde_json::from_str::<Selection>("{}").unwrap().asset,
            Asset::Btc
        );
    }
}
