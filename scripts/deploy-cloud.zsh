#!/bin/zsh
set -euo pipefail
# Deploy only the crypto service. Runtime reads its two dedicated secrets.
image="${1:-us-central1-docker.pkg.dev/gitunderstand/crypto-prediction-lab/dashboard:data-lab-20261109-r2}"
gcloud run deploy crypto-prediction-lab \
  --project=gitunderstand --region=us-central1 \
  --image="$image" \
  --service-account=crypto-prediction-lab@gitunderstand.iam.gserviceaccount.com \
  --port=8080 --cpu=0.25 --memory=256Mi --concurrency=1 --execution-environment=gen1 \
  --min=0 --max=1 --min-instances=0 --max-instances=1 \
  --cpu-throttling --no-cpu-boost --timeout=30s \
  --ingress=all --allow-unauthenticated \
  --set-env-vars=DASHBOARD_BIND_ADDRESS=0.0.0.0,DASHBOARD_DELIVERY=poll,KALSHI_PRIVATE_KEY_PATH=/secrets/kalshi/private-key.pem,HISTORY_PROJECT=gitunderstand,HISTORY_DATABASE=crypto-prediction-lab \
  --set-secrets=KALSHI_API_KEY_ID=crypto-prediction-lab-key-id:1,/secrets/kalshi/private-key.pem=crypto-prediction-lab-private-key:1 \
  --labels=app=crypto-prediction-lab,environment=public-pilot \
  --quiet
