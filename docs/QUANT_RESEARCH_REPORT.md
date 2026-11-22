# Quantitative Research Report · BTC 15-Minute Binary Microstructure

**Dataset:** `1,614` settled `KXBTC15M` contracts (`2026-11-04 23:00 UTC → 2026-11-22 00:45 UTC`), joined with `1,613` `KXETH15M`, `1,608` `KXSOL15M`, `6,483` opening/+45s L2 books, `3,348` minute-10 L2 books, `2,468` CoinMarketCap samples, and `746` Tavily/RSS news snapshots.  
**Evaluation Protocol:** Strict **5-fold expanding chronological walk-forward** over the final **1,050 unseen OOS windows**, separated by a **4-window (1-hour) purged embargo**. All results reflect **read-only theoretical paper simulation** (`$100.00` starting bankroll per agent).

---

## 1. Exact Market Mechanics & Fee Geometry

Kalshi's 15-minute binary crypto contracts settle YES (`$1.00`) if the CF Benchmarks 60-second final-minute reference average finishes at or above the fixed opening strike, and NO (`$0.00`) otherwise.

### Quadratic Fee Schedule (Exact Rounding)
For a contract priced at $p \in (0, 1)$:
$$\text{Fee}_{\text{taker}}(p) = \frac{\lceil 0.07 \cdot p(1-p) \cdot 10^6 \rceil}{10^6}, \qquad \text{Fee}_{\text{maker}}(p) = \frac{\lceil 0.0175 \cdot p(1-p) \cdot 10^6 \rceil}{10^6}$$

Because $p(1-p)$ peaks at $p = 0.50$, **early-window contracts near 50¢ suffer maximum fee drag (`1.75¢` taker) AND widest spreads (`1.00¢`)**, whereas late-window contracts dispersed toward `10–25¢` or `75–90¢` experience a **3–5× reduction in quadratic fees** plus **10× median spread compression (`1.00¢ → 0.10¢`)**.

| Window Phase | Minutes | Avg / Median Spread | Avg Taker Fee | Total Taker Friction | Avg 1m Volume | Midpoint Brier |
| --- | :---: | :---: | :---: | :---: | ---: | :---: |
| Early Friction Trap | `Min 1–5` | `0.97¢ / 1.00¢` | `1.46¢` | `1.95¢` | `194k` | `0.194 – 0.231` |
| Mid Discovery | `Min 6–9` | `0.78¢ / 1.00¢` | `1.10¢` | `1.49¢` | `186k` | `0.138 – 0.175` |
| **Optimal Compression** | **`Min 10–13`** | **`0.49¢ / 0.10¢`** | **`0.67¢`** | **`0.92¢`** | **`293k`** | **`0.079 – 0.126`** |
| Final-Min Averaging | `Min 14` | `0.40¢ / 0.10¢` | `0.45¢` | `0.65¢` | `560k` | `0.070` |

---

## 2. Models & Signal Formulations

### A. Zero-Intercept Symmetric L2 Log-Odds Residual Model (`btc15m_residual_l2_v1`)
To prevent overfitting to directional crypto drift (`46.28%` YES rate over the sample period), the model anchors directly to the Kalshi implied log-odds $\ell_m = \operatorname{logit}(p_{\text{mid}})$ and estimates a **zero-intercept** regularized residual:
$$\hat{p}_{\text{quant}}(m) = \sigma\!\left(\operatorname{logit}(p_{\text{mid}}) + \operatorname{clip}\!\left(\sum_{j=1}^{6} w_{m,j} \cdot \operatorname{clip}\!\left(\frac{x_j}{s_{m,j}}, -4, 4\right), -0.65, 0.65\right)\right)$$

Fitted via Newton-Raphson with heavy $\ell_2 = 250$ shrinkage on strictly causal features:
1. **`xasset_lead`**: $\frac{1}{2}(\operatorname{logit}(p_{\text{ETH}}) + \operatorname{logit}(p_{\text{SOL}})) - \operatorname{logit}(p_{\text{BTC}})$ — strongest positive OOS Brier Skill contributor (`+0.22%` to `+0.35%` in Min 12–13).
2. **`book_imb10`**: Top-10 L2 bid-size imbalance $\frac{Q_{\text{YES}} - Q_{\text{NO}}}{Q_{\text{YES}} + Q_{\text{NO}}}$.
3. **`cmc_1h`**: Causal CoinMarketCap 1-hour spot momentum.
4. **`news_tone`**: Single-pass `gemini-decision-1` scored Tavily/RSS catalyst net tone.
5. **`trade_press` & `mom_1m`**: Trade-to-mid execution pressure and 1-minute quote velocity.

