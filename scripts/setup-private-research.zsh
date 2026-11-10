#!/bin/zsh
set -euo pipefail
research_bucket=crypto-prediction-lab-research-308289525742
if ! gcloud storage buckets describe "gs://$research_bucket" --project=gitunderstand >/dev/null 2>&1; then
  gcloud storage buckets create "gs://$research_bucket" --project=gitunderstand \
    --location=us-central1 --default-storage-class=STANDARD \
    --uniform-bucket-level-access --public-access-prevention --quiet
fi
research_project=$(gcloud storage buckets describe "gs://$research_bucket" --raw --format='value(projectNumber)')
if [[ "$research_project" != 308289525742 ]]; then
  print -u2 'Unexpected research bucket project'
  exit 1
fi
for research_role in roles/storage.objectCreator roles/storage.objectViewer; do
  gcloud storage buckets add-iam-policy-binding "gs://$research_bucket" \
    --member=serviceAccount:crypto-history-collector@gitunderstand.iam.gserviceaccount.com \
    --role="$research_role" --quiet
done
