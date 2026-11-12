const test = require('node:test');
const assert = require('node:assert/strict');
const {validate} = require('../public/forecast-study.js');
const report = require('../public/forecast-pilot-v1.json');
const audit = require('../../../docs/research-audits/forecasts-2026-11-11.json');
test('public aggregate reproduces published holdout values and sample counts', () => {
  assert.ok(validate(report));
  for (const [group, cohort] of Object.entries(report.evaluation)) {
    for (const [method, fields] of Object.entries(cohort.strategies)) {
      for (const key of Object.keys(fields)) assert.deepEqual(fields[key],audit.evaluation[group].strategies[method][key]);
    }
  }
});
test('empty bins cannot acquire outcomes and the support must reconcile', () => {
  const copy = structuredClone(report), stats = copy.evaluation.pooled.strategies.market_midpoint;
  stats.reliability_bins[0].observed_yes_rate = 0;
  assert.equal(validate(copy),false);
  stats.reliability_bins[0].observed_yes_rate = null; stats.rows++;
  assert.equal(validate(copy),false);
});