### B. Black-Scholes Digital Binary Option Mispricing (`N(d2)`)
Using causal trailing realized 15-minute volatility $\hat{\sigma}_{15m}$ and remaining time fraction $\tau = \frac{15 - m}{15}$:
$$d_{2,\text{mkt}} = \Phi^{-1}(p_{\text{mid}}), \qquad p_{\text{BS}} = \Phi\!\left(\frac{d_{2,\text{mkt}}}{\hat{\sigma}_{15m} / \sigma_0} + \mu_{1h}\sqrt{\tau}\right), \qquad \Gamma_{\text{bin}} \propto \frac{\phi(d_{2,\text{mkt}})}{\sqrt{\tau}}$$
Evaluated both standalone and blended with `xasset_lead`, plus digital-gamma pin-risk filtering near `50¢`.

### C. Google `gemini-decision-1` Hybrid Overlay (0 Output Tokens)
A compact, fully blinded feature packet (zero ticker symbol, zero calendar date, zero USD strike) queries `POST /v1beta/decisions` across four calibrated decision heads (`settle_yes` noul probability, `action` choice, `timing_window` 0–2 score, `catalyst_alignment` choice) and blends conservatively ($\alpha = 0.08$):
$$\hat{p}_{\text{hybrid}} = \sigma\!\left(\operatorname{logit}(\hat{p}_{\text{quant}}) + 0.08 \cdot \operatorname{clip}\!\left(\operatorname{logit}(p_{\text{gemini}}) - \operatorname{logit}(p_{\text{mid}}), -0.9, 0.9\right)\right)$$

---

## 3. 100-Agent Parallel `$100` Paper-Trading Tournament

We evaluated **100 distinct paper-trading agents** across **10 strategy families** in parallel (`0.35s` wall time on 10 cores) over the exact same **1,050 unseen OOS windows**, each starting at **`$100.00`** with integer contract sizing (`fixed_1`, `pct_2/3`, `quarter_kelly` capped at 4% risk/trade, `half_kelly`) and strict **Benjamini-Hochberg FDR** multiple-testing correction:

