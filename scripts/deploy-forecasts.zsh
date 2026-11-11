#!/bin/zsh
# Preserve all IAM/resource settings; update MCP first for backwards-compatible scopes.
set -euo pipefail
forecast_image=${1:?Pass immutable app image digest}
forecast_object=${2:?Pass immutable forecast model object name}
[[ "$forecast_image" == us-central1-docker.pkg.dev/gitunderstand/crypto-prediction-lab/dashboard@sha256:* ]] || { print -u2 'Expected app image digest'; exit 1; }
[[ "$forecast_object" =~ '^forecast_models/v1/[a-f0-9]{64}\.json$' ]] || { print -u2 'Expected hashed private model object'; exit 1; }
gcloud run deploy crypto-research-mcp --project=gitunderstand --region=us-central1 --image="$forecast_image" --quiet
gcloud run deploy crypto-history-collector --project=gitunderstand --region=us-central1 --image="$forecast_image" --update-env-vars="FORECAST_MODEL_OBJECT=$forecast_object" --quiet
