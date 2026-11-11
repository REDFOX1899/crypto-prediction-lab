#!/bin/zsh
# Image-only update. MCP first accepts old and new scopes; collector follows.
set -euo pipefail
rules_image=${1:?Pass the immutable app image digest}
[[ "$rules_image" == us-central1-docker.pkg.dev/gitunderstand/crypto-prediction-lab/dashboard@sha256:* ]] || { print -u2 'Expected app image digest'; exit 1; }
for rules_service in crypto-research-mcp crypto-history-collector; do
  gcloud run deploy "$rules_service" --project=gitunderstand --region=us-central1 --image="$rules_image" --quiet
done
