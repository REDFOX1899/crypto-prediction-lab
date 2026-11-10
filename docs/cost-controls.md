# Crypto pilot cost controls — 8 November 2026

Owner's monthly maximum requested: USD 40 for this crypto application, not the other applications sharing gitunderstand. Google Cloud does not provide an exact per-service dollar cap. These controls reduce normal costs and bound resource use; they do not guarantee an invoice ceiling. Budget alerts are delayed notifications and do not shut down services. Do not describe the limit as enforced or disable project billing.

## Runtime plan

Public dashboard: request-based first-generation Cloud Run, 0.25 vCPU, 256 MiB, concurrency one, maximum one instance, minimum zero, startup CPU boost off, timeout 30 seconds. Short browser polling replaces long-lived SSE in production; Kalshi market and reference connections remain upstream. Long-lived public SSE endpoints return 410 in this mode. One tab shares one all-asset feed request every two seconds; hidden tabs slow to 15 seconds. Snapshot and history requests remain bounded/cached. Under load requests may queue or return 429: pilot capacity is intentionally limited.

At us-central1 list prices ($0.000024/vCPU-second and $0.0000025/GiB-second), one dashboard instance active for every hour of a 31-day month is approximately $17.74 CPU+memory, before credits and ancillary charges. Startup/transient platform behavior can vary. The collector uses the same 0.25 vCPU/256 MiB maximum-one configuration, with a 60-second HTTP timeout, scheduled every quarter hour with one retry. Its normal few-second settlement runs cost far less than continuous collection. A 60-second platform request timeout is not a guarantee that application work was cancelled immediately.

After the November 10 availability incident, production maintains two separate per-instance allowances. Page loads, registered static assets, the asset catalog, the homepage study and health checks share a burst of 240 requests and 2 MiB, refilling at 8 requests/s and 8 KiB/s. Market-data requests have a burst of 120 requests and 1 MiB, refilling at 8 requests/s and 16 KiB/s. Saturating data polling cannot consume the page allowance. Encoded response bodies are charged after compression. Each decoded response is limited to 1 MiB. Local counters reset on restart. HTTP 429 responses specify a retry delay and disable caching. Most static assets cache for one hour.

The previous shared allowance of 2 requests/s and 8 KiB/s rejected essential scripts during normal multi-tab traffic. Home now requests only twelve small official-result records per asset from the existing archive cache. The full board remains available in the research workspaces. Visitors requesting the same asset within five seconds share one public market and order-book snapshot. The response retains the original fetch time and expires at contract rollover. A failed market discovery has a three-second retry cooldown per asset. Combined sustained outbound allowance is now 24 KiB/s before headers, compared with the earlier 8 KiB/s. Actual transfer depends on demand. This change does not increase CPU, memory, instances, scheduled collection or the monthly billing alert.

Plan roughly $25–$35 total for sustained pilot activity, allowing database operations/storage, collector runs, outbound traffic, secrets, images, scheduler and occasional builds. This is an estimate, not measured current spend. Free allowances are shared and are not assumed. Future builds, storage growth, traffic abuse, platform overlap and unlabelled charges can exceed the estimate. Review actual billing before expanding capacity.

## Alert

Created a separate USD 40 monthly budget filtered to project gitunderstand and label app=crypto-prediction-lab. Existing budgets are unchanged. Alerts: actual spend 50%, 75%, 90%, 100%, and forecast 90%. Dedicated Cloud Run services and the image repository carry the app label. This alert covers label-attributed charges only; unlabelled Firestore, Secret Manager, build and other charges may not be included. Review those separately in billing. The alert is not a complete app cost ledger or spending cutoff.

If an absolute no-exceptions $40 bill is required, the current public Google Cloud pay-as-you-go setup does not meet that requirement. A prepaid/capped hosting product or a separately reviewed automatic service-only shutdown with a substantial safety margin is needed; even shutdown cannot reverse delayed charges. Never turn off billing for shared gitunderstand.

Sources:
- https://cloud.google.com/run/pricing
- https://docs.cloud.google.com/run/docs/configuring/services/cpu
- https://docs.cloud.google.com/billing/docs/how-to/budgets

Verified deployed dashboard revision crypto-prediction-lab-00005-dnn and collector revision crypto-history-collector-00003-lj5 both have CPU 0.25 and memory 256Mi. Collector test invocation returned HTTP 200 in approximately 2.91 seconds. The USD 40 label-filtered budget was created as 059bac23-0a03-4b41-b55a-f426df66a309. No service-only automatic spending shutdown is configured; the absolute dollar ceiling remains unguaranteed.

Final-minute view (9 November): bounded in-memory source-second observations piggyback on the existing shared reference feed. No additional upstream subscriptions, polling requests, databases or cloud resources. Reply size increases slightly within the same outbound limiter. Existing cost caveats and $40 alert target still apply.

Live homepage delivery (9 November): all three cards share one /api/feeds request per visible tab every two seconds instead of issuing per-card live requests. Existing provider connections are reused. Target/window snapshots slow to thirty seconds; history stays on a slower cadence. Background tabs slow live polling and history. Requests are aborted on leaving a workspace, use an eight-second timeout, and honor rate-limit cooldown with bounded failure backoff. Total homepage HTTP requests increase versus its old twenty-second snapshot mode, but no cloud resource limits, provider permissions or paid services change. The existing $40 alert is still not an invoice cap.

Focus UI update: reference listeners use the same two-second all-asset response as the market cards; no new upstream connection or request loop. The raster hero is no longer requested on Home. New CSS, theme code and bounded SVG geometry replace it without a graphics dependency or paid generation service. Current service, database and secret permissions remain unchanged. The $40 alert remains a target rather than an enforced cap.


Data Lab delivery correction (9 November): the existing 1 MiB burst and 8 KiB/second outbound budget now counts encoded response-body bytes when the client accepts gzip. Public HTML, scripts, CSS and JSON are compressed with the fast setting before budget accounting; clients declining gzip retain identity responses. Cache variants declare Accept-Encoding. The decoded response ceiling remains 1 MiB, and instance/CPU/memory/request limits are unchanged. This reduces actual transfer and prevents routine live polling from unnecessarily crowding out uncompressed page assets. It remains a shared allowance, not a guarantee against 429s under load.


Data Lab price/volume pilot (10 November): one seven-day batched fetch, three Blockworks requests total for 6,048 candles; actual credit charges remain unverified. No recurring collection or paid upgrade. Three immutable asset records plus one manifest use the existing named Firestore database, with ten-minute shared reads and a one-day public chart projection. The cutoff inspector reuses bounded archive/history caches and returns at most one hour of Blockworks intervals plus completed in-window minute quotes. No visitor-triggered provider fetch or writes are exposed. Existing Cloud Run, request and outbound controls are unchanged; build/database operations remain billable as applicable.


Minute-10 checkpoints and replay (10 November). The owner authorized a private scheduled capture at UTC 10/25/40/55 using the existing collector and scheduler identities. It plans 288 small immutable records per day, checks existing records before retrying and makes at most six public provider requests per run. The new job has no retries. The handler has a 50-second internal deadline. Replay reads reuse the existing archive cache and make no provider collection calls. Runtime settings and the $40 billing alert are retained. A temporary service-wide two-instance allowance is used during public revision overlap and restored to one after promotion.
