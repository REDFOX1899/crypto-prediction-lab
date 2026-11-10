#!/bin/zsh
set -euo pipefail
project=gitunderstand
region=us-central1
service_url=$(gcloud run services describe crypto-history-collector --project="$project" --region="$region" --format='value(status.url)')
action=create
if gcloud scheduler jobs describe crypto-market-context --project="$project" --location="$region" >/dev/null 2>&1; then
  action=update
fi
gcloud scheduler jobs "$action" http crypto-market-context \
 --project="$project" --location="$region" --schedule='*/5 * * * *' --time-zone=UTC \
 --uri="$service_url/collect-context" --http-method=POST \
 --oidc-service-account-email=crypto-history-scheduler@gitunderstand.iam.gserviceaccount.com \
 --oidc-token-audience="$service_url" --attempt-deadline=60s --max-retry-attempts=1 \
 --min-backoff=60s --max-backoff=120s --quiet
