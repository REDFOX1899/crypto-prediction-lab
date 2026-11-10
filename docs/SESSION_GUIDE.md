# Starting fresh chats without losing the project

The Codex project should point to the folder containing this repository's `Cargo.toml`, `README.md` and `.git`. A project name or grouped chat list does not substitute for having the actual code in the attached folder.

Use `AGENTS.md` for short, persistent working rules; `PROJECT_MEMORY.md` for lasting product/architecture facts; `SESSION_HANDOFF.md` for the latest completed work and next step; topic documents for detail; Git for exact changes. Keep these files concise and current rather than copying the entire conversation into each new chat.

## A useful working rhythm

1. Start one new chat per feature or bug, with an outcome-based title, such as "Show CMC context for completed windows" or "Explain missing reference coverage".
2. Say what should change, who it helps, how you will judge success, and whether to keep it local or deploy. Include screenshots when they clarify the issue.
3. Continue the same chat for fixes to that feature. Start another when the objective changes substantially or the old discussion becomes hard to follow.
4. End a feature by asking for the handoff to be updated and the code committed/pushed. Deployment should have its own clear outcome and verification.
5. Avoid editing the same files from several chats in one checkout at once. Separate branches/worktrees can isolate simultaneous work when needed; start with one active feature at a time.

The project notes carry the compact context. A new chat should inspect them and the current code rather than assuming it remembers every previous conversation.

## Paste this to begin a feature

> Read AGENTS.md, docs/PROJECT_MEMORY.md, docs/SESSION_HANDOFF.md and README.md. Confirm the current application and latest Git state before making changes. We are extending the existing read-only BTC, ETH and SOL 15-minute dashboard. My next feature is: [describe one outcome]. Success means: [observable result]. Work locally first; deploy only if I request it. Explain the changes briefly as you go, and update the handoff when finished.

## Paste this to finish a feature

> Update docs/SESSION_HANDOFF.md with what changed, the checks actually run, any live deployment revision, unresolved issues and the next step. Update PROJECT_MEMORY.md only for lasting decisions. Scan for secrets, commit and push the intended changes, then report the commit and whether production changed.

## Local development

From this repository root:

```sh
source "$HOME/.cargo/env"
cargo run -p dashboard
```

Open http://127.0.0.1:3000. Use `DASHBOARD_PORT=3001 cargo run -p dashboard` if another dashboard is already running. Do not stop an unrelated server. A fresh checkout does not contain credentials: public Kalshi snapshots work without a key, while authenticated reference streams need server-side local configuration. Production secrets remain in Secret Manager and should not be copied into the repository just to start a new chat.
