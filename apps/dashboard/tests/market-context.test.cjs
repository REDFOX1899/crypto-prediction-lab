const {test}=require('node:test');const assert=require('node:assert/strict');const {age,percent,trend}=require('../public/market-context.js');
test('invalid numbers and future source times remain unavailable',()=>{assert.equal(percent(null),'—');assert.equal(percent(-2.4),'-2.40%');assert.equal(age('invalid',Date.now()),Infinity);assert.equal(age(new Date(Date.now()+3600000).toISOString(),Date.now()),Infinity)});
test('sampled history preserves collection gaps and asset identity',()=>{const s=(t,v,a='BTC')=>({at:new Date(t).toISOString(),quotes:[{symbol:a,price:v}]});const p=trend([s(0,10),s(300000,11),s(1200000,9),s(1500000,50,'ETH')],'BTC');assert.equal(p.count,3);assert.equal((p.path.match(/M/g)||[]).length,2);assert.equal(trend([s(0,10)],'BTC').path,'')});
const vm=require('node:vm'),fs=require('node:fs');
test('leaving Home cancels context fetch and a late reply cannot repaint it',async()=>{
 const nodes=new Map();const doc={hidden:false,addEventListener(){},getElementById(id){if(!nodes.has(id))nodes.set(id,{textContent:'',dataset:{},setAttribute(){}});return nodes.get(id)}};
 let reply,signal;const w={};const ctx={window:w,document:doc,Intl,Date,Number,Math,JSON,AbortController,setTimeout,clearTimeout,fetch:(u,o)=>{assert.equal(u,'/api/context');signal=o.signal;return new Promise(r=>reply=r)}};
 vm.runInNewContext(fs.readFileSync(require.resolve('../public/market-context.js'),'utf8'),ctx);
 w.MarketContext.enter();assert.equal(nodes.get('context-price-BTC').textContent,'—');w.MarketContext.leave();assert.equal(signal.aborted,true);
 reply({ok:true,json:async()=>({status:'available',latest:{quotes:[{symbol:'BTC',price:12345,source_at:new Date().toISOString()}]},history:[]})});
 await new Promise(setImmediate);assert.equal(nodes.get('context-price-BTC').textContent,'—');
});
