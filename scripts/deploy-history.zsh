#!/bin/zsh
set -euo pipefail
image=us-central1-docker.pkg.dev/gitunderstand/crypto-prediction-lab/dashboard:cmc-context-20261109-r3
gcloud run deploy crypto-history-collector --project=gitunderstand --region=us-central1 \
 --image="$image" --service-account=crypto-history-collector@gitunderstand.iam.gserviceaccount.com \
 --port=8080 --cpu=0.25 --memory=256Mi --execution-environment=gen1 --concurrency=1 --min=0 --max=1 --max-instances=1 \
 --cpu-throttling --no-cpu-boost --timeout=60s --ingress=all --no-allow-unauthenticated \
 --set-env-vars=DASHBOARD_ROLE=collector,HISTORY_PROJECT=gitunderstand,HISTORY_DATABASE=crypto-prediction-lab,DASHBOARD_BIND_ADDRESS=0.0.0.0,CMC_API_KEY_PATH=/secrets/cmc/key.txt \
 --set-secrets=/secrets/cmc/key.txt=crypto-prediction-lab-cmc-key:1 \
 --labels=app=crypto-prediction-lab,component=history-collector --quiet
