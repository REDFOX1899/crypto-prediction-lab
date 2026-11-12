const { test } = require('node:test');
const assert = require('node:assert/strict');
const { cents, nearest } = require('../public/hourly.js');
test('hourly board excludes absent and boundary asks, retains sub-cent quotes', () => {
  for (const v of [null, undefined, '', ' ', '0', '1', 'garbage']) assert.equal(cents(v), null);
  assert.equal(cents('0.0250'), 2.5);
});
test('default strike balances two available asks and handles an empty listing', () => {
  const rows = [['1','0.01'],['0.57','0.46'],['0.45','0.59']].map(([yes_ask_dollars,no_ask_dollars]) => ({yes_ask_dollars,no_ask_dollars}));
  assert.equal(nearest(rows), rows[1]);
  assert.equal(nearest([]), undefined);
});
