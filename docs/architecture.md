# Architecture

The CLI handles presentation; `kalshi-client` owns the read-only HTTP boundary and typed response models. The asynchronous client reuses one HTTP connection pool, times out requests, checks HTTP status, and follows cursors until exhausted. It rejects repeated cursors to avoid infinite loops. A snapshot can change between pages, so results do not represent an atomic exchange snapshot.

The selector takes the clock as an argument, making its boundary behavior deterministic in tests. It chooses an active market whose open time has passed and close time has not, then the nearest close (ticker breaks ties). This is suitable for the initial 15-minute series; it is not a general market selection policy for all contracts.

Dollar values use `Decimal`, never binary floating point. YES and NO bids and asks are shown independently, rather than treating a bid as a purchase price. Missing fields stay missing. Quotes are snapshots, not executable fills. Exchange-wide status describes the default exchange index, which may differ from a particular market's index.

No credentials, orders, trading, persistence, live streaming, or public redistribution are implemented. Check market-specific contract terms before interpreting settlement. Review Kalshi and index-provider market-data rights before redistribution.
