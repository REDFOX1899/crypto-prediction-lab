# Data Lab: price/volume pilot and before-outcome research

Scope: additive Data Lab features. Home, Single market, Combo lab, the live source workers and scheduled collector remain unchanged. Publication was requested on 10 November 2026. The owner explicitly confirmed permission for public Blockworks price/volume display; the application records that confirmation in the pilot manifest.

## Price and volume pilot

The manually invoked Rust `blockworks-pilot` role supports one or seven days, with at most three sequential provider requests and no retries. The seven-day pilot returned 2,016 valid five-minute candles for each of BTC, ETH and SOL: 6,048 total, no rejected points. Provider timestamp range is `[2026-11-03T05:10:00Z, 2026-11-10T05:10:00Z)`; downloaded at `2026-11-10T05:15:07Z`. Actual credit cost remains unverified. This does not activate recurring collection or automatic purchases.

The ignored local file is `data/blockworks-seven-day-20261110.json`. `scripts/publish-blockworks-pilot.py` validates the normalized schema and atomically creates three immutable asset documents and the latest manifest in the existing named Firestore database `projects/gitunderstand/databases/crypto-prediction-lab`, collection `blockworks_pilots`. It uses the operator's existing cloud identity; the API key and cloud access token are never uploaded or printed. No IAM change or new cloud resource is needed. The request is under 1 MB; each asset document is bounded separately. Conditional writes refuse to overwrite immutable captures or race a newer manifest.

Public reads require `BLOCKWORKS_PUBLIC_DISPLAY=approved` and a manifest containing the recorded permission confirmation. The dashboard retains its existing database-viewer identity and no Blockworks secret. A ten-minute shared cache bounds archive reads. `GET /api/blockworks` displays at most the final 24 hours (288 candles per asset), plus the seven-day pilot's counts and bounds. The entire seven-day capture is not returned by this display route. Local snapshots still require a loopback bind.

Source attribution links are visible in both Blockworks panels. Raw provider payloads and credentials are absent from responses. Observation exports continue to exclude Blockworks.

## Before this outcome

Select an outcome cell, an asset and a cutoff at open, +5m, +10m or +14m (one minute before close). `GET /api/research/before?asset=BTC&ticker=<archived-ticker>&minute=10` validates the supported asset/ticker association, rejects other cutoffs/unknown parameters, and uses only a recent completed 15-minute window in the archive. The route is read-only, has a twelve-second deadline and uses existing shared caches. POST is rejected. It performs no provider collection or database writes.

- Saved CMC samples must have collection and source timestamps at or before cutoff and be no older than eleven minutes. The closest valid sample is selected without forward filling. Collection timestamps are not claimed to establish an independently archived first receipt.
- Kalshi minute YES ask candles must end by cutoff; an unfinished minute is excluded. These are retrospectively downloaded quote ranges, not an executable historical order book or fills.
- Blockworks context uses the preceding hour ending at the most recent five-minute boundary at/before cutoff. Provider timestamp + five minutes must be at/before that boundary, conservatively excluding the potentially unfinished bucket. Exact bucket start/end convention remains unverified and is disclosed.
- Prior 5m/15m/60m moves require all corresponding consecutive candles. The typical five-minute move is the mean absolute open-to-close candle return across twelve complete candles. Relative fifteen-minute volume compares the last three complete candles with the average of the preceding three fifteen-minute blocks, requiring all twelve volumes and a positive baseline. Missing volume is not zero.
- Historical target text is shown with settled-archive provenance. No pre-cutoff target observation, CF reference/distance or historical depth/spread is invented. Those unavailable records are explicitly marked.
- Official result, expiration value and hindsight best-entry are excluded from the response/calculations. Changing the winner/settlement cannot change features. Results remain in the separate outcome inspector.

Retrospective history is labeled with its actual download time and unverified original availability. This is descriptive research, **not a point-in-time training dataset**, a prediction model or a profitability claim. A seven-day historical download does not supply seven days of contemporaneous CF/book observations.

New browser reads are aborted when selecting a different asset, cutoff or window and when leaving Data Lab. Late replies are checked against both request generation and exact asset/ticker/cutoff. The small local response cache has at most 32 selections.

## Verification and operation

Required checks: cargo fmt, clippy with warnings denied, locked workspace tests, frontend tests and Python diagnostic/publisher tests. Unit tests cover future-interval exclusion, missing data, winner-independent features, exact response selection, cancellation and publication scoping. Production verification must exercise actual stored data, all three assets, all four cutoffs, rejected invalid/write requests and the existing pages. Preserve the existing resource limits, identities and collector schedule.

The one-time publisher and data fetch are operator actions, never public routes. Refreshing Data Lab reads the saved pilot; it cannot spend Blockworks credits. Future data refreshes/backfills should follow an explicit usage review. Removing the display approval flag disables cloud-backed public Blockworks reads; retiring the pilot requires a separate retention decision consistent with the owner's agreement.
