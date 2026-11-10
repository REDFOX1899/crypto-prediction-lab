# Focus UI: price, target and time

The homepage is a quarter-hour crypto research desk for Kalshi users. It prioritizes live reference prices, distance from the contract threshold and remaining time. Familiar YES/NO ask labels, fixed targets, asset selection and read-only status remain. The design is original; it does not copy Kalshi's assets or add trade controls. The screenshots supplied by the owner informed the target/quote hierarchy.

## Research translated into UI

- Apple's Dark Mode guidance describes base and elevated surfaces. We use a near-black canvas with raised charcoal cards, plus a separately designed porcelain light palette. Theme choice initially follows the system, can be toggled with a labeled 44px control, persists locally when available, and updates the detailed chart without resetting its data or zoom. Storage failure does not disable the control. Source: https://developer.apple.com/design/human-interface-guidelines/dark-mode
- Apple's Color and Charts guidance favors contextual contrast and accessible chart descriptions. Main reference numbers are the strongest visual element in each card; target, delta and time form a consistent comparison layout. Charts have text summaries and target legends. Sources: https://developer.apple.com/design/human-interface-guidelines/color and https://developer.apple.com/design/human-interface-guidelines/charts
- NN/g discusses visual hierarchy and simple visual encodings for dashboard scanning. The large concept illustration is replaced by a compact SVG window dial, and explanatory sections move below the market cards. Tiny reference plots and linear progress graphics provide context rather than speculative signals. Sources: https://www.nngroup.com/articles/visual-hierarchy-ux-definition/ and https://www.nngroup.com/articles/dashboards-preattentive/
- W3C requires meaning beyond color alone. Above/below status uses arrows, signed amounts and words; source state has a text label; elapsed/final-minute graphics have accessible descriptions. A core palette test checks primary, secondary, positive, negative and accent text at at least 4.5:1 on page/card surfaces. This is not a claim of a complete WCAG audit. Motion follows reduced-motion preferences. Source: https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html

## Live reference semantics

BTC uses BRTI; ETH uses ETHUSD_RTI; SOL uses SOLUSD_RTI. These are CF Benchmarks reference feeds delivered through Kalshi, not a promise of an exchange spot price or the official settlement value. Existing public snapshot metadata supplies each active 900-second contract and its numeric greater_or_equal floor_strike target. Delta = reference value minus target; percentage = 100 times delta / target. Unsupported/missing targets do not produce a distance.

Six client listeners (market and reference for each asset) share the existing single two-second all-asset HTTP request. No new upstream subscriptions, API keys, permissions, cloud services, inference or database writes are needed. Existing slow metadata/history refresh and request budgets remain. The shared request's server time anchors the homepage clock using elapsed monotonic browser time.

A current price must be positive, within the active market window, no more than ten seconds old and at most two seconds ahead. Reference delivery must be recent and metadata must remain fresh. Stale/failed references cannot retain a live distance. New market windows reject older reference timestamps and reset the miniature chart observations.

The countdown shows time until the final minute, then switches to time until close. The final minute gets an amber surface and explicit label. Closing does not infer an official result. Settlement follows the individual rules; see https://help.kalshi.com/en/articles/13823838-crypto-markets .

SVG sparklines are built from a bounded buffer of this visit's actual observed values (maximum 120 per asset, last two minutes). Duplicate timestamps do not increase sample count. Lines break across gaps exceeding six seconds; no missing tick is fabricated. Target and observed values share an automatically scaled vertical range. A current marker appears only with fresh data. The mini plot is context, not a probability, predicted path or historical backfill.

## Implementation and cost

Semantic light/dark color tokens coordinate all three workspaces. Pure data/geometry logic lives in home-visuals.js; theme behavior in theme.js; focus.css defines the visual system. Rust adds only three fixed read-only static asset routes. The order book remains visible in Single Market.

The old roughly 148 KB hero WebP is no longer loaded on Home. New SVG artwork is native markup and observed-data geometry; there is no raster generation call or graphics dependency. New CSS and modules add only tens of KB. Existing hosting limits remain, and total traffic/storage charges are still subject to the previously documented $40 budget caveat.

## Validation and first-user checks

Test source freshness, invalid targets, contract boundaries, reference isolation, line gaps, palette contrast, storage failures and six-listener request sharing. Verify light/dark navigation and mobile overflow. Verify actual hosted reference values and deltas before calling the release complete.

Next user test: ask five people who already follow Kalshi's 15-minute crypto markets to identify (1) the current reference price, (2) whether it is above or below target, and (3) when the final minute starts. Record task completion and confusion before claiming the design improves retention or attracts customers. No conversion or predictive-performance claim is implied by the redesign.

## Published verification

Revision crypto-prediction-lab-00011-wl9; image focus-ui-20261109; build 09aa54eb-e417-4453-8fa3-216f2ff47759. All 33 Rust and 35 JavaScript tests passed; formatting and clippy passed. Production static assets returned HTTP 200 with the expected CSS/JavaScript types. Hosted BTC, ETH and SOL reference values, numeric target differences, source ages and final-minute countdowns were checked. Their mini plots accumulated real observations. A sampled BTC check showed $75,051.33 versus $75,217.95, correctly displaying −$166.62 and 0.2215% below target. These are release-check observations, not current-price claims.

Light/dark switching and detailed chart navigation passed without console errors. The preference persisted on reload. At 390px the homepage and Combo Lab had no horizontal overflow; the live mobile card displayed reference, delta, target, SVG plot, timing bar and quotes together. Temporary viewport overrides were reset after QA. No new provider permissions, external graphics dependency or cloud resources were added.
