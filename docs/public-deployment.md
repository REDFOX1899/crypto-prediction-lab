# Public Cloud Run pilot

Project: gitunderstand (308289525742)
Service: crypto-prediction-lab
Region: us-central1
Audience: public, no login
Scope: read-only BTC, ETH and SOL 15-minute market data

## Runtime

- Request-based CPU, 1 vCPU and 256 MiB memory
- Zero minimum instances and maximum one at service and revision level
- 80 concurrent requests; 300-second request timeout
- Browser EventSource reconnects after the timeout
- Long-running SSE requests keep compute billable; no continuous recording is provided
- Global in-process API token bucket: burst 120 requests, refill two per second; returns 429 with Retry-After
- GET /healthz is a process health check, not proof of upstream freshness
- PORT selects the server port, DASHBOARD_BIND_ADDRESS selects its interface; local default stays loopback
- SIGTERM and Ctrl-C trigger graceful server shutdown

## Secret handling

A dedicated runtime service account reads only the two crypto-prediction-lab secret entries. The private key is mounted at /secrets/kalshi/private-key.pem, and the ID is injected through a secret-backed environment variable. Neither is in the source archive, image, JavaScript or repository. This app contains no order-submission route.

The source and Docker upload ignore files allow only Cargo files, apps, crates and the Dockerfile. Keep any downloaded key outside the project. Never put a private key in a tracked text file.

## Budget estimate

Target: $10/month incremental spend for this app. This is not a hard cap.

At the published us-central1 request-based list rates, 1 vCPU and 0.25 GiB cost about $0.089 per actively serving hour before free allowances, requests, network, logging, image storage and build costs. Roughly 80 stream-active hours would be about $7.10 compute. Continuous 30-day serving would be around $64 compute. Actual use varies; multiple users can share an instance, and empty time can scale to zero. Do not assume shared free allowances remain available.

The one-instance setting limits normal scaling, not total active hours or an absolute bill. Do not apply a project-wide spend cap or billing shutdown because other applications share gitunderstand. More than a small pilot requires a new budget decision.

Sources:
- https://cloud.google.com/run/pricing
- https://docs.cloud.google.com/run/docs/configuring/billing-settings

## Operations

Build with gcloud builds submit --project=gitunderstand --tag=us-central1-docker.pkg.dev/gitunderstand/crypto-prediction-lab/dashboard:launch-20261106 from the repository. Review the upload allowlist before building. Use scripts/deploy-cloud.zsh for the initial deployment. Pin a new version tag for future releases rather than silently replacing a running release tag.

To stop public access, remove allUsers from roles/run.invoker on this service only. To retire the pilot, delete only the crypto-prediction-lab Cloud Run service; separately review its dedicated secrets and image repository. Never disable project billing or change unrelated resources.

A custom domain, durable recorder, replay, saved research, user accounts and payments are not part of this launch. Market data must be used within the applicable Kalshi and benchmark-provider terms.

## Launch verification on 6 November 2026

Public URL: https://crypto-prediction-lab-308289525742.us-central1.run.app
Revision: crypto-prediction-lab-00001-jcs
Image digest: sha256:8462d3d48c8b65510dc1d747fbb10201de36c06525780dbffa5110c7075d2040
Cloud Build: 682aaf70-134d-4964-99c7-099d47a6f235

Unauthenticated access, all three snapshots, and live ticker/book/reference streams passed. Browser asset switching passed. Unsupported asset input returned 400. Local checks passed: 18 Rust tests, four JavaScript tests, formatting and clippy. The public /healthz path returned a frontend 404 despite working locally; use /api/assets for a basic external availability check for this revision. Investigate the public health path before adding uptime monitoring that depends on it. The header still says local; this wording needs correction in the next visual update.

No unrelated service settings or project-wide billing controls were changed. No domain, database, payment system or durable recorder was created.

## Superseding runtime plan — 8 November 2026

The combo-book release changes production to 0.25 vCPU, concurrency one, first-generation request billing and short shared polling. Previous 1-vCPU/SSE cost estimates above describe the earlier release. See cost-controls.md for the new $40 target, detailed limitations, label-filtered alert and resource settings. The current health endpoint is /api/health. Durable history and combo research are now implemented; older launch scope statements above are historical.

## Data Lab release — 9 November 2026

Application source `63c67ac` is published to GitHub main. Cloud Build `9be3254e-d6ae-4a13-8360-bd7433b64466` produced image tag `data-lab-20261109-r2` and digest `sha256:8b2d525eea3b187853d4b2d51663d94f50e121ec8194f180151ee7b672f387bd`. Public revision `crypto-prediction-lab-00016-mer` serves 100% of traffic. The Data Lab opens at `/?view=data`.

The revision was tested through a temporary tag before switching public traffic. Both the candidate and public URL passed real archive/history, source-identity and read-only route checks. All three assets returned 15 stored minute candles for the sampled completed window. The CMC history query returned 17 actual saved samples; it supports a bounded 24-hour horizon but does not manufacture older history.

Runtime configuration and access were preserved, including existing secrets, service identity, 0.25 CPU / 256 MiB, concurrency one, max one, scale to zero and request billing. The private collector remains on revision `crypto-history-collector-00005-msp`. No new provider subscriptions, collection schedules or permissions were enabled. Temporary revision tag was removed. See SESSION_HANDOFF.md for exact checks and the remaining Blockworks access limitation.

The final release includes response compression after public browser testing exposed the uncompressed outbound-budget bottleneck. Gzip-capable clients receive smaller bodies, and the same outbound budget accounts for encoded bytes. Public HTML measured 36,736 bytes on the wire versus 107,948 decoded. The live Data Lab and archived quote charts were confirmed in the browser after the correction. The owner allowed a GCP capacity increase if needed; none was required.
