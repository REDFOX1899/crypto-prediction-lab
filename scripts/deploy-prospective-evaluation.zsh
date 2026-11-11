#!/bin/zsh
# Operator reports remain on the existing IAM-protected collector only.
set -euo pipefail
evaluation_image=${1:?Pass immutable application image digest}
evaluation_object=${2:?Pass registered experiment object}
[[ "$evaluation_image" == us-central1-docker.pkg.dev/gitunderstand/crypto-prediction-lab/dashboard@sha256:* ]] || { print -u2 'Expected app image digest'; exit 1; }
[[ "$evaluation_object" =~ '^prospective/experiments/v1/[a-f0-9]{64}\.json$' ]] || { print -u2 'Expected hashed experiment object'; exit 1; }
gcloud run deploy crypto-history-collector --project=gitunderstand --region=us-central1 --image="$evaluation_image" --update-env-vars="EVALUATION_EXPERIMENT_OBJECT=$evaluation_object" --quiet
