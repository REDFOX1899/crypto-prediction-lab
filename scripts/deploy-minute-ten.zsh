#!/bin/zsh
set -euo pipefail
service_url=$(gcloud run services describe crypto-history-collector --project=gitunderstand --region=us-central1 --format='value(status.url)')
action=create
if gcloud scheduler jobs describe crypto-minute-ten --project=gitunderstand --location=us-central1 >/dev/null 2>&1; then
  action=update
fi
gcloud scheduler jobs "$action" http crypto-minute-ten \
 --project=gitunderstand --location=us-central1 --schedule='10,25,40,55 * * * *' --time-zone=UTC \
 --uri="$service_url/collect-minute-ten" --http-method=POST \
 --oidc-service-account-email=crypto-history-scheduler@gitunderstand.iam.gserviceaccount.com \
 --oidc-token-audience="$service_url" --attempt-deadline=60s --max-retry-attempts=0 --quiet
