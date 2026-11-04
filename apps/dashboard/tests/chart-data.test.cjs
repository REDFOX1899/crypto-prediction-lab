const test = require('node:test');
const assert = require('node:assert/strict');
const { candleData, lineData, combine } = require('../public/chart-data.js');

test('candles contain observed open, high, low and close without inventing empty minutes', () => {
  const bars = candleData([
    { t: 120001, v: 100 }, { t: 125000, v: 105 }, { t: 130000, v: 98 },
    { t: 179999, v: 102 }, { t: 240010, v: 99 },
  ]);
  assert.deepEqual(bars, [
    { time: 120, open: 100, high: 105, low: 98, close: 102 },
    { time: 240, open: 99, high: 99, low: 99, close: 99 },
  ]);
});

test('line keeps the latest observation per second', () => {
  assert.deepEqual(lineData([{t:1000,v:1},{t:1500,v:2},{t:2000,v:3}]), [
    {time:1,value:2},{time:2,value:3},
  ]);
});


test('history and live candles stay separate across a data gap', () => {
  const history=[{time:60,open:'10',high:'12',low:'9',close:'11'}];
  const result=combine(history,[{t:240000,v:20},{t:250000,v:21}],'candles');
  assert.deepEqual(result,[{time:60,open:10,high:12,low:9,close:11},{time:120},{time:240,open:20,high:21,low:20,close:21}]);
});

test('observations covered by completed history do not overwrite it', () => {
  const history=[{time:60,open:'1',high:'2',low:'1',close:'2'}];
  assert.deepEqual(combine(history,[{t:61000,v:9},{t:120000,v:3}],'line'),[{time:60,value:2},{time:120,value:3}]);
});
