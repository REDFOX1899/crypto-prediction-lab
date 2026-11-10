# Market replay and minute 10 observations

Replay opens from Home asset cards, Single market settled details and Data Lab window details. The slider advances from open to close in whole minutes. Only candles whose end is at or before the selected time appear. Missing minutes remain gaps. The official outcome appears at minute 15. Playback pauses when hidden, closes on workspace changes and cancels pending reads when another window is selected.

The line shows archived one-minute YES ask closes on a fixed 0–100¢ scale. Exact OHLC values remain available. These quotes were downloaded after settlement. Reference index history and earlier order books cannot be reconstructed from those candles.

## Prospective checkpoint collection

The private `crypto-history-collector` service exposes `POST /collect-minute-ten` under its existing Cloud Run IAM policy. The handler enforces a 50-second internal deadline below the service’s 60-second request timeout. The public dashboard has no matching write route. Scheduler job `crypto-minute-ten` invokes it at UTC minutes 10/25/40/55 using the existing dedicated scheduler identity. It has no automatic retries and a 60-second attempt deadline. No continuously running worker or new provider subscription is used.

The handler only starts collection during seconds 600–659 of the current 15-minute interval. It checks the market belongs to that exact interval. It saves one document per asset and open time to `minute_ten_v1/s<open_timestamp>_<symbol>` in the named `crypto-prediction-lab` database. Conditional creation preserves the first attempt. Duplicate requests read the existing record. Successfully stored assets survive partial failure.

Records contain intended cutoff, request start, actual receive time, seconds after cutoff, capture status, native ticker, target and top-ten YES/NO bids with quantities. The market and book requests are sequential and non-atomic. An unavailable provider response stores an explicit unavailable observation. A receipt after the capture minute is marked late. A scheduler delivery outside that minute is skipped, so absence remains visible.

A minute-10 scheduler invocation generally receives data after the exact cutoff. Use the actual receipt time when constructing research datasets. Replay reveals a saved checkpoint only when the slider time reaches its receipt time. No checkpoint exists for historical windows before this release.

`GET /api/research/checkpoint?asset=BTC&ticker=...` validates asset/ticker identity, looks up a completed archived window and returns its saved observation or an explicit missing state. It cannot invoke collection. The dashboard reads through its existing database viewer identity.

## Operation

Deploy the same verified application image to the existing public and private services while retaining their identities, environment, mounts and resource limits. Run `zsh scripts/deploy-minute-ten.zsh` to create or update the dedicated schedule. The design plans up to 288 small documents per day, plus three read checks and up to six public provider requests per run. Collection gaps require inspection. Retention is durable with no automatic deletion.

Verify the public replay with real archived minutes and live quotes. Verify private unauthenticated invocation is rejected, observe the first scheduled capture and read back the three records. Compare the actual receipt times and fields before describing the dataset as available.
