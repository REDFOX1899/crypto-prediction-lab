# Read-only 15-minute combo lab

Scope: BTC, ETH and SOL only. Each asset can contribute one UP (YES at or above target) or DOWN (NO below target) leg. At least two legs must share identical open and close timestamps, have exactly 900 seconds duration, and still be active. Current market metadata must have been checked within 45 seconds. Rollover clears selected legs and the assumed combo price.

This is a research interface, not an execution integration. It does not send RFQs, create markets, submit trades, expose account balances or change API permissions. Whether any exact combination is eligible on Kalshi is not verified. Individual asks are shown separately and are never multiplied to fabricate a combo quote or probability.

Kalshi's official combo documentation describes a dedicated market/order book and RFQ pricing. A combo can pay up to $1 per contract, based on component settlement values. For binary legs settling at $1 or $0, all must settle at $1 for the full $1 payout; a zero component gives zero. Other settlement provisions can differ. This workspace shows hypothetical binary scenarios and links the official explanation.

The optional calculator requires a user-entered assumed combo price between 0.1 and 99.9 cents and a hypothetical budget up to $1,000. It uses whole contracts, excludes fees, slippage and fill availability, and shows cost, payout if all legs win, and profit before fees. These inputs stay in browser memory and are never submitted to a server. A scenario is not a live offer, expected value or recommendation.

Historical comparison joins stored outcomes on both opening and closing times. Only 15-minute binary records with the correct asset count. The last 96 slots are fixed to the current completed-window boundary. Missing legs remain incomplete; they are not treated as losses or wins. The reported percentage uses complete joined observations only. It is a descriptive frequency, not a forecast or a fair price. BTC/ETH/SOL dependence is explicitly noted; adding legs does not improve the chance that every pick wins.

Resource use: the combo view requests snapshots every 20 seconds and archive summaries every minute. It subscribes to the existing three shared market feeds, not extra upstream Kalshi sockets. Switching views closes the inactive view's streams and stops its refresh timers. No new cloud resource or write endpoint was added.

Checks: same-window and duration validation; expired/insufficient selections; missing, wrong-asset and mismatched historical records; hypothetical-price bounds and whole-contract arithmetic; browser two-leg selection, payout example and return to single-market navigation.

Reference checked 8 November 2026:
https://help.kalshi.com/en/articles/13823820-combos

## Release verification

Published 8 November 2026 to https://crypto-prediction-lab-308289525742.us-central1.run.app/?view=combos
Revision: crypto-prediction-lab-00004-q7n
Image tag: combos-20261108
Cloud Build: 77357255-5962-47e5-bd18-d373f5174fdd

Checks passed: 21 Rust tests, 10 JavaScript tests, formatting, clippy, local two-leg selection and scenario input, return to single-market navigation, and hosted three-leg selection with live quotes. The hosted history panel joined 96 actual stored windows for the tested three-leg selection. The collector service and its schedule were not redeployed or changed. No RFQ or order was sent.

## Public combo order books — 8 November 2026

GET /api/combos/quote?btc=yes&eth=no&sol=yes accepts only these three optional directions, requires at least two, and rejects unknown fields. The server discovers the currently active underlying 900-second markets, then requests public GET /markets with mve_filter=only and a bounded current-expiry range. It checks at most four pages of 1,000 rows (4 MiB per response), rejects repeated cursors, and reports incomplete search separately from no match. No market, RFQ, quote or order is created. No additional Kalshi permission is used.

Matching requires exactly the same native underlying tickers, selected YES/NO sides, leg count, active status and closing time. A wrong direction, extra leg, previous window or expired market cannot match. Multiple matching markets can exist: up to three are checked, choosing the lowest available ask among checked books, otherwise the highest bid. This is not an exhaustive exchange-wide best-price claim. The response reports matching/check/error counts and identifies the single book displayed. YES asks are derived as $1 minus NO bids with their original quantities; duplicate levels are aggregated, invalid/zero prices and sizes omitted. Bid-only and empty books never produce an invented ask.

Shared single-flight catalog cache: 30 seconds; per-selection book cache: five seconds; discovery-error cooldown: 30 seconds. New picks and rollover invalidate the browser quote. Responses from old selections are ignored. Snapshots older than 15 seconds are hidden. Provider failures, partial search, no matching market, empty books and bid-only liquidity are distinct states. The calculator remains an independent assumed-price scenario.

Current source validation found active BTC/ETH/SOL combo markets and successfully read empty public books. Private RFQ quotations remain unavailable to this read-only observer. A public book may remain empty even when market makers respond to private RFQs.

Production delivery now uses one shared /api/feeds request per visible tab every two seconds (15 seconds while hidden), carrying the latest upstream market and reference feed states for all three assets. Upstream authenticated subscriptions remain shared and read-only. Local default remains SSE. Production disables the old long-lived SSE routes, avoiding a browser holding the single low-cost instance slot open. Combo books are five-second public snapshots, not order-by-order WebSocket updates. Browser reconnects can miss upstream events; this is not a lossless recorder.

Release verification: public revision crypto-prediction-lab-00005-dnn, image combo-books-20261108, build b61dae91-45a2-478c-b2b3-7f7248559253. 28 Rust tests and 11 JavaScript tests passed, plus formatting and clippy. Hosted exact two-/three-leg lookups returned actual matching empty books. All three authenticated upstream market and reference feeds were live; reference values were approximately 0.7 seconds old during verification. Navigation back to the SOL chart showed a current reference value and polled live book. Public archive returned 96 SOL records. The current newly settled slot may remain incomplete until scheduled collection. No RFQ or order was sent.

## Compact combo builder layout

The screenshot-inspired layout uses compact BTC/ETH/SOL cards with percentage-form individual ask prices, selected-direction tints, a desktop sticky preview slip, per-pick remove controls, a visible preview budget and +$5 shortcut. Mobile stacks the cards and slip. The public order book stays expanded. The separate assumed-price calculator starts collapsed. No new assets, API permissions, paid services, subscriptions, trading controls or cloud resources were added.

A public-ask payout preview requires fresh exact matching legs, a valid window and displayed ask quantity. It limits whole contracts to the smaller of the budget-supported count and displayed size; the headline shows the cost of those contracts and conditional gross payout before fees. No available public ask leaves payout blank. Individual percentage quotes are not multiplied or interpreted as combo probability. The budget is local browser state. Selected rows remain stable between price updates so keyboard focus and remove actions survive live refreshes.

Layout release verification: deployed public revision crypto-prediction-lab-00006-6tg using combo-layout-20261108, build 48a9fc9d-b771-4eca-9e2b-300d47f91dda. Twelve JavaScript checks passed; Rust server sources and API permissions are unchanged from the previous tested release. Local desktop/mobile screenshots, percentage buttons, +$5 increment and individual-pick removal passed. Quote preview tests cover source staleness, exact leg matching, invalid budget and visible-size limits. The existing quarter-CPU and one-instance configuration is retained.
