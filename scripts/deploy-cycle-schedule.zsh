#!/bin/zsh
set -euo pipefail
service_url=$(gcloud run services describe crypto-history-collector --project=gitunderstand --region=us-central1 --format='value(status.url)')
for cycle_job in crypto-cycle-followup crypto-cycle-checkpoints; do
  action=create
  if gcloud scheduler jobs describe "$cycle_job" --project=gitunderstand --location=us-central1 >/dev/null 2>&1; then
    action=update
  fi
  if [[ "$cycle_job" == crypto-cycle-checkpoints ]]; then
    cycle_cron='14,29,44,59 * * * *'
    cycle_path=collect-opening
    cycle_retries=4
  else
    cycle_cron='0,15,30,45 * * * *'
    cycle_path=collect-followup
    cycle_retries=2
  fi
  gcloud scheduler jobs "$action" http "$cycle_job" \
   --project=gitunderstand --location=us-central1 --schedule="$cycle_cron" --time-zone=UTC \
   --uri="$service_url/$cycle_path" --http-method=POST \
   --oidc-service-account-email=crypto-history-scheduler@gitunderstand.iam.gserviceaccount.com \
   --oidc-token-audience="$service_url" --attempt-deadline=60s --max-retry-attempts="$cycle_retries" \
   --min-backoff=5s --max-backoff=5s --max-doublings=0 --max-retry-duration=100s --quiet
done
