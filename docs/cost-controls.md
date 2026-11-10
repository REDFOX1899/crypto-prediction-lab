# Crypto pilot cost controls — 8 November 2026

Owner's monthly maximum requested: USD 40 for this crypto application, not the other applications sharing gitunderstand. Google Cloud does not provide an exact per-service dollar cap. These controls reduce normal costs and bound resource use; they do not guarantee an invoice ceiling. Budget alerts are delayed notifications and do not shut down services. Do not describe the limit as enforced or disable project billing.

## Runtime plan

Public dashboard: request-based first-generation Cloud Run, 0.25 vCPU, 256 MiB, concurrency one, maximum one instance, minimum zero, startup CPU boost off, timeout 30 seconds. Short browser polling replaces long-lived SSE in production; Kalshi market and reference connections remain upstream. Long-lived public SSE endpoints return 410 in this mode. One tab shares one all-asset feed request every two seconds; hidden tabs slow to 15 seconds. Snapshot and history requests remain bounded/cached. Under load requests may queue or return 429: pilot capacity is intentionally limited.

At us-central1 list prices ($0.000024/vCPU-second and $0.0000025/GiB-second), one dashboard instance active for every hour of a 31-day month is approximately $17.74 CPU+memory, before credits and ancillary charges. Startup/transient platform behavior can vary. The collector uses the same 0.25 vCPU/256 MiB maximum-one configuration, with a 60-second HTTP timeout, scheduled every quarter hour with one retry. Its normal few-second settlement runs cost far less than continuous collection. A 60-second platform request timeout is not a guarantee that application work was cancelled immediately.

Production application request budget: shared burst 120, refill two/second, applying to all paths. Shared outbound response budget: 1 MiB burst and 8 KiB/second refill, including static files and JSON; up to 1 MiB per response. Large or busy responses return 429 with Retry-After, and static assets cache for one hour. These are per-instance memory counters, reset on restarts; they limit normal traffic rather than providing abuse-proof billing protection. Requests denied by the application can still be billed by Google. There is no paid CDN, load balancer, Cloud SQL, new always-running worker or paid external data product.

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
