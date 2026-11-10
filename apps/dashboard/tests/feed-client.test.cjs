const {test}=require('node:test');const assert=require('node:assert/strict');const fs=require('node:fs');const vm=require('node:vm');
test('three subscriptions share one request and closed listeners receive no updates',async()=>{
 const timers=new Map();let id=0,requests=0;const root={DASHBOARD_POLLING:true};
 const context={window:root,location:{href:'https://example.test/'},URL,JSON,Error,AbortController,document:{hidden:false,addEventListener(){}},setTimeout(fn){timers.set(++id,fn);return id},clearTimeout(id){timers.delete(id)},fetch:async()=>{requests++;return {ok:true,json:async()=>({feeds:{BTC:{market:{ticker:'BTC'},reference:{value:'5'}},ETH:{market:{ticker:'ETH'}}}})}}};
 vm.runInNewContext(fs.readFileSync(require.resolve('../public/feed-client.js'),'utf8'),context);
 const btc=new root.DashboardFeed('/api/live?asset=BTC'),eth=new root.DashboardFeed('/api/live?asset=ETH'),reference=new root.DashboardFeed('/api/reference?asset=BTC');const received=[];
 for(const feed of [btc,eth,reference])feed.onmessage=e=>received.push(JSON.parse(e.data));eth.close();
 assert.equal(timers.size,1);const [key,callback]=timers.entries().next().value;timers.delete(key);callback();await new Promise(setImmediate);
 assert.equal(requests,1);assert.deepEqual(received,[{ticker:'BTC'},{value:'5'}]);assert.equal(timers.size,1);btc.close();reference.close();assert.equal(timers.size,0);
});
function harness(fetcher){
 const timers=new Map(),events={},root={DASHBOARD_POLLING:true};let id=0,now=100000,requests=0;
 const document={hidden:false,addEventListener(name,fn){events[name]=fn}};
 class Clock extends Date{static now(){return now}}
 const context={window:root,location:{href:'https://example.test/'},URL,JSON,Error,AbortController,Date:Clock,document,setTimeout(fn,ms){timers.set(++id,{fn,ms});return id},clearTimeout(id){timers.delete(id)},fetch:async(...args)=>{requests++;return fetcher(...args)}};
 vm.runInNewContext(fs.readFileSync(require.resolve('../public/feed-client.js'),'utf8'),context);
 return {root,timers,events,document,setNow:n=>{now=n},get requests(){return requests},async run(delay){const entry=[...timers.entries()].find(([,t])=>t.ms===delay);assert.ok(entry,`Missing ${delay}ms timer`);timers.delete(entry[0]);entry[1].fn();await new Promise(setImmediate)}};
}
const response=value=>({ok:true,json:async()=>({feeds:{BTC:{market:{value}}}})});
test('steady polling counts request time and respects rate-limit cooldown after tab visibility changes',async()=>{
 let count=0,h;h=harness(async()=>{if(++count===1)return {ok:false,status:429,headers:{get:()=> '30'}};h.setNow(130500);return response('fresh')});
 const feed=new h.root.DashboardFeed('/api/live?asset=BTC');let errors=0;feed.onerror=()=>errors++;
 await h.run(0);assert.equal(errors,1);assert.equal(h.requests,1);
 assert.equal([...h.timers.values()][0].ms,30000);
 h.document.hidden=true;h.events.visibilitychange();h.document.hidden=false;h.events.visibilitychange();assert.equal([...h.timers.values()][0].ms,30000);
 h.setNow(130000);await h.run(30000);assert.equal([...h.timers.values()][0].ms,1500);feed.close();
});
test('closing a view aborts its request and late replies cannot reach a new view',async()=>{
 const pending=[];const h=harness(()=>new Promise(resolve=>pending.push(resolve)));
 const old=new h.root.DashboardFeed('/api/live?asset=BTC');let oldCalls=0,newCalls=[];old.onmessage=()=>oldCalls++;
 await h.run(0);old.close();const next=new h.root.DashboardFeed('/api/live?asset=BTC');next.onmessage=e=>newCalls.push(JSON.parse(e.data).value);
 await h.run(0);assert.equal(h.requests,2);
 pending[0](response('old'));await new Promise(setImmediate);assert.equal(oldCalls,0);assert.deepEqual(newCalls,[]);
 pending[1](response('new'));await new Promise(setImmediate);assert.deepEqual(newCalls,['new']);assert.equal(h.timers.size,1);next.close();assert.equal(h.timers.size,0);
});
test('a hung request times out and cannot prevent retries',async()=>{
 const h=harness((_,opts)=>new Promise((resolve,reject)=>opts.signal.addEventListener('abort',()=>reject(Error('aborted')))));
 const feed=new h.root.DashboardFeed('/api/live?asset=BTC');let errors=0;feed.onerror=()=>errors++;
 await h.run(0);h.setNow(108000);await h.run(8000);assert.equal(errors,1);assert.equal(h.timers.size,1);assert.equal([...h.timers.values()][0].ms,2000);feed.close();
});
test('one failing card cannot interrupt other subscribers and hidden tabs slow down',async()=>{
 const h=harness(async()=>response('live'));const broken=new h.root.DashboardFeed('/api/live?asset=BTC'),good=new h.root.DashboardFeed('/api/live?asset=BTC');let goodCalls=0;
 broken.onmessage=()=>{throw Error('render failed')};good.onmessage=()=>goodCalls++;
 h.document.hidden=true;await h.run(0);assert.equal(goodCalls,1);assert.equal(h.requests,1);assert.equal([...h.timers.values()][0].ms,15000);
 h.document.hidden=false;h.events.visibilitychange();assert.equal([...h.timers.values()][0].ms,0);broken.close();good.close();
});

test('all three market and reference channels still share one HTTP request',async()=>{
 const data={feeds:Object.fromEntries(['BTC','ETH','SOL'].map(asset=>[asset,{market:{asset,kind:'market'},reference:{asset,kind:'reference'}}]))};
 const h=harness(async()=>({ok:true,json:async()=>data})),seen=[];
 const feeds=['BTC','ETH','SOL'].flatMap(asset=>['live','reference'].map(channel=>{const f=new h.root.DashboardFeed(`/api/${channel}?asset=${asset}`);f.onmessage=e=>seen.push(JSON.parse(e.data));return f}));
 await h.run(0);assert.equal(h.requests,1);assert.equal(seen.length,6);assert.equal(new Set(seen.map(x=>x.asset+':'+x.kind)).size,6);feeds.forEach(f=>f.close());assert.equal(h.timers.size,0);
});