- **Distribution (`$100` start):** `10/100` profitable · `89/100` negative · `1/100` flat (`$100.00` Cash Benchmark) · Mean `$89.93` · Median `$95.31` · P90 `$100.03` · P10 `$75.34`.
- **Multiple-Testing Discipline:** **`0/100` agents** achieve Benjamini-Hochberg FDR `q ≤ 0.05` across 100 simultaneous strategies — demonstrating why high-frequency binary markets require severe skepticism against backtest selection bias.
- **Best Realistic Taker (`n ≥ 30`, Fixed-1):** **`FLB Fav 72–88¢ + XAsset Min11–13`** turned `$100.00 → $104.44` (`+4.44%`, `197` trades, `84.8%` win rate, `+2.25¢/trade` EV after taker fees, `2.3%` max drawdown, Sharpe `+6.64`, unadjusted bootstrap `p = 0.050`).
- **Second Realistic Taker (`n ≥ 30`, Fixed-1):** **`6-Factor L2 Min12–14 Taker 0.6¢`** turned `$100.00 → $103.46` (`+3.46%`, `227` trades, `48.9%` win rate, `+1.52¢/trade`, `6.2%` max drawdown, Sharpe `+2.94`).
- **Adverse-Selection Proof (`Touch-Maker` vs `Mid-Maker`):** Optimistic Midpoint-Maker (`6-Factor L2 Min12–14`) posts `$109.78` Fixed-1 / `$115.93` Q-Kelly (`+3.46¢/trade`), but when realistic **Touch-Fill Adverse Selection** is enforced (limit order fills only if next minute's low/high crosses the resting limit), **every maker agent drops below `$100`** (best: `$98.01`, `−0.26¢/trade`).

---

## 4. Latency Architecture & Edge-Decay Engineering

Empirical Min 10–13 quote movement averages **`4.5¢/min`** (`3.8¢` over opening→+45s L2 books). Under Brownian diffusion scaling $\Delta p_{\text{adv}}(\Delta t) \propto \sqrt{\Delta t / 60}$, cutting signal latency directly preserves realized EV:

| Optimization Layer | Technique Implemented in Rust | Impact |
| --- | --- | --- |
| **Zero-REST Live Metadata** | `ensure_snapshot_market` reuses active `Market` metadata from memory whenever `stream_tx` is live and `close_time > now` | Eliminates `120–180 ms` Kalshi REST round-trips |
| **Two-Tier SWR + Singleflight** | Rust quant + exact Kalshi fees recompute on every live WS tick (`<200 µs`); Gemini overlay serves warm (`<10 s`) or SWR (`10–30 s`) with 1 coalesced `tokio::spawn` refresh | Cuts warm `/api/quant/signal` from `375–508 ms` → **`<2 ms`** |
| **Compact Prompt Prefill** | Compressed blinded feature string + tight rubrics (~280 vs ~770 input tokens; `0` output tokens) | Reduces live `gemini-decision-1` prefill latency & token spend by ~60% |
| **Persistent Transport Pools** | `TCP_NODELAY`, `tcp_keepalive(30s)`, `pool_idle_timeout(90s)` on both `kalshi-client` and `gemini_decisions` | Removes cold TLS handshake spikes |
| **Observability** | RFC `Server-Timing` header + 128-sample lock-bounded `p50/p95` latency ring buffer in JSON & UI | Live latency auditing on `yesnotape.com` |

---

## 5. Google `gemini-decision-1` & Tavily Audit

- **360-Call Blinded Benchmark (`120` unseen windows × Min 4/9/12):** `0` errors · `p50 = 287.8 ms` · `p95 = 441.2 ms` · `p99 = 541.3 ms` · `0` output tokens · `$0.0255` total spend. At Minute 9, the Hybrid (`Quant + Gemini`) achieved the lowest Brier score (`0.15190` vs Quant `0.15212` vs Market `0.15303`), while Gemini's `action` head prudently selected `wait_or_pass` on `87.5%` of windows.
- **Single-Pass Tavily News Screening:** Scoring all candidate headlines in **one single `POST /v1beta/decisions` call** (`36` questions in `425 ms` offline; `257 ms` live in Cloud Run) strips SEO converter pages and off-topic false positives (e.g., removing `"FBI's criminal information database hacked"` previously mis-tagged as bearish crypto news) while retaining high-impact liquidation and regulatory catalysts.

---

## 6. Gen-2 100-Agent Multi-Asset (`BTC + ETH + SOL`) & Sub-100ms Latency Audit (200 Total Agents)

Following a 10-specialist parallel quant research review, we ran **100 additional Gen-2 paper-trading agents (`B001–B100`)** across all **3,150 out-of-sample contract-windows** (`1,050` cycles × `BTC`, `ETH`, `SOL`) with strict 5-fold expanding chronological walk-forward, 1-hour purged embargo, empirical Brownian latency drift (`2ms` → `5,000ms`), split-half chronological stability (`H1` vs `H2`), and exact **1-day ("tomorrow") empirical profit/loss distributions**, combined with Gen-1 into a **200-agent Benjamini-Hochberg FDR audit**:

| Key Discovery | Empirical Result (`$100` Start, Strict OOS) | Interpretation & Risk Reality |
| --- | --- | --- |
| **Falsification Proof (`Pure FLB` vs `XAsset`)** | `B099` Pure FLB 72–88¢ (No Cross-Asset): **`$86.59`** (`-13.41%`, `-3.34¢/tr`, `401` trades) vs `B027` 3-Asset Best-EV FLB + XAsset: **`$110.61`** (`+10.61%`, `+2.47¢/tr`, `429` trades) | Blindly buying 72–88¢ favorites loses badly after Kalshi fees; cross-asset lead-lag confirmation is doing the real work. |
| **SOL Inefficiency vs ETH Efficiency** | **SOL (`KXSOL15M`)**: **`10/10` profitable** (median `$107.00`; `B013` SOL BTC+ETH Lead 50ms: **`$107.81`**, `+8.78¢/tr`, Sharpe `+13.31`, `H1 +$1.91 / H2 +$5.90`; `B018` SOL FLB+XAsset: `$107.11`, `87.7%` win, `H1 +$3.51 / H2 +$3.60`) vs **ETH (`KXETH15M`)**: `1/10` profitable (median `$98.68`) | SOL's OOS base rate was balanced (`47.7%` YES), yet SOL quotes lag BTC+ETH repricing longer than ETH does. |
| **Best Balanced Realistic Taker** | `B083` (`EU+US Liquid Hours 08–21 UTC · 3-Asset Best-EV FLB 72–88¢ + XAsset`): **`$110.22`** (`+10.22%`, `230` trades, `87.4%` win, `+4.44¢/tr`, `2.6%` max DD, `H1 +$4.56 / H2 +$5.66`, `p = 0.0275`) | Avoids thin overnight books and triple-correlated fee drag (`N_eff ≈ 1.07` when buying 3 assets simultaneously). |
| **Honest 1-Day ("Tomorrow") Risk & 200-Agent FDR** | **`0/200` realistic takers (`n ≥ 30`)** survive combined 200-agent BH FDR `q ≤ 0.05`. Even `B083` & `B027` **lost money on `27.3%–38.5%` of individual 24-hour days** (`Worst 1d: -$2.35 to -$5.11` at 1-ct; `-$3.89` at 3% sizing). | **No strategy guarantees profit tomorrow.** Over 12 calendar days, binary favorite negative skew (~4:1 loss-to-win ratio) means any single day remains high-variance. Strictly read-only paper research. |
