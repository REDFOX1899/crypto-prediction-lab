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
test('binary YES range derives bid from NO ask and rejects crossed or absent quotes', () => {
  const range=(yes,no)=>require('../public/hourly.js').bidAsk({yes_ask_dollars:yes,no_ask_dollars:no});
  assert.deepEqual(range('.5325','.4800'),{bid:52,ask:53.25});
  assert.equal(range('.40','.50'),null);
  assert.equal(range('.40',null),null);
});
test('hourly sizes preserve fractional contracts and distinguish missing from zero',()=>{
  const {quantity,bidAsk}=require('../public/hourly.js');
  assert.equal(quantity('12.25'),12.25);
  assert.equal(quantity('0.00'),0);
  for(const value of [null,'',' ','-2','Infinity','5 contracts']) assert.equal(quantity(value),null);
  assert.deepEqual(bidAsk({yes_bid_dollars:'.49',yes_ask_dollars:'.54',no_ask_dollars:'.50'}),{bid:49,ask:54});
});
