const {test}=require('node:test');
const assert=require('node:assert/strict');
const {slots,summary,contextRows,returns,correlation,chart}=require('../public/data-lab.js');
const STEP=900000,now=STEP*100;
const contract=(asset,close,result='yes',extra={})=>({asset,result,open_time:new Date(close-STEP).toISOString(),close_time:new Date(close).toISOString(),captured_at:new Date(close+1000).toISOString(),candle_count:15,...extra});
test('aligned slots preserve missing windows and reject future, wrong-asset, void and non-15m records',()=>{
 const boards={BTC:{contracts:[contract('BTC',now),contract('ETH',now-STEP),contract('BTC',now+STEP),contract('BTC',now-2*STEP,'void'),contract('BTC',now-STEP,'yes',{open_time:new Date(now-STEP-60000).toISOString()})]}};
 const rows=slots(boards,3,now+10000);assert.equal(rows.length,3);assert.equal(rows[2].contracts.BTC.result,'yes');assert.equal(rows[1].contracts.BTC,null);assert.equal(rows[0].contracts.BTC,null);assert.equal(rows[2].contracts.ETH,null);
});
test('newer captured revisions replace older source summaries without counting twice',()=>{
 const yes=contract('BTC',now),no=contract('BTC',now,'no',{captured_at:new Date(now+2000).toISOString()});
 assert.equal(slots({BTC:{contracts:[no,yes]}},1,now)[0].contracts.BTC.result,'no');
});
test('agreement denominator includes only fully observed three-asset windows',()=>{
 const boards=Object.fromEntries(['BTC','ETH','SOL'].map(a=>[a,{contracts:[contract(a,now,a==='SOL'?'no':'yes'),contract(a,now-STEP)]}]));
 const s=summary(slots(boards,3,now));assert.equal(s.complete,2);assert.equal(s.aligned,1);assert.equal(s.total,6);assert.equal(s.perAsset.BTC.count,2);
 assert.equal(summary(slots({},3,now)).complete,0);
});
const sample=(t,prices=[100,200,50],extra={})=>({at:new Date(t).toISOString(),quotes:prices.map((price,i)=>({symbol:['BTC','ETH','SOL'][i],price,source_at:new Date(t).toISOString(),...extra}))});
test('context comparisons reject missing, invalid, stale, future and unsynchronized source observations',()=>{
 const data=[sample(now),sample(now-300000,[null,20,30]),sample(now-600000,[10,20,30],{source_at:new Date(now-2000000).toISOString()}),sample(now+300000),sample(now-900000,[10,20,30],{source_at:new Date(now).toISOString()})];
 const rows=contextRows(data,now);assert.equal(rows.length,1);assert.equal(rows[0].t,now);
});
test('returns exclude gaps, duplicate observations and stale source timestamps',()=>{
 const points=contextRows([sample(0),sample(300000,[101,202,51]),sample(600000,[101,202,51],{source_at:new Date(300000).toISOString()}),sample(1800000,[120,240,55])],1800000);
 assert.equal(returns(points).length,1);assert.ok(Math.abs(returns(points)[0].values.BTC-1)<1e-8);
 assert.equal(contextRows([sample(0),sample(0)],0).length,1);
});
test('shared comparisons reject sources more than two minutes apart',()=>{
 const row=sample(now);row.quotes[2].source_at=new Date(now-180000).toISOString();assert.equal(contextRows([row],now).length,0);
});
test('correlation uses return variation and refuses tiny or constant samples',()=>{
 const rows=Array.from({length:8},(_,i)=>({values:{BTC:i,ETH:2*i,SOL:-i}}));
 assert.equal(correlation(rows,'BTC','ETH'),1);assert.equal(correlation(rows,'BTC','SOL'),-1);assert.equal(correlation(rows.slice(0,5),'BTC','ETH'),null);
 assert.equal(correlation(rows.map(r=>({values:{...r.values,ETH:2}})),'BTC','ETH'),null);
});
test('normalized plot rebases shared observations and breaks across gaps',()=>{
 const points=contextRows([sample(0),sample(300000,[101,199,49]),sample(1800000,[102,198,48])],1800000),plot=chart(points);
 assert.equal((plot.paths.BTC.match(/M/g)||[]).length,2);assert.ok(plot.paths.BTC.startsWith('M48.00,125.00'));assert.equal(chart([]).extent,0);
});
test('data deep link preserves the established workspace routes',()=>{
 const {route}=require('../public/navigation.js');assert.equal(route('/?view=data').mode,'data');assert.equal(route('/?asset=SOL').mode,'market');assert.equal(route('/?view=combos').mode,'combos');
});
const fs=require('node:fs'),vm=require('node:vm');
test('archived minute quotes keep 15 explicit slots and reject invalid OHLC or asset identity',()=>{
 const {candleRows}=require('../public/data-lab.js'),start=1800000;
 const good={time:start,open:'0.4',high:'0.6',low:'0.3',close:'0.5'},data={status:'available',asset:'BTC',ticker:'KXBTC15M-X',open_time:new Date(start*1000).toISOString(),close_time:new Date((start+900)*1000).toISOString(),candles:[good,{...good,time:start+60,low:null},{...good,time:start+120,high:'0.1'},{...good,time:start-60},{...good,time:start+900}]};
 const rows=candleRows(data,'BTC',data.ticker);assert.equal(rows.length,15);assert.equal(rows.filter(r=>r.candle).length,1);assert.equal(rows[0].candle.close,.5);assert.equal(rows[1].candle,null);assert.deepEqual(candleRows(data,'ETH',data.ticker),[]);assert.deepEqual(candleRows(data,'BTC','wrong'),[]);
});
test('leaving the lab aborts all reads and late replies cannot repaint another workspace',async()=>{
 const nodes=new Map(),pending=[],signals=[],timers=new Map();let id=0;
 function node(){return {textContent:'',dataset:{},style:{setProperty(){}},children:[],setAttribute(){},getAttribute(){return ''},append(...c){this.children.push(...c)},replaceChildren(){this.children=[]},addEventListener(){},querySelectorAll(){return []}}}
 const doc={hidden:false,getElementById(k){if(!nodes.has(k))nodes.set(k,node());return nodes.get(k)},createElement:node,querySelectorAll:()=>[],addEventListener(){}};
 const window={};vm.runInNewContext(fs.readFileSync(require.resolve('../public/data-lab.js'),'utf8'),{window,document:doc,Date,AbortController,URL,Blob,fetch:(url,opts)=>{signals.push(opts.signal);return new Promise(resolve=>pending.push(()=>resolve({ok:true,json:async()=>({status:'available',contracts:[],history:[]})})))},setTimeout(fn,ms){timers.set(++id,{fn,ms});return id},clearTimeout(id){timers.delete(id)}});
 window.DataLab.enter();assert.equal(pending.length,4);window.DataLab.leave();assert.ok(signals.every(s=>s.aborted));pending.forEach(fn=>fn());await new Promise(setImmediate);assert.equal(nodes.get('lab-status').textContent,'Loading recorded observations…');assert.equal(timers.size,0);
});
