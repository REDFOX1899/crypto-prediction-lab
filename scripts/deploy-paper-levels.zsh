#!/bin/zsh
# Image-only release: retain private identity, IAM, environment and resource limits.
set -euo pipefail
paper_image=${1:?Pass the built immutable app image digest}
[[ "$paper_image" == us-central1-docker.pkg.dev/gitunderstand/crypto-prediction-lab/dashboard@sha256:* ]] || { print -u2 'Expected app image digest'; exit 1; }
gcloud run deploy crypto-research-mcp --project=gitunderstand --region=us-central1 --image="$paper_image" --quiet
