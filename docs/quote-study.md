# Historical YES quotes and official outcomes

Home compares completed BTC, ETH and SOL contracts at minutes 5, 10 and 14 after open. The initial study uses windows **opened November 7–9, 2026 UTC**. The day includes the contract that opens at 23:45 and closes at midnight the following day.

## Reading the chart

For each asset and cutoff, take the closing YES ask from the one-minute candle ending exactly at that cutoff. Require a unique candle with valid YES ask OHLC and a closing ask strictly between $0 and $1. Group by price in five 20-cent bands. A price on an interior boundary belongs to the higher band. Preserve missing observations. A contract contributes at most once to each asset/cutoff view.

- **Bar height** is the count of official YES results in the band divided by its eligible contract count.
- **Diamond height** is the average ask expressed as a percentage of the $1 binary payout. It gives the break-even win rate for a hypothetical equal-contract-size purchase at every observed ask, before fees.
- **Gap** is the historical YES share minus that break-even rate, in percentage points. The mean ask, counts and spread all have distinct units.
- **Coverage** counts eligible quotes, settled binary windows without an eligible quote, and expected windows without a recorded binary result. Counts at different cutoffs can refer to the same contract. A missing quote and a 0% YES share render differently.
- **Live bid–ask spread**, below the chart, is the YES ask minus YES bid in cents from the same fresh book, ticker update or market snapshot. A locked quote gives 0¢. Crossed, stale or missing sides suppress the spread.

For the initial BTC study at minute 5, 37 of the 67 contracts quoted from 40¢ up to 60¢ settled YES. The YES share was 55.2%, the average ask was 50.3¢ and the before-fee difference was +4.9 percentage points. These retrospective candles supply neither executable size nor contemporaneous receipt times. Nearby contracts share market conditions. A gap observed across these three days is insufficient evidence of a repeatable advantage.

The quote, interval-end timestamp and official result come from the stored Kalshi contract and candlestick records. This study excludes CF observations, CMC aggregates and Blockworks data. [Kalshi candlestick fields](https://docs.kalshi.com/api-reference/market/get-market-candlesticks)

## Saved data

The operator reads nine asset-days from the existing named `crypto-prediction-lab` Firestore database in project `gitunderstand`. Nine bounded queries share a single Firestore read time to keep all partitions consistent. A query returning more than 96 contracts for an asset-day stops the build. Identity checks link the document, market, candle envelope and summary. Revising a source document changes its partition revision.

The `quote_studies_v1` collection contains immutable asset-day documents identified by date, symbol and SHA-256 of their normalized contents. Each holds a features map by ticker with contract open time, observed YES asks in ten-thousandths of $1, archive capture time and source document revision. Official YES/NO labels live in a separate map keyed by ticker. Missing cutoff keys mean no eligible observation. The archive capture and revision times describe retrieval after settlement.

The `latest` manifest references an exact set of partitions and carries the compact aggregate. Cells retain the eligible count, YES count and sum of ask units, which allow exact recomputation before display rounding. A conditional, atomic commit publishes new partitions with the manifest. The operator reads the saved documents back and verifies their contents. A rerun with unchanged inputs reuses the existing partitions. The original contract archive remains the source of record. [Firestore commit semantics](https://firebase.google.com/docs/firestore/reference/rest/v1/projects.databases.documents/commit)

`GET /api/research/quote-study` reads one fixed manifest through the dashboard's existing viewer identity. The server validates the three-day projection, returns only chart fields and caches it for ten minutes. Asset, cutoff and band changes use the same small browser response. The operator is the only writer. This batch has no schedule and makes no provider calls.

## Running the study

With the existing operator's Google Cloud login, run from the workspace root. Choose the last of three complete UTC dates. A build without `--publish` only reads cloud data. The local preview is optional and belongs under the ignored `data/` directory.

```sh
mkdir -p data
python3 scripts/build_quote_study.py --end-day 2026-11-09 --preview data/quote-study.json
python3 scripts/build_quote_study.py --end-day 2026-11-09 --publish --preview data/quote-study.json
```

Both `cargo run -p dashboard` on loopback without a configured archive and `python3 scripts/preview-data-lab.py --port 3013` can show the local aggregate. The latter also reads the public dashboard feeds. A network-bound Rust server reads saved studies only through its configured archive.

## Next experiment

Freeze the price bands and cutoffs before evaluating later complete UTC days. Keep all three assets from the same time block together. Report sample sizes and missing coverage by cutoff, and estimate uncertainty over separate days. The next dataset for an entry study needs prospective, timestamped bid/ask observations with available size, fee assumptions and a documented latency rule. Compare any candidate measure against contemporaneous quotes on untouched dates before claiming accuracy or profitability.
