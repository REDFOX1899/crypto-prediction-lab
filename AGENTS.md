# Crypto Prediction Lab

## Start a new chat

Read `docs/PROJECT_MEMORY.md`, `docs/SESSION_HANDOFF.md` and `README.md` before proposing or implementing changes. Check `git status` and recent commits. Open the topic-specific documents linked from the memory file only when needed. Treat dated plans as proposals unless the current memory or implementation confirms completion.

Work from the repository attached to the current project. Do not silently switch to another local checkout or an unrelated application in the shared Google Cloud project. If this folder lacks `Cargo.toml` and `.git`, report the mismatch before creating a replacement application.

## Product and architecture

- Read-only research for BTC, ETH and SOL **15-minute Kalshi markets**.
- Rust workspace, Axum/Tokio backend, plain JavaScript/CSS/SVG frontend. Extend the existing application; do not scaffold a Next.js/React replacement for ordinary features.
- Preserve the shared asset registry and workspaces: Home, Single market and Combo lab.
- Preserve light/dark themes, accessible numeric labels, source timestamps, visible gaps and the always-visible detailed order book.
- Keep official Kalshi outcomes, CF reference observations, contract prices and CoinMarketCap aggregates distinct. An observed average is not an official settlement result. Rolling 24-hour volume is not 15-minute traded volume.
- Never invent unavailable quotes, missing history, historical fills, profitability or predictive accuracy. Private combo RFQs, order execution, transfers and payments remain disabled.

## Security and operations

- Never print or commit API keys, tokens, private keys, user captures or service-account credentials. Keep secrets outside the repository and server-side in Secret Manager. Public visitors cannot invoke collection writes.
- Use only this application's dedicated services and named Firestore database in `gitunderstand`. Preserve unrelated resources, including `thesis.fde.guru`.
- Preserve current cost controls: the $40 monthly budget alert is a target, not a guaranteed spending cap. Do not enable paid subscriptions, increase resource limits or introduce an always-on collector without an explicit budget decision.
- Coinbase is on hold. Direct CF access is pending licensing; existing CF values arrive through Kalshi. Do not assume new source permissions or subscriptions exist.
- Implement locally unless the task requests publishing/deployment. When publication is requested, retain existing IAM/resource limits and verify the actual live result. Do not redeploy for documentation-only changes.

## Verification and handoff

Run checks appropriate to the change. For Rust/backend changes: `cargo fmt --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, and `cargo test --locked --workspace`. For frontend behavior: `node --test apps/dashboard/tests/*.test.cjs`; inspect affected desktop/mobile and light/dark states when the layout changes. Test authorization for new API routes and cancel stale requests on workspace changes.

After meaningful work, update `docs/SESSION_HANDOFF.md` with completed work, exact checks, deployment state, unresolved issues and the next concrete step. Update `docs/PROJECT_MEMORY.md` only for lasting product/architecture decisions. Do not paste chat transcripts, credentials or speculative features into memory. Respect existing user changes and use `codex/` for new feature branches.
