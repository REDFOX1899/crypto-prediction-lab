const test=require('node:test'), assert=require('node:assert/strict');
const {samples,indicators}=require('../public/technical-chart.js');
const start=Date.parse('2026-11-10T00:00:00Z'), step=300000;
const sample=(i,price,source=i)=>({at:new Date(start+i*step).toISOString(),quotes:[{symbol:'BTC',price,source_at:new Date(start+source*step).toISOString()}]});
const series=(n)=>Array.from({length:n},(_,i)=>sample(i,100+i));
test('warming, rolling band, simple RSI and log volatility use earlier observations only',()=>{
  const rows=indicators(samples(series(23),'BTC',Infinity));
  assert.equal(rows[13].rsi,null);assert.equal(rows[14].rsi,100);
  assert.equal(rows[18].mean,null);assert.equal(rows[19].mean,109.5);
  assert.ok(Math.abs(rows[19].upper-(109.5+2*Math.sqrt(33.25)))<1e-9);
  const sum=Array.from({length:14},(_,i)=>Math.log((101+i)/(100+i))**2).reduce((a,b)=>a+b);
  assert.ok(Math.abs(rows[14].volatility-Math.sqrt(sum)*100)<1e-9);
  assert.deepEqual(rows.slice(0,20),indicators(samples(series(20),'BTC',Infinity)));
});
test('gaps, future sources, invalid observations and duplicated publications do not create returns',()=>{
  const list=[...series(21),sample(21,120,20),sample(25,124),sample(26,125,27),sample(27,126)];
  const out=indicators(samples(list.reverse(),'BTC',start+27*step));
  assert.equal(out.length,23);
  assert.equal(out.at(-2).rsi,null);assert.equal(out.at(-1).rsi,null);
  assert.notEqual(out.at(-1).segment,out.at(-2).segment);
});
test('flat or missing asset observations never fabricate momentum',()=>{
  const flat=series(20).map((_,i)=>sample(i,100));
  assert.equal(indicators(samples(flat,'BTC',Infinity)).at(-1).rsi,null);
  assert.equal(indicators(samples(flat,'BTC',Infinity)).at(-1).volatility,0);
  assert.deepEqual(samples(flat,'SOL',Infinity),[]);
});
