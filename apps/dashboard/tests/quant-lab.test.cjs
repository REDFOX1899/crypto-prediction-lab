const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { formatPosture, signedCents, signedPct, signedUsd, selectGridRow, filterAndSortAgents } = require('../public/quant-lab.js');
const { route } = require('../public/navigation.js');

test('navigation supports quant workspace deep links alongside existing views', () => {
  assert.deepEqual(route('/?view=quant'), { mode: 'quant', asset: 'BTC' });
  assert.deepEqual(route('/?view=quant&asset=ETH'), { mode: 'quant', asset: 'ETH' });
  assert.deepEqual(route('/?view=data'), { mode: 'data', asset: 'BTC' });
  assert.deepEqual(route('/'), { mode: 'home', asset: 'BTC' });
});

test('quant posture formatter maps every server posture to explicit non-color label and tone', () => {
  assert.deepEqual(formatPosture('YES_TAKER_CANDIDATE'), { label: 'YES Candidate (Taker)', tone: 'up' });
  assert.deepEqual(formatPosture('NO_TAKER_CANDIDATE'), { label: 'NO Candidate (Taker)', tone: 'down' });
  assert.deepEqual(formatPosture('WAIT_COMPRESSION'), { label: 'Wait · Min 10–13 Window', tone: 'warn' });
  assert.deepEqual(formatPosture('PASS'), { label: 'Pass · Below Fee Hurdle', tone: 'neutral' });
});

test('signed numeric formatters and interactive simulation grid lookup handle edge cases cleanly', () => {
  assert.equal(signedCents(0.3), '+0.30¢');
  assert.equal(signedCents(-2.4), '-2.40¢');
  assert.equal(signedCents(null), '—');
  assert.equal(signedPct(2.93), '+2.93%');
  assert.equal(signedPct(-4.33), '-4.33%');
  const mockSim = {
    interactive_grid: {
      'compression_11_13|taker|0.3': { trades: 71, net_pnl_cents: -100.1 }
    }
  };
  assert.equal(selectGridRow(mockSim, 'compression_11_13', 'taker', 0.3).trades, 71);
  assert.equal(selectGridRow(mockSim, 'missing', 'taker', 0.3), null);
  assert.equal(signedUsd(4.44), '+$4.44');
  assert.equal(signedUsd(-1.99), '-$1.99');
});

test('100-agent $100 paper-trading tournament artifact and filter/sort logic validate cleanly', () => {
  const sim = JSON.parse(fs.readFileSync(path.join(__dirname, '../public/btc-15m-simulation.json'), 'utf8'));
  const t = sim.tournament_100;
  assert.equal(t.total_agents, 100);
  assert.equal(t.start_bankroll_usd, 100);
  assert.equal(t.agents.length, 100);
  assert.ok(sim.advanced_diagnostics.latency_decay_frontier.length >= 6);
  assert.equal(sim.advanced_diagnostics.minute_hurdle_heatmap.length, 14);
  assert.equal(sim.advanced_diagnostics.utc_session_profile.length, 24);

  const takers = filterAndSortAgents(t.agents, { family: 'ALL', mode: 'taker', sort: 'end_usd', search: '' });
  assert.ok(takers.length > 40);
  assert.ok(takers[0].end_usd >= takers[takers.length - 1].end_usd);
  const lowDd = filterAndSortAgents(t.agents, { family: 'Favorite-Longshot Bias', mode: 'taker', sort: 'max_drawdown_pct', search: '72-88c' });
  assert.ok(lowDd.length >= 1);
  assert.ok(lowDd[0].end_usd > 100);
});

test('quant-lab browser bundle exposes enter/leave lifecycle hooks expected by index.html', () => {
  const src = fs.readFileSync(path.join(__dirname, '../public/quant-lab.js'), 'utf8');
  for (const fn of ['enterHome()', 'leaveHome()', 'enter()', 'leave()']) {
    assert.ok(src.includes(fn), `Missing browser lifecycle hook ${fn}`);
  }
});
