const { test } = require('node:test');
const assert = require('node:assert/strict');
const {
  SERIES_LIGHT,
  SERIES_DARK,
  FAMILY_ORDER,
  FAMILY_ORDER_GEN2,
  hexLum,
  isDarkHex,
  familyColor,
  palette,
  computeParetoFrontier,
} = require('../public/chart-theme.js');

test('Artificial Analysis palette defines vivid light & dark series colors and all 20 Gen-1 + Gen-2 tournament families', () => {
  assert.equal(FAMILY_ORDER.length, 10);
  assert.equal(FAMILY_ORDER_GEN2.length, 10);
  assert.equal(SERIES_LIGHT.purple, '#7F4BF3');
  assert.equal(SERIES_LIGHT.green, '#34A853');
  assert.equal(SERIES_LIGHT.terracotta, '#CC785C');
  assert.equal(SERIES_LIGHT.ink, '#1F1F1F');
  for (const [family] of [...FAMILY_ORDER, ...FAMILY_ORDER_GEN2]) {
    assert.match(familyColor(family, false), /^#[0-9A-Fa-f]{6}$/);
    assert.match(familyColor(family, true), /^#[0-9A-Fa-f]{6}$/);
  }
  assert.equal(isDarkHex('#1F1F1F'), true);
  assert.equal(isDarkHex('#FFFFFF'), false);
  assert.ok(hexLum('#FFFFFF') > hexLum('#1F1F1F'));
  const p = palette();
  assert.equal(p.grid, '#CCCCCC');
  assert.equal(p.surface, '#FFFFFF');
});

test('computeParetoFrontier finds exact non-dominated (low drawdown, high bankroll) frontier', () => {
  const sample = [
    { id: 'A1', max_drawdown_pct: 1.0, end_usd: 101.0, trades: 20 },
    { id: 'A2', max_drawdown_pct: 2.3, end_usd: 104.4, trades: 197 },
    { id: 'A3', max_drawdown_pct: 3.0, end_usd: 103.0, trades: 50 }, // dominated by A2
    { id: 'A4', max_drawdown_pct: 4.5, end_usd: 115.9, trades: 120 },
    { id: 'A5', max_drawdown_pct: 8.0, end_usd: 95.0, trades: 90 },  // dominated by A4
    { id: 'A6', max_drawdown_pct: 0.0, end_usd: 100.0, trades: 0 },  // inactive, ignored
  ];
  const frontier = computeParetoFrontier(sample);
  assert.deepEqual(frontier.map(a => a.id), ['A1', 'A2', 'A4']);
});
