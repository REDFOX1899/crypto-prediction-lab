(function(root){
'use strict';
const ASSETS=['BTC','ETH','SOL'], STEP=900000;
const finite=n=>typeof n==='number'&&Number.isFinite(n);
function slots(boards,count,now=Date.now()){
 const end=Math.floor(now/STEP)*STEP, maps={};
 for(const asset of ASSETS){
  const map=new Map();
  for(const c of boards[asset]?.contracts||[]){
   const t=Date.parse(c.close_time),open=Date.parse(c.open_time);
   if(c.asset!==asset||!['yes','no'].includes(c.result)||!Number.isFinite(t)||t>end||t%STEP!==0||t-open!==STEP)continue;
   const old=map.get(t);
   if(!old||Date.parse(c.captured_at)>Date.parse(old.captured_at))map.set(t,c);
  }
  maps[asset]=map;
 }
 return Array.from({length:count},(_,i)=>{const at=end-(count-1-i)*STEP;return {at,contracts:Object.fromEntries(ASSETS.map(a=>[a,maps[a].get(at)||null]))}});
}
function transitions(rows,asset){
 const groups={yes:{wins:0,n:0},no:{wins:0,n:0}};
 for(let i=1;i<rows.length;i++){
   const previous=rows[i-1],current=rows[i],prior=previous.contracts[asset]?.result,next=current.contracts[asset]?.result;
   if(current.at-previous.at!==STEP||!['yes','no'].includes(prior)||!['yes','no'].includes(next))continue;
   groups[prior].n++;groups[prior].wins+=Number(next==='yes');
 }
 return groups;
}
function summary(rows){
 const perAsset=Object.fromEntries(ASSETS.map(a=>{const found=rows.map(r=>r.contracts[a]).filter(Boolean);return [a,{count:found.length,yes:found.filter(c=>c.result==='yes').length,candles:found.filter(c=>c.candle_count>0).length}]}));
 const joined=rows.filter(r=>ASSETS.every(a=>r.contracts[a]));
 return {perAsset,complete:joined.length,aligned:joined.filter(r=>new Set(ASSETS.map(a=>r.contracts[a].result)).size===1).length,total:ASSETS.reduce((n,a)=>n+perAsset[a].count,0)};
}
// Only synchronized observations enter comparisons. Never forward-fill another asset.
function contextRows(history,now=Date.now()){
 const map=new Map();
 for(const s of history||[]){const t=Date.parse(s.at);if(!Number.isFinite(t)||t>now)continue;
  const prices={},sourceTimes={};for(const a of ASSETS){const q=s.quotes?.find(q=>q.symbol===a),source=Date.parse(q?.source_at);if(finite(q?.price)&&q.price>0&&Number.isFinite(source)&&source<=t+120000&&t-source<=660000){prices[a]=q.price;sourceTimes[a]=source}}
  if(ASSETS.every(a=>prices[a]>0)&&Math.max(...Object.values(sourceTimes))-Math.min(...Object.values(sourceTimes))<=120000)map.set(t,{t,prices,sourceTimes});
 }
 return [...map.values()].sort((a,b)=>a.t-b.t);
}
function returns(points){
 const result=[];for(let i=1;i<points.length;i++){
  const a=points[i-1],b=points[i];
  if(b.t-a.t<240000||b.t-a.t>450000||ASSETS.some(s=>b.sourceTimes[s]<=a.sourceTimes[s]))continue;
  result.push({t:b.t,values:Object.fromEntries(ASSETS.map(s=>[s,(b.prices[s]/a.prices[s]-1)*100]))});
 }return result;
}
function correlation(rows,a,b){
 if(rows.length<6)return null;
 const mean=s=>rows.reduce((n,r)=>n+r.values[s],0)/rows.length,ma=mean(a),mb=mean(b);let cov=0,va=0,vb=0;
 for(const r of rows){const x=r.values[a]-ma,y=r.values[b]-mb;cov+=x*y;va+=x*x;vb+=y*y}
 return va>0&&vb>0?Math.max(-1,Math.min(1,cov/Math.sqrt(va*vb))):null;
}
function chart(points){
 if(points.length<2)return {paths:{},extent:0};
 const first=points[0],end=points.at(-1).t,values=points.flatMap(p=>ASSETS.map(a=>(p.prices[a]/first.prices[a]-1)*100)),extent=Math.max(.01,...values.map(Math.abs));
 const paths=Object.fromEntries(ASSETS.map(a=>[a,points.map((p,i)=>`${!i||p.t-points[i-1].t>450000?'M':'L'}${(48+(p.t-first.t)/Math.max(1,end-first.t)*804).toFixed(2)},${(125-(p.prices[a]/first.prices[a]-1)*100/extent*88).toFixed(2)}`).join(' ')]));
 return {paths,extent};
}
function candleRows(data,asset,ticker){
 const start=Date.parse(data?.open_time)/1000,end=Date.parse(data?.close_time)/1000;
 if(data?.status!=='available'||data.asset!==asset||data.ticker!==ticker||!Number.isInteger(start)||end-start!==900)return [];
 const map=new Map(),number=v=>(typeof v==='number'||typeof v==='string'&&v.trim()!=='')&&Number.isFinite(Number(v))?Number(v):NaN;
 for(const c of data.candles||[]){const values=['open','high','low','close'].map(k=>number(c[k]));const [o,h,l,cl]=values;
  if(!Number.isInteger(c.time)||c.time%60!==0||c.time<start||c.time+60>end||!values.every(finite)||l<0||h>1||l>o||o>h||l>cl||cl>h)continue;
  map.set(c.time,{time:c.time,open:o,high:h,low:l,close:cl});
 }
 return Array.from({length:15},(_,i)=>({time:start+i*60,candle:map.get(start+i*60)||null}));
}
// Provider bucket timestamps remain separate from retrieval time and Kalshi windows.
function blockworksRows(board,asset,now=Date.now()){
 const s=board?.snapshot,start=s?.start,end=s?.end;
 if(board?.status!=='available'||s?.schema_version!==1||s.provider!=='Blockworks'||s.model!=='asset-price'||s.granularity!=='5m'||!ASSETS.includes(asset)||!Number.isInteger(start)||!Number.isInteger(end)||start<0||end<=start||end-start>86400||start%300||end%300||!Number.isInteger(s.captured_at)||s.captured_at*1000>now+60000||end>s.captured_at-300)return [];
 const series=s.series?.filter(r=>r.asset===asset);if(series?.length!==1)return [];
 const map=new Map(),duplicates=new Set();
 for(const c of series[0].candles||[]){
  if(!Number.isInteger(c.time)||c.time%300||c.time<start||c.time>=end||!['open','high','low','close'].every(k=>finite(c[k])&&c[k]>0)||c.low>Math.min(c.open,c.close)||c.high<Math.max(c.open,c.close)||!(c.volume===null||finite(c.volume)&&c.volume>=0))continue;
  if(map.has(c.time))duplicates.add(c.time);map.set(c.time,c);
 }
 return Array.from({length:(end-start)/300},(_,i)=>{const t=start+i*300;return {time:t,candle:duplicates.has(t)?null:map.get(t)||null}});
}
function volumeRatio(rows){
 const recent=rows.slice(-13);if(recent.length!==13||recent.some(r=>!r.candle||!finite(r.candle.volume)))return null;
 const mean=recent.slice(0,12).reduce((n,r)=>n+r.candle.volume,0)/12;
 return mean>0?recent[12].candle.volume/mean:null;
}
function validBefore(data,contract,minute){
 const start=Date.parse(contract?.open_time)/1000,close=Date.parse(contract?.close_time)/1000;
 return data?.status==='available'&&data.asset===contract.asset&&data.ticker===contract.ticker&&data.minute===minute&&data.cutoff===start+minute*60&&close-start===900&&[0,5,10,14].includes(minute);
}
if(typeof module==='object'&&module.exports){module.exports={slots,summary,transitions,contextRows,returns,correlation,chart,candleRows,blockworksRows,volumeRatio,validBefore};return}
const $=id=>document.getElementById(id),pct=(n,d)=>d?`${(n/d*100).toFixed(1)}%`:'—';
const time=t=>new Date(t).toLocaleString([], {month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'});
let active=false,generation=0,timer,flight,count=48,boards={},context={},blockworks={},blockworksAsset='BTC',chosen=null,loadedAt=null,candleFlight=null,candleGeneration=0,selectedTicker=null;
const candleCache=new Map();
let beforeRow=null,beforeAsset='BTC',beforeMinute=10,beforeFlight=null,beforeGeneration=0;
const beforeCache=new Map();
function element(tag,text,cls){const e=document.createElement(tag);if(text!==undefined)e.textContent=text;if(cls)e.className=cls;return e}
function detail(row){
 if(chosen!==row.at){cancelCandles();selectedTicker=null}chosen=row.at;const host=$('lab-detail');host.replaceChildren(element('h3',`Window closed ${time(row.at)}`));
 const table=element('table');const head=element('tr');for(const h of ['Asset','Official result','Kalshi target','Settlement value','Minute candles'])head.append(element('th',h));table.append(head);
 for(const a of ASSETS){const c=row.contracts[a],tr=element('tr'),td=element('td'),b=element('button',`${a} · inspect quotes`,'lab-button');b.type='button';b.disabled=!c;b.addEventListener('click',()=>loadCandles(c));const replay=element('button','Replay','lab-button');replay.type='button';replay.disabled=!c;replay.addEventListener('click',()=>root.MarketReplay.open(c));td.append(b,replay);tr.append(td);for(const v of [c?c.result.toUpperCase():'Missing',c?.target||'—',c?.expiration_value||'—',c?String(c.candle_count):'—'])tr.append(element('td',v));table.append(tr)}
 host.append(table,element('p','Targets are source text; settlement values come from Kalshi. Candle counts describe available source records, not tick coverage. No inferred fills or forecasts.','lab-note'));
 const quotes=element('div');quotes.id='lab-quotes';host.append(quotes);
 const c=ASSETS.map(a=>row.contracts[a]).find(c=>c?.ticker===selectedTicker);if(c&&candleCache.has(c.ticker))drawCandles(c,candleCache.get(c.ticker));
}
function cancelCandles(){candleGeneration++;candleFlight?.abort();candleFlight=null}
function svgNode(tag,attributes){const e=document.createElementNS('http://www.w3.org/2000/svg',tag);for(const [k,v]of Object.entries(attributes))e.setAttribute(k,String(v));return e}
function drawCandles(contract,data){
 const host=$('lab-quotes');if(!host)return;host.replaceChildren();const rows=candleRows(data,contract.asset,contract.ticker),found=rows.filter(r=>r.candle);
 host.append(element('h3',`${contract.asset} · archived one-minute YES asks`));
 if(!found.length){host.append(element('p','No valid stored quote candles are available for this window.','lab-note'));return}
 const svg=svgNode('svg',{viewBox:'0 0 900 240',role:'img','aria-label':`${contract.asset} archived one-minute YES ask OHLC, ${found.length} of 15 minutes observed, 0 to 100 cents`});svg.classList.add('lab-quote-chart');
 for(const v of [0,.25,.5,.75,1]){const y=200-v*170;svg.append(svgNode('line',{x1:45,x2:875,y1:y,y2:y,stroke:'var(--border)'}));const text=svgNode('text',{x:0,y:y+4});text.textContent=`${Math.round(v*100)}¢`;svg.append(text)}
 rows.forEach((r,i)=>{const x=70+i*56,c=r.candle;if(!c){const mark=svgNode('text',{x,y:220,'text-anchor':'middle'});mark.textContent='·';svg.append(mark);return}const y=v=>200-v*170,color=c.close>=c.open?'var(--up)':'var(--down)';svg.append(svgNode('line',{x1:x,x2:x,y1:y(c.high),y2:y(c.low),stroke:color,'stroke-width':2}),svgNode('rect',{x:x-10,y:y(Math.max(c.open,c.close)),width:20,height:Math.max(1,Math.abs(c.close-c.open)*170),fill:color}));const title=svgNode('title',{});title.textContent=`${time(r.time*1000)}: open ${(c.open*100).toFixed(2)}¢, high ${(c.high*100).toFixed(2)}¢, low ${(c.low*100).toFixed(2)}¢, close ${(c.close*100).toFixed(2)}¢`;svg.lastChild.append(title)});
 host.append(svg,element('p',`${found.length}/15 minutes with valid quote OHLC. Green: ask closed at or above its open; red: below. These are historical quotes, not trades, fills or outcome probabilities. Missing minutes are left empty.`,'lab-note'));
 const details=element('details'),label=element('summary','Inspect the exact minute values');details.append(label);const table=element('table'),head=element('tr');for(const h of ['Minute start','Open ¢','High ¢','Low ¢','Close ¢'])head.append(element('th',h));table.append(head);
 for(const r of rows){const tr=element('tr');tr.append(element('td',time(r.time*1000)));for(const k of ['open','high','low','close'])tr.append(element('td',r.candle?(r.candle[k]*100).toFixed(2):'Missing'));table.append(tr)}details.append(table);host.append(details);
}
async function loadCandles(contract){
 cancelCandles();selectedTicker=contract.ticker;const cg=candleGeneration,g=generation,at=chosen,host=$('lab-quotes');
 if(candleCache.has(contract.ticker)){drawCandles(contract,candleCache.get(contract.ticker));return}
 host.textContent='Loading stored minute quotes…';const controller=new AbortController();candleFlight=controller;const deadline=setTimeout(()=>controller.abort(),12000);
 try{const r=await fetch(`/api/archive/window?asset=${encodeURIComponent(contract.asset)}&ticker=${encodeURIComponent(contract.ticker)}`,{signal:controller.signal});if(!r.ok)throw Error();const data=await r.json();if(!active||g!==generation||cg!==candleGeneration||chosen!==at)return;if(data.status!=='available')throw Error();if(candleCache.size>=32)candleCache.delete(candleCache.keys().next().value);candleCache.set(contract.ticker,data);drawCandles(contract,data)}
 catch{if(active&&g===generation&&cg===candleGeneration&&chosen===at)host.textContent='Archived minute quotes unavailable in this environment. Official outcome summaries remain above.'}
 finally{clearTimeout(deadline);if(candleFlight===controller)candleFlight=null}
}
function cancelBefore(){beforeGeneration++;beforeFlight?.abort();beforeFlight=null}
function beforeControls(){
 for(const b of document.querySelectorAll('[data-before-asset]')){b.disabled=!beforeRow?.contracts[b.dataset.beforeAsset];b.setAttribute('aria-pressed',String(b.dataset.beforeAsset===beforeAsset))}
 for(const b of document.querySelectorAll('[data-before-minute]')){b.disabled=!beforeRow;b.setAttribute('aria-pressed',String(Number(b.dataset.beforeMinute)===beforeMinute))}
}
function selectBefore(row){
 beforeRow=row;if(!row.contracts[beforeAsset])beforeAsset=ASSETS.find(a=>row.contracts[a])||'BTC';beforeControls();loadBefore();
}
function metricCard(label,value,note){const c=element('article');c.append(element('span',label),element('strong',value),element('small',note));return c}
function drawBefore(data,contract){
 const host=$('lab-before-detail');host.replaceChildren();
 $('lab-before-status').textContent=`${contract.asset} · ${contract.ticker} · cutoff ${time(data.cutoff*1000)} · ${15-beforeMinute} minutes to close`;
 const facts=element('div',undefined,'lab-before-facts');
 for(const [label,value,note]of [
  ['Kalshi target',data.target?.source_text||'Not recorded','Source text from settled archive; pre-cutoff observation unverified'],
  ['CF reference / distance to target','Not archived','Aggregate prices cannot replace the contract’s CF reference'],
  ['Historical order-book depth / spread','Not archived','No contemporaneous book was recorded']
 ]){const fact=element('article');fact.append(element('strong',label),element('p',value),element('small',note));facts.append(fact)}host.append(facts);
 const cmc=data.cmc||{};host.append(element('h3','CMC sample timestamps before cutoff'));
 host.append(element('p',cmc.status==='available'?`${cmc.price.toLocaleString('en-US',{maximumFractionDigits:4})} USD · source ${time(cmc.source_at*1000)} · collection ${time(cmc.collection_at*1000)} · ${cmc.age_seconds}s old. First receipt was not separately archived.`:'No fresh saved CMC sample has both timestamps at or before this cutoff.','lab-note'));
 const market=data.market_context||{};host.append(element('h3','Earlier market intervals · retrospective Blockworks history'));
 if(market.status==='available'){
  host.append(element('p',`${market.valid_candles}/${market.expected_candles} five-minute candles in the preceding hour · downloaded ${time(market.captured_at*1000)}. Original availability is unverified. Provider timestamp + five minutes must be at or before cutoff; bucket convention remains unverified.`,'lab-note'));
  const m=market.metrics||{},format=v=>finite(v)?`${v>=0?'+':''}${v.toFixed(3)}%`:'—',cards=element('div',undefined,'lab-before-metrics');
  for(const [label,value,note]of [
   ['Prior 5m move',format(m.return_5m_pct),'One complete candle'],['Prior 15m move',format(m.return_15m_pct),'Three consecutive candles'],['Prior 60m move',format(m.return_60m_pct),'Twelve consecutive candles'],
   ['Typical 5m move',finite(m.typical_5m_move_pct)?`${m.typical_5m_move_pct.toFixed(3)}%`:'—','Mean absolute candle move · preceding hour'],
   ['Relative 15m volume',finite(m.relative_volume_15m)?`${m.relative_volume_15m.toFixed(2)}×`:'—','Latest 15m / average of preceding three 15m blocks']
  ])cards.append(metricCard(label,value,note));host.append(cards);
  const details=element('details');details.append(element('summary','Inspect earlier price and volume intervals'));const wrap=element('div',undefined,'lab-source-wrap'),table=element('table',undefined,'lab-source-table'),head=element('tr');for(const h of ['Provider timestamp','Open · USD','High · USD','Low · USD','Close · USD',market.volume_unit==='usd'?'Volume · USD':'Volume · units unverified'])head.append(element('th',h));table.append(head);
  const end=market.end,map=new Map((market.candles||[]).map(c=>[c.time,c]));for(let t=end-3600;t<end;t+=300){const c=map.get(t),tr=element('tr');tr.append(element('td',time(t*1000)));for(const k of ['open','high','low','close','volume'])tr.append(element('td',c&&finite(c[k])?c[k].toLocaleString('en-US',{maximumFractionDigits:8}):'Missing'));table.append(tr)}wrap.append(table);details.append(wrap);host.append(details);
  const source=element('a','Source: Blockworks · Powered by Blockworks','lab-note');source.href='https://app.blockworks.com/';source.target='_blank';source.rel='noopener';host.append(source);
 }else host.append(element('p','No saved Blockworks pilot data covers this selection.','lab-note'));
 const quotes=data.contract_quotes?.candles||[];host.append(element('h3','Completed one-minute YES ask intervals'));
 host.append(element('p',`${quotes.length} completed quote candles inside the window end by this cutoff. These were retrieved retrospectively; they do not establish contemporaneous availability or fills.`,'lab-note'));
 if(quotes.length){const details=element('details');details.append(element('summary','Inspect earlier YES ask candles'));const table=element('table',undefined,'lab-source-table'),head=element('tr');for(const h of ['Minute start','Open ¢','High ¢','Low ¢','Close ¢'])head.append(element('th',h));table.append(head);for(const c of quotes){const tr=element('tr');tr.append(element('td',time(c.time*1000)));for(const k of ['open','high','low','close'])tr.append(element('td',(c[k]*100).toFixed(2)));table.append(tr)}const wrap=element('div',undefined,'lab-source-wrap');wrap.append(table);details.append(wrap);host.append(details)}
 host.append(element('p','Official results and settlement values stay in the outcome inspector above. They are excluded from these calculations. Missing data stays missing. This view is not a point-in-time training dataset.','lab-note'));
}
async function loadBefore(){
 cancelBefore();const contract=beforeRow?.contracts[beforeAsset],host=$('lab-before-detail');root.MarketNews?.cutoff(contract ? Math.floor(Date.parse(contract.open_time)/1000)+60*beforeMinute : null);host.replaceChildren();if(!contract){$('lab-before-status').textContent='No archived contract exists in the selected cell.';return}
 const key=`${contract.ticker}:${beforeMinute}`,minute=beforeMinute,bg=beforeGeneration,g=generation;
 if(beforeCache.has(key)){drawBefore(beforeCache.get(key),contract);return}
 $('lab-before-status').textContent=`Loading ${beforeAsset} evidence before the selected cutoff…`;
 const controller=new AbortController();beforeFlight=controller;const deadline=setTimeout(()=>controller.abort(),14000);
 try{const r=await fetch(`/api/research/before?asset=${contract.asset}&ticker=${encodeURIComponent(contract.ticker)}&minute=${minute}`,{signal:controller.signal});if(!r.ok)throw Error();const data=await r.json();if(!active||g!==generation||bg!==beforeGeneration)return;if(!validBefore(data,contract,minute))throw Error();if(beforeCache.size>=32)beforeCache.delete(beforeCache.keys().next().value);beforeCache.set(key,data);drawBefore(data,contract)}
 catch{if(active&&g===generation&&bg===beforeGeneration)$('lab-before-status').textContent='Evidence is temporarily unavailable for this cutoff. Select another completed window or retry.'}
 finally{clearTimeout(deadline);if(beforeFlight===controller)beforeFlight=null}
}
function renderBlockworks(){
 const rows=blockworksRows(blockworks,blockworksAsset),found=rows.filter(r=>r.candle),snapshot=blockworks.snapshot;
 const host=$('lab-blockworks-detail');host.replaceChildren();
 for(const b of document.querySelectorAll('[data-blockworks-asset]')){b.disabled=!rows.length;b.setAttribute('aria-pressed',String(b.dataset.blockworksAsset===blockworksAsset))}
 if(!rows.length){$('lab-blockworks-status').textContent='Blockworks history is not connected. No Blockworks market observations are available in this view.';return}
 const last=rows.at(-1)?.candle,ratio=volumeRatio(rows),volumeUnit=snapshot.volume_unit==='usd'?'USD':'provider units (unverified)';
 $('lab-blockworks-status').textContent=`Retrieved ${time(snapshot.captured_at*1000)} · provider timestamps ${time(snapshot.start*1000)} → ${time(snapshot.end*1000)} · ${found.length}/${rows.length} valid candles in the displayed day${blockworks.pilot?` · ${blockworks.pilot.valid_candles}/${blockworks.pilot.expected_candles} candles in the ${(blockworks.pilot.end-blockworks.pilot.start)/86400}-day pilot`:''}${Date.now()/1000-snapshot.captured_at>1800?' · older snapshot':''}`;
 const cards=element('div',undefined,'lab-pairs');
 for(const [label,value,note]of [
  ['Candle coverage',pct(found.length,rows.length),`${found.length} valid / ${rows.length} expected`],
  ['Latest candle move',last?`${((last.close/last.open-1)*100).toFixed(3)}%`:'—','Close / open · one five-minute candle'],
  ['Relative volume',ratio===null?'—':`${ratio.toFixed(2)}×`,'Latest / prior 12 contiguous candles']
 ]){const card=element('article');card.append(element('span',label),element('strong',value),element('small',note));cards.append(card)}host.append(cards);
 if(found.length){
  const min=Math.min(...found.map(r=>r.candle.low)),max=Math.max(...found.map(r=>r.candle.high)),span=Math.max(max-min,max*.00001);
  const svg=svgNode('svg',{viewBox:'0 0 900 210',role:'img','aria-label':`${blockworksAsset} five-minute Blockworks close prices in USD, ${found.length} of ${rows.length} candles. Gaps stay empty.`});svg.classList.add('lab-quote-chart');
  for(const [value,y]of [[max,25],[min,175]]){svg.append(svgNode('line',{x1:85,x2:880,y1:y,y2:y,stroke:'var(--border)'}));const label=svgNode('text',{x:0,y:y+4});label.textContent=value.toLocaleString('en-US',{maximumFractionDigits:4});svg.append(label)}
  let previous=false;const path=rows.map((r,i)=>{if(!r.candle){previous=false;return ''}const d=`${previous?'L':'M'}${(85+i/Math.max(1,rows.length-1)*795).toFixed(2)},${(25+(max-r.candle.close)/span*150).toFixed(2)}`;previous=true;return d}).join(' ');
  svg.append(svgNode('path',{d:path,fill:'none',stroke:'var(--accent)','stroke-width':2}));host.append(svg);
 }
 const details=element('details'),label=element('summary',`Inspect ${blockworksAsset} five-minute OHLC and volume`);details.append(label);
 const wrap=element('div',undefined,'lab-source-wrap'),table=element('table',undefined,'lab-source-table'),head=element('tr');for(const h of ['Provider timestamp','Open · USD','High · USD','Low · USD','Close · USD',`Volume · ${volumeUnit}`])head.append(element('th',h));table.append(head);
 for(const r of rows){const tr=element('tr');tr.append(element('td',time(r.time*1000)));for(const k of ['open','high','low','close','volume'])tr.append(element('td',r.candle&&finite(r.candle[k])?r.candle[k].toLocaleString('en-US',{maximumFractionDigits:8}):'Missing'));table.append(tr)}wrap.append(table);details.append(wrap);host.append(details);
 host.append(element('p',`Volume is per five-minute candle, in ${volumeUnit}. Missing volume is separate from zero. Relative volume requires 13 complete consecutive volume observations. Provider bucket timing and publication delay still need verification before aligning to contract windows. Retrospectively fetched candles are not point-in-time prediction inputs.`,'lab-note'));
}
function renderTransitions(rows){
 const host=$('lab-transitions');host.replaceChildren();
 for(const asset of ASSETS){
   const card=element('article'),counts=transitions(rows,asset);
   card.append(element('h3',asset));
   for(const side of ['yes','no']){
     const {wins,n}=counts[side],line=element('div',undefined,'paper-meter');
     line.append(element('span','After '+side.toUpperCase()),element('strong',pct(wins,n)+' · '+wins+'/'+n));
     const track=element('div',undefined,'paper-meter-track'),bar=element('i');
     bar.style.width=(n?100*wins/n:0)+'%';track.append(bar);line.append(track);card.append(line);
   }
   const gap=counts.yes.n && counts.no.n ? 100*(counts.yes.wins/counts.yes.n-counts.no.wins/counts.no.n) : null;
   card.append(element('p',gap===null?'Difference unavailable':('Difference '+(gap>0?'+':'')+gap.toFixed(1)+' percentage points')));host.append(card);
 }
}
function render(){
 const rows=slots(boards,count),stats=summary(rows),points=contextRows(context.history),changes=returns(points),plot=chart(points);
 root.TechnicalChart?.update(context.history||[]);
 $('lab-count').textContent=String(stats.total);$('lab-coverage').textContent=pct(stats.total,count*3);$('lab-aligned').textContent=pct(stats.aligned,stats.complete);$('lab-aligned-note').textContent=`${stats.aligned} / ${stats.complete} complete windows · same official direction`;
 $('lab-samples').textContent=String(points.length);$('lab-samples-note').textContent=`${changes.length} synchronized return pairs · CMC observations`;
 $('lab-status').textContent=loadedAt?`Last refresh ${time(loadedAt)} · ${count} completed slots · times in your timezone`:'Loading recorded observations…';
 const host=$('lab-ribbons');host.style.setProperty('--timeline-width',`${count*14+160}px`);host.replaceChildren();
 for(const a of ASSETS){const row=element('div',undefined,'lab-ribbon'),s=stats.perAsset[a],label=element('div',undefined,'lab-ribbon-label');label.append(element('strong',a),element('span',`${s.count}/${count} observed · ${pct(s.yes,s.count)} YES`));const cells=element('div',undefined,'lab-cells');cells.style.setProperty('--slots',String(count));
  for(const r of rows){const c=r.contracts[a],b=element('button',c?c.result==='yes'?'▲':'▼':'·','lab-cell');b.type='button';b.dataset.result=c?.result||'missing';b.setAttribute('aria-label',`${a}, closed ${time(r.at)}, ${c?c.result.toUpperCase():'missing archive'}`);b.setAttribute('aria-pressed',String(chosen===r.at));b.title=b.getAttribute('aria-label');b.addEventListener('click',()=>{detail(r);renderSelection();selectBefore(r)});cells.append(b)}
  row.append(label,cells);host.append(row);
 }
 const unavailable=ASSETS.filter(a=>boards[a]?.status!=='available');$('lab-archive-state').textContent=unavailable.length?`Archive unavailable for ${unavailable.join(', ')}. Gaps remain visible.`:`${stats.complete} windows have all three official results. ${count-stats.complete} are incomplete.`;
 if(chosen!==null){const r=rows.find(r=>r.at===chosen);if(r)detail(r);else{chosen=null;$('lab-detail').textContent='Select a cell to inspect that exact 15-minute window.'}}
 for(const a of ASSETS){$(`lab-path-${a}`).setAttribute('d',plot.paths[a]||'');const value=points.length>1?(points.at(-1).prices[a]/points[0].prices[a]-1)*100:null;$(`lab-change-${a}`).textContent=value===null?'—':`${value>=0?'+':''}${value.toFixed(3)}%`}
 $('lab-axis-top').textContent=`+${plot.extent.toFixed(2)}%`;$('lab-axis-bottom').textContent=`−${plot.extent.toFixed(2)}%`;
 $('lab-context-range').textContent=points.length>1?`${time(points[0].t)} → ${time(points.at(-1).t)} · rebased to first shared observation · gaps over 7.5m break the lines`:'Waiting for at least two shared, fresh CMC observations. No history is invented.';
 $('lab-chart-empty').hidden=points.length>1;
 const latest=points.at(-1);$('lab-context-age').textContent=latest?`Newest shared sample: ${time(latest.t)}${Date.now()-latest.t>660000?' · stale':''}`:'No shared sample available';
 const sampleHost=$('lab-context-table');sampleHost.replaceChildren();if(points.length){const details=element('details'),label=element('summary','Inspect recorded sample values');details.append(label);const table=element('table',undefined,'lab-source-table'),head=element('tr');for(const h of ['Collection time','BTC · USD','ETH · USD','SOL · USD'])head.append(element('th',h));table.append(head);for(const p of points){const row=element('tr');row.append(element('td',time(p.t)));for(const a of ASSETS){const td=element('td',p.prices[a].toLocaleString('en-US',{minimumFractionDigits:2,maximumFractionDigits:4}));td.append(element('small',`Source ${time(p.sourceTimes[a])}`));row.append(td)}table.append(row)}details.append(table);sampleHost.append(details)}
 const pairs=$('lab-pairs');pairs.replaceChildren();for(const [a,b]of [['BTC','ETH'],['BTC','SOL'],['ETH','SOL']]){const n=correlation(changes,a,b),card=element('article');card.append(element('span',`${a} / ${b}`),element('strong',n===null?'—':n.toFixed(2)),element('small',n===null?'Need 6 variable, synchronized return pairs':`Pearson r · ${changes.length} return pairs`));pairs.append(card)}
 const source=$('lab-sources');source.replaceChildren();for(const a of ASSETS){const s=stats.perAsset[a],at=boards[a]?.fetched_at;source.append(sourceRow(`Kalshi · ${a}`,`${s.count}/${count} official outcomes`,`${s.candles} contracts with minute candles`,at?`Fetched ${time(Date.parse(at))}`:'Archive not configured or unavailable'))}
 source.append(sourceRow('CoinMarketCap',`${points.length} shared samples`,'Aggregate market context',latest?`Sample ${time(latest.t)}`:'No stored samples'),sourceRow('Blockworks',blockworks.status==='available'?`${blockworksRows(blockworks,blockworksAsset).filter(r=>r.candle).length} valid ${blockworksAsset} candles`:'Not connected','Five-minute aggregate OHLCV',blockworks.snapshot?`Retrieved ${time(blockworks.snapshot.captured_at*1000)}`:'No snapshot available'));
 renderTransitions(rows);
 renderBlockworks();
 $('lab-export').disabled=!loadedAt||(!stats.total&&!points.length);
}
function sourceRow(...values){const tr=element('tr');for(const v of values)tr.append(element('td',v));return tr}
function renderSelection(){for(const b of $('lab-ribbons').querySelectorAll('button'))b.setAttribute('aria-pressed',String(b.getAttribute('aria-label').includes(time(chosen))))}
async function refresh(g){
 if(!active||g!==generation||flight)return;
 const f={controller:new AbortController()};flight=f;$('lab-refresh').disabled=true;
 const deadline=setTimeout(()=>f.controller.abort(),12000);
 try{
  const paths=[...ASSETS.map(a=>`/api/outcomes?asset=${a}`),'/api/context/history','/api/blockworks'];
  const results=await Promise.allSettled(paths.map(async p=>{const r=await fetch(p,{signal:f.controller.signal});if(!r.ok)throw Error('unavailable');return r.json()}));
  if(!active||g!==generation)return;
  ASSETS.forEach((a,i)=>{boards[a]=results[i].status==='fulfilled'?results[i].value:{status:'unavailable',contracts:[]}});
  context=results[3].status==='fulfilled'?results[3].value:{status:'unavailable',history:[]};blockworks=results[4].status==='fulfilled'?results[4].value:{status:'unavailable'};loadedAt=Date.now();render();
  if(results.some(r=>r.status==='rejected'))$('lab-status').textContent+=' · some sources unavailable';
 }finally{clearTimeout(deadline);if(flight===f)flight=null;if(active&&g===generation){$('lab-refresh').disabled=false;timer=setTimeout(()=>refresh(g),document.hidden?300000:60000)}}
}
function leave(){root.MarketNews?.leave();root.ForecastStudy?.leave();cancelBefore();active=false;generation++;clearTimeout(timer);flight?.controller.abort();flight=null;cancelCandles()}
for(const b of document.querySelectorAll('[data-lab-window]'))b.addEventListener('click',()=>{count=Number(b.dataset.labWindow);chosen=null;for(const p of document.querySelectorAll('[data-lab-window]'))p.setAttribute('aria-pressed',String(p===b));render()});
for(const b of document.querySelectorAll('[data-blockworks-asset]'))b.addEventListener('click',()=>{blockworksAsset=b.dataset.blockworksAsset;render()});
for(const b of document.querySelectorAll('[data-before-asset]'))b.addEventListener('click',()=>{beforeAsset=b.dataset.beforeAsset;beforeControls();loadBefore()});
for(const b of document.querySelectorAll('[data-before-minute]'))b.addEventListener('click',()=>{beforeMinute=Number(b.dataset.beforeMinute);beforeControls();loadBefore()});
$('lab-refresh').addEventListener('click',()=>{clearTimeout(timer);beforeCache.clear();if(beforeRow)loadBefore();refresh(generation)});
$('lab-export').addEventListener('click',()=>{const value={schema_version:1,exported_at:new Date().toISOString(),window_count:count,notes:'Official archived outcomes and CMC observations. Missing slots remain null. Not a training-ready point-in-time dataset. No Blockworks data.',windows:slots(boards,count),context};const url=URL.createObjectURL(new Blob([JSON.stringify(value,null,2)],{type:'application/json'}));const link=element('a');link.href=url;link.download=`crypto-data-lab-${new Date().toISOString().slice(0,10)}.json`;link.click();setTimeout(()=>URL.revokeObjectURL(url),1000)});
document.addEventListener('visibilitychange',()=>{if(active&&!document.hidden&&!flight){clearTimeout(timer);refresh(generation)}});
root.DataLab={enter(){leave();root.MarketNews?.enter();root.ForecastStudy?.enter();active=true;render();refresh(generation);if(beforeRow)loadBefore()},leave};
})(typeof window!=='undefined'?window:globalThis);
