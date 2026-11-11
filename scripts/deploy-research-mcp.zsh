#!/bin/zsh
# Private, first-party MCP only. No scheduler, provider subscription or token delegation.
set -euo pipefail
research_image=${1:?Pass the built immutable image digest}
[[ "$research_image" == us-central1-docker.pkg.dev/gitunderstand/crypto-prediction-lab/dashboard@sha256:* ]] || { print -u2 'Expected app image digest'; exit 1; }
research_identity=crypto-research-mcp@gitunderstand.iam.gserviceaccount.com
if ! gcloud iam service-accounts describe "$research_identity" --project=gitunderstand >/dev/null 2>&1; then
  gcloud iam service-accounts create crypto-research-mcp --project=gitunderstand --display-name='Crypto cutoff research MCP'
fi
# Prefix condition blocks listing and access to decisions, links and every other object.
gcloud storage buckets add-iam-policy-binding gs://crypto-prediction-lab-research-308289525742 \
  --member="serviceAccount:$research_identity" --role=roles/storage.objectViewer \
  --condition='title=read-frozen-mcp-scopes,expression=resource.name.startsWith("projects/_/buckets/crypto-prediction-lab-research-308289525742/objects/mcp/scopes/v1/")' --quiet
# Preserve existing services' identity, resources, environment, mounts and IAM.
gcloud run deploy crypto-history-collector --project=gitunderstand --region=us-central1 --image="$research_image" --quiet
gcloud run deploy crypto-research-mcp --project=gitunderstand --region=us-central1 \
  --image="$research_image" --service-account="$research_identity" \
  --port=8080 --cpu=0.25 --memory=256Mi --execution-environment=gen1 --concurrency=1 \
  --min=0 --max=1 --max-instances=1 --cpu-throttling --no-cpu-boost --timeout=15s \
  --ingress=all --no-allow-unauthenticated \
  --set-env-vars='^;^DASHBOARD_ROLE=research-mcp;DASHBOARD_BIND_ADDRESS=0.0.0.0;MCP_ALLOWED_HOSTS=crypto-research-mcp-308289525742.us-central1.run.app,crypto-research-mcp-a53petnlfa-uc.a.run.app' \
  --labels=app=crypto-prediction-lab,component=research-mcp --quiet
