# CoinMarketCap market context

Coinbase remains on hold. CoinMarketCap Basic was verified with the owner's key on 9 November 2026 (US Eastern): 15,000 monthly credits and 50 requests/minute. The BTC (1), ETH (1027) and SOL (5426) V3 USD quotes share one request costing one credit. Global metrics and sentiment calls each cost one credit; key usage checks cost zero credits.

The IAM-protected existing collector exposes POST /collect-context. A dedicated OIDC scheduler invokes it every five minutes. Public GET /api/context reads only normalized Firestore data, caches reads for two minutes and exposes no provider credentials. The public dashboard identity retains database viewer access. Only the existing private collector can read the new dedicated crypto-prediction-lab-cmc-key secret. There is no new always-on worker, database, paid API subscription or write/trading permission.

Collection: three quotes together every five minutes, global metrics hourly and sentiment daily. Approximate 30-day usage: 8,640 + 720 + 30 = 9,390 credits, plus manual tests/retries. Credit usage is checked before each due run; collection pauses at the lower of 12,000 credits or 90% of the account allowance. No automated upgrade or overage purchase is implemented. Successful steps are persisted before the next step to reduce repeat requests after retries. An upstream call followed by a failed database write can still consume another credit on retry. Existing platform timeouts and max-one-instance settings apply.

Firestore cmc_context/latest stores the latest normalized prices, broader returns, volume and global/sentiment data. cmc_samples/s<five-minute-slot> stores combined three-asset observations using stable slot IDs; repeated writes cannot create duplicate slot documents. No full raw response, secret, key usage details or private account data is exposed or archived. Source quote timestamps and collection timestamps remain separate. Previously received global/sentiment values retain their own timestamps. The response returns at most 24 recorded samples, leaving gaps unfilled. Data grows from collector startup, not retroactive tick reconstruction. There is no deletion/TTL in this release; review storage growth and retention terms before increasing scope or promising a permanent raw data archive. CMC documents legitimate application caching.

The homepage adds a separate Market context section with market cap, rolling 24-hour volume, BTC dominance, dated sentiment, three-asset 1h/24h/7d return comparisons, and sampled-price SVG charts. Return bars cap at +/-10%, show numeric values, and state their scale. Missing values stay unavailable; gaps greater than 7.5 minutes break plotted paths. Charts use their own vertical scales and are not relative-performance overlays. Source timestamps drive stale states. Sentiment is sampled daily even though the upstream endpoint may change more often.

CMC aggregated prices do not replace CF reference prices, contract targets, final-minute observations, Kalshi order books or official settlement outcomes. Rolling 24-hour volume is not fifteen-minute traded volume. No AI model, trading, signal accuracy claim, or automatic contract prediction is added. The stored observations can support future window-context joins, but this release does not attach CMC context retrospectively to settlement documents.

The browser requests cached context once a minute while Home is visible, slows to five minutes in background, and cancels when leaving Home. Visitor requests never invoke CMC. Quota pauses and outages leave older data visible with source-age labels. Current CMC pricing advertises Basic commercial use; attribution links to CoinMarketCap. It does not authorize standalone resale or unrestricted redistribution.

Budget: API remains free. This feature adds modest Firestore reads/writes/storage, one scheduled trigger, a dedicated secret and brief collector requests within existing CPU/memory limits. The existing $40 alert is unchanged and is not a guaranteed bill ceiling. Other gitunderstand resources and thesis.fde.guru are untouched.

References:
- https://coinmarketcap.com/api/pricing/
- https://coinmarketcap.com/api/documentation/pro-api-reference/cryptocurrency
- https://coinmarketcap.com/api/documentation/pro-api-reference/global-metrics
- https://coinmarketcap.com/api/documentation/pro-api-reference/tools
- https://coinmarketcap.com/api/documentation/guides/best-practices

## Release verification — 9 November 2026 (US Eastern)

Public revision: crypto-prediction-lab-00012-jx6. Private collector: crypto-history-collector-00005-msp. Image: cmc-context-20261109-r3. Successful Cloud Build: 3d8f5edc-fd30-467a-9adc-3c39813d9040. Source checks: 37 Rust tests, 38 JavaScript tests, formatting and clippy passed.

Live checks confirmed all three CMC assets, global metrics and sentiment in Firestore and on the homepage. A repeated collector invocation returned cached without another collection. The new OIDC scheduler received HTTP 200. Unauthenticated access to the private collector returned 403; the public service has no /collect-context route (404) and rejects POST /api/context (405). Existing BTC/ETH/SOL market and reference feeds all reported live. Mobile and both theme layouts were reviewed; no horizontal overflow was observed. Stored CMC samples began at release and subsequent scheduled collection added observations without inventing earlier history.

CMC endpoints return success error_code as either numeric 0 or string "0". The parser accepts both and rejects absent/nonzero codes; a regression test covers the provider discrepancy. Quote snapshots skip sample writes if the normalized observations are unchanged. Successful collections still persist their check time.
