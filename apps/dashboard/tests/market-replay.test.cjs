const {test}=require('node:test');
const assert=require('node:assert/strict');
const {visibleCandles}=require('../public/market-replay.js');
test('replay reveals completed minutes only and leaves missing minutes absent',()=>{
 const start=1800,data={open_time:new Date(start*1000).toISOString(),candles:[0,60,180,840].map(t=>({time:start+t,close:'.50'}))};
 assert.equal(visibleCandles(data,0).length,0);
 assert.equal(visibleCandles(data,2).length,2);
 assert.equal(visibleCandles(data,3).length,2);
 assert.equal(visibleCandles(data,14).length,3);
 assert.equal(visibleCandles(data,15).length,4);
});
