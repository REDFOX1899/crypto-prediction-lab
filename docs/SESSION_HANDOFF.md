# Session handoff

Updated: 9 November 2026, America/New_York.

## Completed

- Current application code, including the live CoinMarketCap context dashboard, is on GitHub at `66e3bf7`.
- CMC is deployed and collecting: public revision `crypto-prediction-lab-00012-jx6`, private collector `crypto-history-collector-00005-msp`. Deployment details are in `coinmarketcap-context.md`.
- This session populated the previously empty project folder from GitHub and added persistent orientation, session guidance and an updated roadmap. This is documentation/setup work; no production deployment is needed for it.
- Use the current project checkout for subsequent work; the older Desktop checkout was not deleted or modified by this setup.

## Verified evidence

Application release: 37 Rust tests, 38 JavaScript tests, formatting and clippy passed. GitHub CI for `66e3bf7` completed successfully: https://github.com/REDFOX1899/crypto-prediction-lab/actions/runs/42747073098.

Live release checks confirmed all three CMC assets, global metrics and sentiment saved to Firestore and rendered on the homepage. Repeat collection reused the saved snapshot. Authenticated scheduler calls returned HTTP 200; unauthenticated collector access returned 403; the public service has no collection route and rejects POST /api/context. Existing Kalshi/CF feeds reported live. Mobile and light/dark layouts were inspected. These are dated checks, not a guarantee of current service health.

The current setup change only modifies documentation and repository instructions. Validate Markdown links, whitespace and the pushed Git commit; do not redeploy or rerun unrelated production mutations for it.

## Pending work and limitations

- No new product feature is selected for the next chat. Ask for the desired outcome if none is supplied; do not silently implement the whole roadmap.
- Coinbase stays on hold. CF trial/licensing response and direct access are unconfirmed.
- CMC/settlement context joins, full market replay, always-on tick collection and Nautilus integration are future work.
- CMC historical charts grow from actual recorded samples, with visible gaps. They are not complete tick history.
- The $40 alert is not a spending cutoff. Do not claim all cloud costs are guaranteed below it.

## First steps for the next chat

1. Read AGENTS.md, PROJECT_MEMORY.md and README.md; confirm this is the Rust repository and inspect git status.
2. State the current implemented behavior and the requested feature in a few sentences.
3. Read the relevant module and topic document, then implement only that feature with appropriate checks.
4. If publishing is requested, verify the deployed behavior and record the new revision. Otherwise leave production unchanged.
5. Update this handoff with actual outcomes, checks, unresolved issues and the next concrete step; commit/push when requested.
