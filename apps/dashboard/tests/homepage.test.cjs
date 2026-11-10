const {test}=require('node:test'),assert=require('node:assert/strict');
const {windowClose,recentOutcomes,depthShare}=require('../public/homepage.js');
const {route}=require('../public/navigation.js');
test('homepage is default and existing market/combo deep links remain valid',()=>{
 assert.deepEqual(route('/'),{mode:'home',asset:'BTC'});
 assert.deepEqual(route('/?asset=SOL'),{mode:'market',asset:'SOL'});
 assert.deepEqual(route('/?view=market&asset=ETH'),{mode:'market',asset:'ETH'});
 assert.equal(route('/?view=combos&asset=SOL').mode,'combos');
 assert.equal(route('/?view=home&asset=ETH').mode,'home');
 assert.deepEqual(route('/?view=anything&asset=BAD'),{mode:'home',asset:'BTC'});
});
test('home clock requires three matching active 15-minute windows',()=>{
 const open='2026-11-09T15:00:00Z',close='2026-11-09T15:15:00Z',now=Date.parse('2026-11-09T15:03:00Z');
 const m={status:'active',open_time:open,close_time:close},ms={BTC:m,ETH:m,SOL:m};
 assert.equal(windowClose(ms,now),Date.parse(close));
 assert.equal(windowClose({...ms,SOL:null},now),null);
 assert.equal(windowClose({...ms,ETH:{...m,close_time:'2026-11-09T15:30:00Z'}},now),null);
 assert.equal(windowClose(ms,Date.parse(close)),null);
});
test('home history preserves missing slots and does not turn unknown results into losses',()=>{
 const now=Date.parse('2026-11-09T15:30:00Z');
 const rows=recentOutcomes([{open_time:'2026-11-09T15:00:00Z',close_time:'2026-11-09T15:15:00Z',result:'yes'},{open_time:'2026-11-09T15:15:00Z',close_time:'2026-11-09T15:30:00Z',result:'void'}],now,3);
 assert.deepEqual(rows.map(r=>r.result),[null,'yes',null]);
});
test('visible bid-size share ignores invalid levels and empty books',()=>{
 assert.deepEqual(depthShare({yes_dollars:[['0.4','3'],['0','100'],['0.3','-20']],no_dollars:[['0.5','1']]}),{yes:75,no:25});
 assert.equal(depthShare({yes_dollars:[],no_dollars:[]}),null);
});
const {cardState}=require('../public/homepage.js');
const now=Date.parse('2026-11-09T20:03:00Z');
const market={ticker:'KXBTC15M-TEST',status:'active',open_time:'2026-11-09T20:00:00Z',close_time:'2026-11-09T20:15:00Z',yes_ask_dollars:'0.5',no_ask_dollars:'0.51'};
const snapshot={fetched_at:new Date(now-1000).toISOString(),order_book:{yes_dollars:[['0.49','10']],no_dollars:[['0.5','10']]}};
const live={ticker:market.ticker,state:'live',received_at:new Date(now-100).toISOString(),yes_bid:'0.7',yes_ask:'0.71',book_received_at:new Date(now-50).toISOString(),book:{yes_dollars:[['0.6','3'],['0.7','2']],no_dollars:[['0.29','1']]}};
test('home uses fresh matching book asks and visible size instead of overwriting them with snapshots',()=>{
 const s=cardState(market,snapshot,live,now,now);
 assert.equal(s.mode,'live');assert.equal(s.yes,0.71);assert.ok(Math.abs(s.no-0.3)<1e-10);assert.equal(s.depthLive,true);assert.equal(s.depth.yes,5/6*100);
 const empty=cardState(market,snapshot,{...live,book:{yes_dollars:[],no_dollars:[]}},now,now);
 assert.equal(empty.yes,null);assert.equal(empty.no,null);assert.equal(empty.depth,null);
});
test('wrong ticker, stale transport, reconnecting and expired source never claim live prices',()=>{
 for(const update of [{...live,ticker:'OTHER'},{...live,state:'reconnecting'},{...live,book_received_at:new Date(now-20000).toISOString(),received_at:new Date(now-20000).toISOString()}]){
  const s=cardState(market,snapshot,update,now,now);assert.equal(s.mode,'snapshot');assert.equal(s.yes,0.5);
 }
 assert.equal(cardState(market,snapshot,live,now-9000,now).mode,'snapshot');
 assert.equal(cardState(market,{...snapshot,fetched_at:new Date(now-46000).toISOString()},live,now,now).mode,'stale');
 assert.equal(cardState(market,snapshot,live,now,Date.parse(market.close_time)).mode,'waiting');
});
test('ticker fallback preserves missing asks and validates numeric quotes',()=>{
 assert.equal(cardState(market,snapshot,{...live,book:null,yes_ask:null,yes_bid:null},now,now).yes,null);
 assert.equal(cardState(market,snapshot,{...live,book:null,yes_ask:'nope',yes_bid:'1.1'},now,now).no,null);
});
test('homepage shares subscriptions, preserves live quotes through metadata refresh and cleans up on leave',async()=>{
 const fs=require('node:fs'),vm=require('node:vm'),nodes=new Map(),timers=new Map(),intervals=new Map(),subscriptions=[],calls=[];let id=0;
 function node(){return {textContent:'',dataset:{},hidden:false,style:{setProperty(){}},children:[],setAttribute(){},replaceChildren(){this.children=[]},append(c){this.children.push(c)},animate(){}}}
 const document={hidden:false,getElementById(id){if(!nodes.has(id))nodes.set(id,node());return nodes.get(id)},createElement:node,addEventListener(){}};
 const root={HomeVisuals:require('../public/home-visuals.js'),DASHBOARD_POLLING:true,matchMedia:()=>({matches:false}),DashboardFeed:class{constructor(url){this.url=url;subscriptions.push(this)}close(){this.closed=true}}};
 class TestDate extends Date{static now(){return now}}
 const context={window:root,document,Date:TestDate,AbortController,JSON,URL,Error,fetch:async(url,opts)=>{calls.push({url,signal:opts.signal});const asset=new URL(url,'https://test/').searchParams.get('asset');return {ok:true,json:async()=>url.includes('outcomes')?{status:'available',contracts:[]}:{...snapshot,market:{...market,floor_strike:'100',strike_type:'greater_or_equal',ticker:`KX${asset}15M-TEST`}}}},setTimeout(fn,ms){timers.set(++id,{fn,ms});return id},clearTimeout(id){timers.delete(id)},setInterval(fn,ms){intervals.set(++id,{fn,ms});return id},clearInterval(id){intervals.delete(id)}};
 vm.runInNewContext(fs.readFileSync(require.resolve('../public/homepage.js'),'utf8'),context);
 root.HomePage.enter();await new Promise(setImmediate);
 assert.equal(subscriptions.length,6);assert.equal(calls.length,6);assert.equal(calls.filter(c=>c.url.includes('/api/outcomes')&&c.url.endsWith('&view=recent')).length,3);assert.ok(!calls.some(c=>c.url==='/api/feeds'));
 subscriptions[0].onmessage({data:JSON.stringify(live)});
 assert.equal(nodes.get('home-live-BTC').textContent,'LIVE');assert.equal(nodes.get('home-yes-BTC').textContent,'71¢');
 subscriptions[1].onmessage({data:JSON.stringify({state:'live',value:'102',source_time_ms:now,final_minute:{server_time_ms:now}})});
 assert.equal(nodes.get('home-reference-BTC').textContent,'$102.00');assert.ok(nodes.get('home-distance-BTC').textContent.includes('+$2.00'));assert.equal(nodes.get('home-phase-clock-BTC').textContent,'11:00');
 assert.equal(nodes.get('home-reference-ETH').textContent,'—');
 const refresh=[...timers.values()].find(t=>t.ms===30000);assert.ok(refresh);await refresh.fn();
 assert.equal(nodes.get('home-yes-BTC').textContent,'71¢');
 root.HomePage.leave();assert.ok(subscriptions.every(f=>f.closed));assert.equal(intervals.size,0);
 const before=nodes.get('home-yes-BTC').textContent;subscriptions[0].onmessage({data:JSON.stringify({...live,book:null,yes_ask:'0.9'})});assert.equal(nodes.get('home-yes-BTC').textContent,before);
 assert.ok(calls.every(c=>c.signal instanceof AbortSignal));
});
test('leaving Home aborts pending metadata and history requests; old replies cannot repaint it',async()=>{
 const fs=require('node:fs'),vm=require('node:vm'),nodes=new Map(),pending=[],signals=[];let id=0;
 function node(){return {textContent:'',dataset:{},style:{setProperty(){}},setAttribute(){},replaceChildren(){},append(){}}}
 const root={HomeVisuals:require('../public/home-visuals.js'),DashboardFeed:class{close(){}}},document={hidden:false,getElementById(id){if(!nodes.has(id))nodes.set(id,node());return nodes.get(id)},createElement:node,addEventListener(){}};
 vm.runInNewContext(fs.readFileSync(require.resolve('../public/homepage.js'),'utf8'),{window:root,document,AbortController,Date,Error,JSON,setTimeout:()=>++id,clearTimeout(){},setInterval:()=>++id,clearInterval(){},fetch:(url,opts)=>{signals.push(opts.signal);return new Promise(resolve=>pending.push(()=>resolve({ok:true,json:async()=>url.includes('outcomes')?{contracts:[]}:{...snapshot,market}})))}});
 root.HomePage.enter();assert.equal(signals.length,6);root.HomePage.leave();assert.ok(signals.every(s=>s.aborted));
 pending.forEach(resolve=>resolve());await new Promise(setImmediate);assert.equal(nodes.get('home-target-BTC').textContent,'—');
});
test('spread uses matching quote sides and keeps locked books distinct from missing or crossed quotes',()=>{
 const update = {...live,book:null,yes_bid:'0.451',yes_ask:'0.467'};
 assert.equal(cardState(market,snapshot,update,now,now).spread,0.016);
 assert.equal(cardState(market,snapshot,{...update,yes_ask:'0.450'},now,now).spread,null);
 assert.equal(cardState(market,snapshot,{...update,yes_ask:null},now,now).spread,null);
 const locked={...live,book:{yes_dollars:[['0.10','1']],no_dollars:[['0.90','1']]}};
 assert.equal(cardState(market,snapshot,locked,now,now).spread,0);
 assert.equal(cardState({...market,yes_bid_dollars:'0.45'},snapshot,null,null,now).spread,0.05);
});
