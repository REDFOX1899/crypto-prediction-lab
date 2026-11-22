const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { latencyTier, fmtMs, speedupLabel, computePercentile } = require('../public/latency-lab.js');
const { route } = require('../public/navigation.js');

test('navigation routes /?view=latency cleanly', () => {
  assert.deepEqual(route('/?view=latency'), { mode: 'latency', asset: 'BTC' });
});

test('latency helpers format sub-millisecond, millisecond, second, tier and percentiles accurately', () => {
  assert.equal(latencyTier(120), 'fast');
  assert.equal(latencyTier(400), 'ok');
  assert.equal(latencyTier(950), 'slow');
  assert.equal(fmtMs(0.25), '0.25ms');
  assert.equal(fmtMs(24.6), '24.6ms');
  assert.equal(fmtMs(1420), '1.42s');
  assert.equal(fmtMs(null), '—');
  assert.equal(speedupLabel(440, 0.25), '1,760× faster');
  assert.equal(speedupLabel(2100, 1020), '2.1× faster');
  assert.equal(computePercentile([10, 20, 30, 40, 50], 0.50), 30);
  assert.equal(computePercentile([10, 20, 30, 40, 50], 0.95), 50);
  assert.equal(computePercentile([], 0.50), null);
});

test('latency-lab browser bundle exposes enter/leave lifecycle hooks expected by index.html', () => {
  const src = fs.readFileSync(path.join(__dirname, '../public/latency-lab.js'), 'utf8');
  assert.ok(src.includes('enter()'));
  assert.ok(src.includes('leave()'));
});
