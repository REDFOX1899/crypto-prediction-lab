#!/bin/zsh
set -euo pipefail
read -rs 'kalshi_key_id?Enter your Kalshi API key ID (hidden): '
printf '\n'
if [[ -z "$kalshi_key_id" ]]; then
  printf 'No ID entered. Nothing uploaded.\n'
  exit 1
fi
printf '%s' "$kalshi_key_id" | gcloud secrets versions add crypto-prediction-lab-key-id --project=gitunderstand --data-file=- --quiet
unset kalshi_key_id
printf 'Kalshi key ID saved securely. Tell Codex: key ID uploaded.\n'
