# BTC / ETH / SOL: data-provider and collection plan

Research date: 9–10 November 2026. This is a proposed architecture and procurement shortlist, not an implemented collector, purchased subscription or latency benchmark. The public application remains read-only. Existing resources in gitunderstand and unrelated applications remain untouched.

## Recommendation

First make CF reference and Kalshi market capture durable, independently of website visitors. Add Coinbase USD trades/books next, then Kraken USD trades/books as a second venue. Evaluate one permitted derivatives feed only after measuring its additional predictive value. Consider paid normalized feeds or historical vendors when the budget or need for historical coverage justifies them.

More feeds are not automatically more predictive. The important dataset is a timestamped record of what the application could actually have known at each prediction time.

## Current gap

The project currently archives settled Kalshi contracts and available one-minute candles in Firestore. Live order books, reference observations and homepage plots mostly live in bounded memory. The two-second browser response is a sampled presentation, not a durable tick archive.

The public service uses request-based CPU allocation, fractional CPU and scale-to-zero. Google allocates CPU during request processing under that billing mode. It is not a reliable visitor-independent continuous collection service. A separate continuously running worker is needed before promising complete 24/7 capture. Source: [Cloud Run billing settings](https://docs.cloud.google.com/run/docs/configuring/billing-settings).

Record events at the server's WebSocket ingress, before watch-channel coalescing, per-second averaging or top-ten UI conversion. Recording the browser poll response would irreversibly discard the detail needed for millisecond research.

## Which source is authoritative?

- **Kalshi:** contract rules, target versions, market state, quotes/trades on its own venue and official settlement results.
- **CF Benchmarks:** BRTI, ETHUSD_RTI and SOLUSD_RTI reference publications, subject to the exact contract's rules and revision treatment.
- **Coinbase/Kraken/other exchanges:** their own trades and order books. These are candidate predictors, not replacement settlement labels.

Kalshi explains that most crypto contracts use a CF index average and that individual rules govern. Its historical pass-through can reflect later CF revisions. Preserve original observations and amendments separately. [Kalshi crypto market explanation](https://help.kalshi.com/en/articles/13823838-crypto-markets).

The CF RTI methodology uses consolidated constituent order books; averaging two exchange last-trade prices does not reproduce it. Preserve methodology and constituent-list versions. The current guide lists 200ms effective-time parameters for the three indices, but actual API entitlements must be confirmed rather than inferred from marketing text or timestamp precision. [RTI methodology, v17](https://docs.cfbenchmarks.com/CME%20CF%20Real%20Time%20Indices%20Methodology.pdf), [constituent exchanges](https://docs.cfbenchmarks.com/CME%20CF%20Constituent%20Exchanges.pdf).

## Provider shortlist

| Provider | Useful data | Decision |
|---|---|---|
| CF Benchmarks direct | Original reference values; optional consolidated bid/ask, mid and utilized-depth statistics | Highest settlement relevance; obtain license/entitlement and permitted-use quote |
| Coinbase Advanced Trade | USD spot trades, order-book changes, heartbeats | First additional live source |
| Kraken WebSocket v2 | USD spot trades and L2 books, including checksum validation | Second venue and comparison source |
| OKX public market data | Perpetual mark/index prices, funding and open interest | Optional research input; verify region, instruments and terms first |
| CoinAPI | Normalized exchange trades, quotes, books and historical access | Easier integration; standard plans exceed the $40 total budget; evaluate metered volume separately |
| Kaiko / Amberdata | Institutional normalized market data and derivatives datasets | Evaluate as one vendor group; paid upgrade rather than the first budget choice |
| Coin Metrics | Market quotes, order books, funding, open interest and network datasets | Alternative paid research/data platform; obtain precise coverage and licensing quote |
| Tardis | Recorded exchange messages, historical books/trades and replay | Useful for historical backtesting; not a substitute settlement source or a promise of hosted low-latency delivery |

### CF Benchmarks

Ask for licensed access to all three exact index IDs and confirm the maximum resolution returned by the API's `info` response. The optional `rti_stats` channel can expose consolidated index bid/ask, mid-price, utilized depth and amendment metadata when configured for that index. Do not assume these fields or 200ms delivery are included for every account. Do not use its basket/Nasdaq constituents channel as if it exposed constituent venue weights for BRTI.

Sources: [API licensing](https://docs.cfbenchmarks.com/api/), [entitlements and resolution](https://docs.cfbenchmarks.com/api/websocket/info/), [RTI statistics](https://docs.cfbenchmarks.com/api/websocket/rti_stats/).

### Coinbase and Kraken

Coinbase public market-data WebSockets do not require a JWT; heartbeat subscriptions help keep idle connections open. Its trade channel batches over 250ms, so that channel cannot support a claimed 10ms source-to-application freshness guarantee. Prefer order-book updates for book signals and keep trades for flow/volume features. [Coinbase WebSocket endpoints](https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/websocket/websocket-endpoints).

Kraken supports multi-symbol L2 subscriptions and a checksum for the top ten levels. A failed checksum invalidates book-derived features until recovery. [Kraken book specification](https://docs.kraken.com/exchange/api-reference/spot-websocket-v2/book).

Read-only product discovery from this Mac found Coinbase BTC-USD, ETH-USD and SOL-USD online, and Kraken XBT/USD, ETH/USD and SOL/USD online. This was not a WebSocket, regional cloud-access or latency benchmark. Kraken's v2 API uses BTC naming in subscriptions; store venue-specific aliases instead of guessing from REST symbols.

Public market-data access does not by itself establish permission to retain, train on or redistribute every feed in a commercial product. Confirm those rights, especially for paid/reference data.

### Paid providers

CoinAPI lists metered access and standard monthly plans starting at $79. Do not treat initial credits as a continuing free allowance or assume every stream has the same volume price. Request a quote for the exact three assets, venues, book depth and data tier. [CoinAPI pricing](https://www.coinapi.io/products/market-data-api/pricing).

Kaiko publishes institutional tier pricing on its market-data page; those displayed tiers are above this project's budget. Amberdata is part of the same vendor group following Kaiko's acquisition, rather than an independent redundancy vendor. [Kaiko data offering](https://www.kaiko.com/products/l1-l2-data), [acquisition announcement](https://www.kaiko.com/news/kaiko-acquires-amberdata-in-landmark-digital-asset-data-consolidation).

Coin Metrics documents streamed market quotes and broader datasets. Obtain instrument/channel coverage and commercial terms before assuming the community tier fits this public product. [Market quotes](https://docs.coinmetrics.io/market-data/market-data-overview/market-quotes).

Tardis preserves recorded feed messages and local arrival timestamps, useful for historical availability-aware replay. Verify dates, symbols and recorded gaps for the exact dataset. Do not infer an inexpensive unlimited plan from its public samples. [Tardis historical API](https://docs.tardis.dev/api/http-api-reference), [recording semantics](https://docs.tardis.dev/faq/data).

OKX's open-interest stream updates on a seconds-level cadence, so it is a slower context feature even when other channels are faster. Keep USD and USDT instruments distinct. Binance/Bybit and other derivatives venues can be later alternatives after access and dataset review; none is necessary for the first release. [OKX public channels](https://www.okx.com/docs-v5/en/#public-data-websocket-open-interest-channel).

## What “millisecond latency” must mean

Treat four quantities separately:

1. Provider publication/batching interval.
2. Network delivery from source to our region.
3. Receipt-to-feature computation and receipt-to-durable-write time.
4. Feature-to-browser delivery.

Timestamp precision is not timestamp accuracy or a latency SLA. Cross-machine event time minus receive time is an observed source-age estimate affected by clock error, not a guaranteed network latency measurement.

Proposed initial engineering targets, not measured promises:

| Stage | Initial objective |
|---|---|
| Receipt to validated in-memory features | p95 under 20ms at the measured peak event rate |
| Local durable capture | Benchmark separately; define fsync/ack policy and crash-loss tolerance |
| Provider-to-receipt | Measure by channel and region, reporting p50/p95/p99 and clock assumptions |
| Remote archival | Batches every 1–5 minutes, with a durable local journal until upload succeeds |
| Browser | Keep current 2s display for the initial data project; faster UI is a separate delivery change |

An always-running Rust collector can target millisecond local processing without making the browser update every millisecond. Benchmark candidate regions rather than declaring a region fastest in advance. Shared-core or fractional resources are not a guarantee of tight tail latency. If the requirement is guaranteed sub-10ms source-to-browser delivery, the present $40 architecture is not an adequate promise.

## Minimum valuable data

### Keep as the long-term research core, within measured storage limits

- Full received CF index publications at the licensed native cadence, including amendments and source timestamps.
- Contract identity, open/close times, rules version, target history, official result and result-availability time.
- One-second feature rows per asset, combining current reference/market and venue features with freshness flags.
- One-minute OHLCV/summary bars, pipeline-quality summaries and dataset/model versions.

### Keep for a short, explicit replay window initially

- Kalshi trades, order-book snapshots and deltas for current BTC/ETH/SOL contracts.
- Coinbase and then Kraken trades/book updates.
- Raw original messages and connection/gap records, proposed 7–30 days depending on measured bytes/day.

Retain a complete base snapshot plus all required deltas for any period advertised as book replay. Top-ten periodic snapshots alone are a sampled dataset, not a complete order-book history. Optional longer retention around opening/final-minute windows must preserve a valid starting snapshot and record what was sampled.

### Add later only if useful

Funding/open interest, liquidation observations, options-implied volatility, scheduled macro events, then selected news/on-chain data. Store each source's publication and arrival time. A liquidation feed is not automatically a complete census of all liquidations. News or aggregate-price APIs are not the first choice for reconstructing a fast exchange book.

## Storage architecture

**WebSockets → always-running Rust collector → durable local journal → normalized in-memory state/features → batched Cloud Storage archive.**

- Use RAM for the current book and rolling features. A database round trip is not on the calculation path.
- Use a durable local journal on persistent storage for restart recovery. Define acknowledgement/fsync policy explicitly: process-crash loss and whole-disk loss are different failure modes.
- Store bulk normalized events and feature rows as compressed Parquet in private Google Cloud Storage. Preserve compact raw chunks for the short replay tier. Partition by date/hour/source and compact small files; do not create one object per tick.
- Keep Firestore for small market summaries, dataset manifests, collection health and application-facing metadata. It remains a good application database, not the default tick warehouse.
- Use DuckDB for initial offline Parquet research. Consider partitioned BigQuery or a dedicated time-series store when actual query volume justifies it. Do not add Kafka, a managed database cluster or a paid stream processor for three assets by default.

Cloud Storage has storage, operation and network costs; file compaction and retention matter. [Storage pricing](https://cloud.google.com/storage/pricing).

### Why not every tick in Firestore?

Illustrative 30-day write-only calculation for one combined row per asset per interval, three assets, using the current us-central1 rate of $0.09 per 100,000 writes:

| Interval | Rows/day | Rows/30 days | Write cost only |
|---|---:|---:|---:|
| 1 second | 259,200 | 7,776,000 | $7.00 |
| 250ms | 1,036,800 | 31,104,000 | $27.99 |
| 100ms | 2,592,000 | 77,760,000 | $69.98 |

This excludes reads, storage, indexes, deletes, retries, networking and multiple venue streams. The project's named database does not get an additional free quota. Batched writes still charge per document write. [Firestore pricing](https://cloud.google.com/firestore/pricing).

For scale intuition only: a compact feature row averaging 200 bytes would mean roughly 52 MB/day for the one-second core, before actual encoding/compression overhead. Raw L2 can be orders of magnitude larger. Measure actual compressed bytes per feed over quiet and volatile periods before setting retention.

## Required event and dataset contracts

Each record needs venue, native instrument, canonical asset, quote currency, market type, channel, schema version, event ID/sequence when provided, connection/session ID, event timestamp, receive UTC timestamp, receive monotonic timestamp, original timestamp precision, normalized decimal values and quality flags. Track `available_at` for feature rows and results. Keep original amendments rather than rewriting the past.

Use decimal or scaled-integer price/size representations in the collector. Validate maker/taker side semantics per exchange before computing buy/sell pressure. Open-interest units, contract multipliers and quote currencies must be explicit; USD and USDT are not interchangeable labels.

Quality requirements:

- Sequence/checksum validation and snapshot recovery, with stale features disabled until resynchronization.
- Deduplication by source identity and revision, not by equal price. Equal successive CF values can still be distinct valid samples.
- Explicit gaps, reconnects, amendments and parser failures; bounded queues and observable overload behavior.
- Durable archive manifests/checksums and idempotent recovery from a local journal.
- Per-feed coverage, source age, queue lag, feature latency, archive lag, compressed bytes/day and projected cost.
- Isolated read-only market-data credentials and private archive access; no trade/RFQ capability.

## How this improves prediction research

Start with distance to target, time to close, final-minute observed-average coverage, short-horizon volatility, venue spreads/depth, signed trade flow, quote changes and cross-venue basis. Add BTC-to-ETH/SOL lead/lag features only when they are available by the prediction time.

Train against official contract results. Split chronologically by complete market windows/days, keeping correlated BTC/ETH/SOL windows together and purging overlapping feature/label horizons. Never randomly split adjacent tick rows. Do not use revised historical reference values or the archive's hindsight `best_entry` as if they were known at prediction time.

Compare a simple time/target/volatility baseline and contemporaneous Kalshi quotes with each added source using walk-forward log loss, Brier score and calibration. If simulating decisions, separately model fees, available size, delayed entry and uncertain fills. More sources are kept only if they improve held-out performance or resilience. No predictive edge or profit is established by this plan.

## First implementation sequence

1. Define latency, retention, outage/recovery and per-feed quality contracts; run a small read-only connectivity and volume benchmark.
2. Implement durable CF + Kalshi capture independent of visits, with replay and coverage reports.
3. Add Coinbase's three USD instruments, then Kraken, using the same normalized schema.
4. Persist one-second features and official labels; run the first reproducible baseline.
5. Evaluate one derivatives feed or a paid historical provider through an ablation, not a shopping list.

The current $40/month total budget remains a constraint. Adding a continuously running worker or CF license on top of current hosting may exceed it. Prepare a concrete full-system cost estimate or reallocate hosting before enabling billable resources. Public API availability, a free trial or a microsecond timestamp is not evidence that the required all-day licensed service fits that budget.

No new cloud resources, keys, paid accounts or data retention rules were enabled during this research.
