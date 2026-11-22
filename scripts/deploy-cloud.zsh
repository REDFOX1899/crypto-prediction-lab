#!/bin/zsh
set -euo pipefail
# Deploy only the crypto service. Runtime reads its dedicated Secret Manager entries.
image="${1:-us-central1-docker.pkg.dev/gitunderstand/crypto-prediction-lab/dashboard:quant-latency-20261122}"
gcloud run deploy crypto-prediction-lab \
  --project=gitunderstand --region=us-central1 \
  --image="$image" \
  --service-account=crypto-prediction-lab@gitunderstand.iam.gserviceaccount.com \
  --port=8080 --cpu=0.25 --memory=256Mi --concurrency=1 --execution-environment=gen1 \
  --min=0 --max=1 --min-instances=0 --max-instances=1 \
  --cpu-throttling --no-cpu-boost --timeout=30s \
  --ingress=all --allow-unauthenticated \
  --set-env-vars=DASHBOARD_BIND_ADDRESS=0.0.0.0,DASHBOARD_DELIVERY=poll,KALSHI_PRIVATE_KEY_PATH=/secrets/kalshi/private-key.pem,HISTORY_PROJECT=gitunderstand,HISTORY_DATABASE=crypto-prediction-lab,BLOCKWORKS_PUBLIC_DISPLAY=approved,GEMINI_API_KEY_FILE=/secrets/gemini/key,GEMINI_API_KEY_SECONDARY_FILE=/secrets/gemini-secondary/key \
  --set-secrets=KALSHI_API_KEY_ID=crypto-prediction-lab-key-id:1,/secrets/kalshi/private-key.pem=crypto-prediction-lab-private-key:1,/secrets/gemini/key=crypto-prediction-lab-gemini-key:1,/secrets/gemini-secondary/key=crypto-prediction-lab-gemini-key-secondary:1 \
  --labels=app=crypto-prediction-lab,environment=public-pilot \
  --quiet
