(function(root){
'use strict';
const period=900000;
function windowClose(markets,now){
 const ms=['BTC','ETH','SOL'].map(a=>markets[a]);
 if(ms.some(m=>!m||m.status!=='active'||Date.parse(m.close_time)-Date.parse(m.open_time)!==period||now<Date.parse(m.open_time)||now>=Date.parse(m.close_time)))return null;
 const close=Date.parse(ms[0].close_time);return ms.every(m=>Date.parse(m.close_time)===close&&Date.parse(m.open_time)===Date.parse(ms[0].open_time))?close:null;
}
function recentOutcomes(records,now,count=12){
 const end=Math.floor(now/period)*period,map=new Map((records||[]).filter(c=>Date.parse(c.close_time)-Date.parse(c.open_time)===period&&['yes','no'].includes(c.result)).map(c=>[Date.parse(c.close_time),c.result]));
 return Array.from({length:count},(_,i)=>({close:end-(count-1-i)*period,result:map.get(end-(count-1-i)*period)||null}));
}
function depthShare(book){
 const total=side=>(side||[]).reduce((sum,[p,q])=>Number.isFinite(Number(p))&&Number(p)>0&&Number(p)<1&&Number.isFinite(Number(q))&&Number(q)>0?sum+Number(q):sum,0);
 const yes=total(book?.yes_dollars),no=total(book?.no_dollars);return yes+no>0?{yes:yes/(yes+no)*100,no:no/(yes+no)*100}:null;
}
function price(value){if(value==null||value==='')return null;const n=Number(value);return Number.isFinite(n)&&n>=0&&n<=1?n:null}
function topBook(book){const side=levels=>(levels||[]).filter(([p,q])=>price(p)!==null&&Number.isFinite(Number(q))&&Number(q)>0).slice().sort((a,b)=>Number(b[0])-Number(a[0])).slice(0,10);return {yes_dollars:side(book?.yes_dollars),no_dollars:side(book?.no_dollars)}}
function cardState(m,snapshot,live,received,now){
 const age=t=>{const ms=Date.parse(t);return Number.isFinite(ms)&&ms<=now+2000?Math.max(0,now-ms):Infinity};
 const marketValid=m?.status==='active'&&Date.parse(m.close_time)-Date.parse(m.open_time)===period&&now>=Date.parse(m.open_time)&&now<Date.parse(m.close_time);
 if(!marketValid)return {mode:'waiting',yes:null,no:null,depth:null,depthLive:false,age:null};
 const metadataFresh=age(snapshot?.fetched_at)<=45000;
 if(!metadataFresh)return {mode:'stale',yes:null,no:null,depth:null,depthLive:false,age:null};
 const matched=live?.ticker===m.ticker&&['live','subscribed'].includes(live.state)&&Number.isFinite(received)&&now-received<=8000;
 if(matched&&live.book&&age(live.book_received_at)<=15000){
  const book=topBook(live.book),yesBid=price(book.yes_dollars[0]?.[0]),noBid=price(book.no_dollars[0]?.[0]);
  return {mode:'live',yes:noBid===null?null:1-noBid,no:yesBid===null?null:1-yesBid,depth:depthShare(book),depthLive:true,age:age(live.book_received_at)};
 }
 const depth=age(snapshot?.fetched_at)<=45000?depthShare(topBook(snapshot?.order_book)):null;
 if(matched&&live.state==='live'&&age(live.received_at)<=15000)return {mode:'live',yes:price(live.yes_ask),no:price(live.yes_bid)===null?null:1-Number(live.yes_bid),depth,depthLive:false,age:age(live.received_at)};
 return {mode:'snapshot',yes:price(m.yes_ask_dollars),no:price(m.no_ask_dollars),depth,depthLive:false,age:age(snapshot?.fetched_at)};
}
if(typeof module==='object'&&module.exports){module.exports={windowClose,recentOutcomes,depthShare,cardState};return}
const $=id=>document.getElementById(id),symbols=['BTC','ETH','SOL'];let active=false,generation=0,timer,historyTimer,clockTimer,markets={},snapshots={},archived={},errors={},live={},received={},feeds=[],snapshotFlight=null,historyFlight=null,refs={},refReceived={},trails={},anchor=null;
const monotonic=()=>root.performance?.now?root.performance.now():Date.now();
function nowMs(){return anchor?anchor.server+Math.max(0,monotonic()-anchor.seen):Date.now()}
const moneyFormats={BTC:new Intl.NumberFormat('en-US',{style:'currency',currency:'USD',minimumFractionDigits:2,maximumFractionDigits:2}),ETH:new Intl.NumberFormat('en-US',{style:'currency',currency:'USD',minimumFractionDigits:2,maximumFractionDigits:2}),SOL:new Intl.NumberFormat('en-US',{style:'currency',currency:'USD',minimumFractionDigits:4,maximumFractionDigits:4})};
const money=(a,value)=>value==null?'—':moneyFormats[a].format(value);
const mmss=seconds=>seconds==null?'—:—':`${String(Math.floor(seconds/60)).padStart(2,'0')}:${String(seconds%60).padStart(2,'0')}`;
function referenceRender(a,now){
 const r=root.HomeVisuals.reference(markets[a],snapshots[a],refs[a],refReceived[a],now),state=$(`home-ref-state-${a}`);
 state.dataset.state=r.fresh?'live':r.value!==null?'stale':'waiting';
 text(`home-ref-state-${a}`,r.fresh?`${Math.floor(r.age/1000)}s · live`:r.value!==null?'Stale reference':refs[a]?.state==='disabled'?'Not configured':'Reference unavailable');
 text(`home-reference-${a}`,money(a,r.value));
 const direction=r.distance==null?'unknown':r.distance>=0?'up':'down';$(`home-distance-${a}`).dataset.direction=direction;
 text(`home-distance-${a}`,r.distance==null?'Distance unavailable · waiting for fresh data':`${r.distance>=0?'↑ +':'↓ −'}${money(a,Math.abs(r.distance))} · ${Math.abs(r.percent).toFixed(4)}% ${r.distance>=0?'above / at':'below'} target`);
 const graph=root.HomeVisuals.sparkline(trails[a],r.target,now),svg=$(`home-spark-${a}`),target=$(`home-spark-target-${a}`),dot=$(`home-spark-dot-${a}`);
 $(`home-spark-path-${a}`).setAttribute('d',graph.path);svg.dataset.stale=String(!r.fresh);
 if(graph.targetY!==null){target.setAttribute('y1',graph.targetY);target.setAttribute('y2',graph.targetY)}target.setAttribute('visibility',graph.targetY===null?'hidden':'visible');
 if(graph.last){dot.setAttribute('cx',graph.last.x);dot.setAttribute('cy',graph.last.y)}dot.setAttribute('visibility',graph.last&&r.fresh?'visible':'hidden');
 svg.setAttribute('aria-label',`${a}: ${graph.count} observed reference values in the last two minutes; ${r.distance==null?'current distance unavailable':`${money(a,Math.abs(r.distance))} ${r.distance>=0?'at or above':'below'} target`}. Gaps are preserved.`);
 text(`home-spark-note-${a}`,graph.count?'Observed · 2m · auto scale':'Waiting for observations');
 const phase=$(`home-phase-${a}`);phase.dataset.phase=r.phase;
 text(`home-phase-label-${a}`,r.phase==='final'?'Final minute · closes in':r.phase==='closed'?'Window closed':r.phase==='waiting'?'Waiting for active window':'Final minute starts in');
 text(`home-phase-clock-${a}`,mmss(r.phase==='final'?r.seconds:r.startsIn));
 $(`home-phase-progress-${a}`).setAttribute('width',Math.min(300,Math.max(0,r.progress*300)).toFixed(2));
 $(`home-phase-graphic-${a}`).setAttribute('aria-label',r.phase==='final'?`Final minute; ${r.seconds} seconds to close`:r.startsIn===null?'Waiting for current market':`${r.startsIn} seconds until the final minute. ${Math.round(r.progress*100)} percent of the window elapsed.`);
}
const reduceMotion=root.matchMedia?.('(prefers-reduced-motion: reduce)');
const cents=n=>n==null?'—':`${(n*100).toLocaleString(undefined,{maximumFractionDigits:2})}¢`;
const time=t=>new Date(t).toLocaleTimeString([],{hour:'numeric',minute:'2-digit'});
function text(id,value){const node=$(id);if(node.textContent!==value)node.textContent=value}
function quote(id,value){const node=$(id),next=cents(value),previous=node.dataset.price;if(node.textContent===next)return;text(id,next);if(previous!==undefined&&value!==null&&Number.isFinite(Number(previous))&&Number(previous)!==value&&!reduceMotion?.matches)node.animate?.([{backgroundColor:value>Number(previous)?'var(--up-soft)':'var(--down-soft)'},{backgroundColor:'transparent'}],{duration:650});if(value===null)delete node.dataset.price;else node.dataset.price=String(value)}
function render(a,now){
 referenceRender(a,now);
 const m=markets[a],state=cardState(m,snapshots[a],live[a],received[a],now),closed=m&&now>=Date.parse(m.close_time);
 quote(`home-yes-${a}`,state.yes);quote(`home-no-${a}`,state.no);
 const badge=$(`home-live-${a}`);badge.dataset.state=document.hidden?'background':state.mode;
 text(`home-live-${a}`,document.hidden?'BACKGROUND':state.mode==='live'?'LIVE':state.mode==='snapshot'?'SNAPSHOT':state.mode==='stale'?'STALE':'CONNECTING');
 text(`home-quote-time-${a}`,state.mode==='live'?`Quote updated ${Math.floor(state.age/1000)}s ago`:state.mode==='snapshot'?`Snapshot · ${Math.floor(state.age/1000)}s ago`:closed?'Window ended · finding next market':state.mode==='stale'?'Market details stale · retrying':'Waiting for current quotes');
 const depth=$(`home-depth-${a}`),share=state.depth;depth.hidden=!share;if(share)depth.style.setProperty('--yes-share',`${share.yes}%`);
 text(`home-depth-label-${a}`,share?`${state.depthLive?'Live':'Snapshot'} top 10 bid size · YES ${Math.round(share.yes)}% / NO ${Math.round(share.no)}%`:'Order-book depth unavailable');
 const checked=snapshots[a]?.fetched_at;const age=checked?Math.max(0,Math.floor((now-Date.parse(checked))/1000)):null;
 text(`home-source-${a}`,errors[a]?'Window refresh failed · retrying':age===null?'Finding current contract':`Window checked ${age}s ago · results update after settlement`);return state.mode;
}
function clock(){
 const now=nowMs(),close=windowClose(markets,now),fresh=symbols.every(a=>snapshots[a]?.fetched_at&&now-Date.parse(snapshots[a].fetched_at)<45000);
 const seconds=close&&fresh?Math.max(0,Math.ceil((close-now)/1000)):null;
 text('home-countdown',seconds===null?'—:—':`${String(Math.floor(seconds/60)).padStart(2,'0')}:${String(seconds%60).padStart(2,'0')}`);
 text('home-window-caption',close&&fresh?`Until the shared ${time(close)} close`:'Waiting for aligned market windows');
 $('home-dial-progress').setAttribute('stroke-dasharray',`${close&&fresh?Math.max(0,Math.min(100,(1-(close-now)/900000)*100)):0} 100`);
 text('home-dial-label',close&&fresh?(close-now<=60000?'FINAL MINUTE':`Final minute in ${mmss(Math.ceil((close-now-60000)/1000))}`):'Waiting for current markets');
 const modes=symbols.map(a=>render(a,now)),anyLive=modes.includes('live');
 text('home-refresh-note',document.hidden?'Background updates slowed':anyLive?(root.DASHBOARD_POLLING?'Live quotes · updates every 2s':'Live streamed quotes'):modes.includes('snapshot')?'Snapshot fallback · live feed unavailable':'Connecting to shared live feed');
}
function history(){for(const a of symbols){
 const host=$(`home-history-${a}`),slots=recentOutcomes((archived[a]||[]).filter(c=>c.asset===a),nowMs()),key=JSON.stringify(slots);if(host.dataset.slots===key)continue;host.dataset.slots=key;
 host.replaceChildren();for(const slot of slots){const cell=document.createElement('span');cell.className=`home-outcome ${slot.result||'missing'}`;cell.textContent=slot.result==='yes'?'▲':slot.result==='no'?'▼':'·';cell.title=`${new Date(slot.close).toLocaleString()} · ${slot.result==='yes'?'YES / at or above target':slot.result==='no'?'NO / below target':'not archived'}`;cell.setAttribute('aria-label',cell.title);host.append(cell)}
}}
function nextRefresh(){
 const remaining=Object.values(markets).map(m=>Date.parse(m?.close_time)-nowMs()).filter(ms=>Number.isFinite(ms)&&ms>0);
 return Math.max(1000,Math.min(document.hidden?45000:30000,...remaining.map(ms=>ms+150)));
}
async function refresh(g){
 clearTimeout(timer);if(snapshotFlight||!active||g!==generation)return;
 const flight={controller:new AbortController()};snapshotFlight=flight;const deadline=setTimeout(()=>flight.controller.abort(),12000);
 await Promise.allSettled(symbols.map(async a=>{
  try{const r=await fetch(`/api/snapshot?asset=${a}`,{cache:'no-store',signal:flight.controller.signal});if(!r.ok)throw Error();const data=await r.json();if(!active||g!==generation)return;
   const m=data.market;if(markets[a]?.ticker!==m?.ticker){delete live[a];delete received[a];trails[a]=[]}
   markets[a]=m;snapshots[a]=data;errors[a]=false;
   text(`home-target-${a}`,m?.yes_sub_title?.replace(/^target price:\s*/i,'')||'No active target');
   text(`home-window-${a}`,m?`${time(m.open_time)}–${time(m.close_time)} · target fixed for this window`:'Waiting for an active window');
  }catch{if(active&&g===generation)errors[a]=true;}
 }));clearTimeout(deadline);if(snapshotFlight===flight)snapshotFlight=null;
 if(!active||g!==generation)return;clock();timer=setTimeout(()=>refresh(g),symbols.some(a=>errors[a]||!markets[a])?5000:nextRefresh());
}
async function loadHistory(g){
 if(historyFlight||!active||g!==generation)return;const flight={controller:new AbortController()};historyFlight=flight;const deadline=setTimeout(()=>flight.controller.abort(),12000);
 await Promise.allSettled(symbols.map(async a=>{
  try{const r=await fetch(`/api/outcomes?asset=${a}`,{signal:flight.controller.signal});if(!r.ok)throw Error();const data=await r.json();if(!active||g!==generation)return;archived[a]=data.contracts||[];text(`home-history-state-${a}`,data.status==='available'?'Last 12 completed windows':'Archive unavailable');}
  catch{if(active&&g===generation)text(`home-history-state-${a}`,'Archive unavailable');}
 }));clearTimeout(deadline);if(historyFlight===flight)historyFlight=null;
 if(!active||g!==generation)return;history();historyTimer=setTimeout(()=>loadHistory(g),document.hidden?180000:60000);
}
function connect(g){for(const a of symbols){
 const feed=new root.DashboardFeed(`/api/live?asset=${a}`);feeds.push(feed);
 feed.onmessage=event=>{if(!active||g!==generation)return;try{live[a]=JSON.parse(event.data);received[a]=nowMs();render(a,nowMs())}catch{delete live[a];render(a,nowMs())}};
 feed.onerror=()=>{if(!active||g!==generation)return;delete live[a];delete received[a];render(a,nowMs())};
 const reference=new root.DashboardFeed(`/api/reference?asset=${a}`);feeds.push(reference);
 reference.onmessage=event=>{if(!active||g!==generation)return;try{
  const u=JSON.parse(event.data),server=u.final_minute?.server_time_ms;
  if(Number.isFinite(server)&&(!anchor||server>=anchor.server))anchor={server,seen:monotonic()};
  refs[a]=u;refReceived[a]=nowMs();const r=root.HomeVisuals.reference(markets[a],snapshots[a],u,refReceived[a],nowMs());
  if(r.fresh){const points=trails[a]||[];if(!points.length||r.source>points.at(-1).t)points.push({t:r.source,v:r.value});trails[a]=points.filter(p=>p.t>=nowMs()-120000).slice(-120)}
  referenceRender(a,nowMs());
 }catch{delete refs[a];referenceRender(a,nowMs())}};
 reference.onerror=()=>{if(!active||g!==generation)return;delete refs[a];referenceRender(a,nowMs())};
}}

function leave(){root.MarketContext?.leave();active=false;generation++;clearTimeout(timer);clearTimeout(historyTimer);clearInterval(clockTimer);feeds.forEach(f=>f.close());feeds=[];snapshotFlight?.controller.abort();historyFlight?.controller.abort();snapshotFlight=null;historyFlight=null}
root.HomePage={enter(){leave();active=true;markets={};snapshots={};archived={};errors={};live={};received={};refs={};refReceived={};trails={};anchor=null;for(const a of symbols){text(`home-target-${a}`,'—');delete $(`home-yes-${a}`).dataset.price;delete $(`home-no-${a}`).dataset.price;text(`home-window-${a}`,'Finding the current market…')}const g=generation;root.MarketContext?.enter();clock();history();connect(g);refresh(g);loadHistory(g);clockTimer=setInterval(clock,1000)},leave};
document.addEventListener('visibilitychange',()=>{if(active&&!document.hidden){refresh(generation);if(!historyFlight){clearTimeout(historyTimer);loadHistory(generation)}}});
})(typeof window!=='undefined'?window:globalThis);
