# Private production pilot

Initial audience: five invited users following BTC, ETH and SOL 15-minute markets.
Operating scope: read-only analysis. No order submission or enabled payments.

## Inputs needed

- Dedicated GCP project ID and billing-enabled status
- Monthly spend target and approved ceiling, including always-on collection, storage and domain routing
- Region preference and expected user locations
- Domain preference and DNS provider; lab.shantanu-patil.com is the recommended initial candidate
- Authentication preference; start with Google sign-in and an allowlist
- Confirmed permission for the intended Kalshi and CF data storage, display and eventual commercial use
- Initial retention period; propose seven days of event capture for the pilot, subject to rights and measured volume

Do not share private keys, passwords or service-account JSON in chat. Use local Google authentication and scoped cloud service identities.

## Delivery sequence

1. Validate source freshness, reconnect behavior, settlement explanation and asset rollover locally.
2. Add a durable recorder before UI state coalescing, versioned manifests, gap markers and deterministic replay of one completed window. Local SQLite metadata and event batches first.
3. Prepare a container, configurable PORT and bind address, health probes, shutdown handling, rate limits and authenticated private access. Preserve loopback defaults for local development.
4. Deploy a private staging environment after budget approval. Use Cloud Run for serving and Secret Manager for the read-only key. Decide continuous collector placement and CPU allocation explicitly; web autoscaling must not multiply independent collectors. Use Cloud Storage for durable batches and a recoverable collector lease.
5. Exercise reconnects, restarts, secret rotation, restore and access rejection before inviting users. Use a stable production custom-domain routing option, not Cloud Run preview domain mapping.
6. Invite five users manually. Observe their first session and ask whether replay answered a real question. Measure return visits and repeat replay use with disclosed minimal analytics. Validate willingness to pay before enabling subscription billing.

## Product hypothesis

A researcher will return for reliable completed-market replay, liquidity context and clear settlement evidence. Visual polish supports this; it does not validate willingness to pay.

Suggested pilot evidence: five completed onboarding sessions, three users returning during the following week, and two requesting continued access. These are decision targets, not forecasts.

## Current status

Added browser source-age indicators, market feed state and settlement explanation. These UI improvements are not a durable recorder or proof of production readiness. GCP project, budget, DNS, authentication and data-use permission remain pending. No cloud resources have been created by this plan.

References checked 6 November 2026:
- https://docs.cloud.google.com/run/docs/configuring/billing-settings
- https://docs.cloud.google.com/run/docs/mapping-custom-domains
- https://docs.cloud.google.com/run/docs/authenticating/overview
- https://help.kalshi.com/en/articles/13823838-crypto-markets

## Confirmed pilot constraints

User selected project gitunderstand (308289525742) and a $10 monthly budget. Read-only checks confirm billing is enabled. Existing Cloud Run services share this project; do not change their scaling, IAM, billing controls or availability.

The $10 scope needs clarification: crypto pilot incremental spend versus total project spend. Prefer a separate project for an isolated Cloud Run spend cap. A Cloud Run spend cap in gitunderstand would apply to the project/service billing scope, not just the new application, and could pause unrelated Cloud Run services. Enforcement can overshoot due to reporting latency and does not cover all other service costs.

At $10, begin with scheduled private evaluation sessions and scale-to-zero serving, no Cloud SQL, no load balancer and no purchased domain. Live SSE requests keep serving compute active; minimum instances zero alone does not bound monthly cost. Continuous recording remains local or explicitly scheduled until a costed collector design fits the approved budget. Shared billing-account free tiers cannot be assumed available. Avoid enabling billing-based shutdown automation in the shared project.

## Project decision

The user explicitly confirmed using the existing gitunderstand project. Do not create or request another project. Plan isolated resources with a crypto-prediction-lab prefix and dedicated service identity, storage and least-privilege permissions. Preserve all unrelated services and domains. Do not apply Cloud Run project-wide spend caps or disable project billing to control the crypto application.

Treat $10/month as the planning target for this application pending a concrete deployment cost review; it is not a guaranteed hard cap or authorization to exceed the budget. Prepare the container, health checks, request limits and authenticated pilot locally before deploying paid resources. Use scheduled sessions and bounded storage initially.
