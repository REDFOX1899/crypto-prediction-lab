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
if(typeof module==='object'&&module.exports){module.exports={slots,summary,contextRows,returns,correlation,chart,candleRows};return}
const $=id=>document.getElementById(id),pct=(n,d)=>d?`${(n/d*100).toFixed(1)}%`:'—';
const time=t=>new Date(t).toLocaleString([], {month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'});
let active=false,generation=0,timer,flight,count=48,boards={},context={},chosen=null,loadedAt=null,candleFlight=null,candleGeneration=0,selectedTicker=null;
const candleCache=new Map();
function element(tag,text,cls){const e=document.createElement(tag);if(text!==undefined)e.textContent=text;if(cls)e.className=cls;return e}
function detail(row){
 if(chosen!==row.at){cancelCandles();selectedTicker=null}chosen=row.at;const host=$('lab-detail');host.replaceChildren(element('h3',`Window closed ${time(row.at)}`));
 const table=element('table');const head=element('tr');for(const h of ['Asset','Official result','Kalshi target','Settlement value','Minute candles'])head.append(element('th',h));table.append(head);
 for(const a of ASSETS){const c=row.contracts[a],tr=element('tr'),td=element('td'),b=element('button',`${a} · inspect quotes`,'lab-button');b.type='button';b.disabled=!c;b.addEventListener('click',()=>loadCandles(c));td.append(b);tr.append(td);for(const v of [c?c.result.toUpperCase():'Missing',c?.target||'—',c?.expiration_value||'—',c?String(c.candle_count):'—'])tr.append(element('td',v));table.append(tr)}
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
function render(){
 const rows=slots(boards,count),stats=summary(rows),points=contextRows(context.history),changes=returns(points),plot=chart(points);
 $('lab-count').textContent=String(stats.total);$('lab-coverage').textContent=pct(stats.total,count*3);$('lab-aligned').textContent=pct(stats.aligned,stats.complete);$('lab-aligned-note').textContent=`${stats.aligned} / ${stats.complete} complete windows · same official direction`;
 $('lab-samples').textContent=String(points.length);$('lab-samples-note').textContent=`${changes.length} synchronized return pairs · CMC observations`;
 $('lab-status').textContent=loadedAt?`Last refresh ${time(loadedAt)} · ${count} completed slots · times in your timezone`:'Loading recorded observations…';
 const host=$('lab-ribbons');host.style.setProperty('--timeline-width',`${count*14+160}px`);host.replaceChildren();
 for(const a of ASSETS){const row=element('div',undefined,'lab-ribbon'),s=stats.perAsset[a],label=element('div',undefined,'lab-ribbon-label');label.append(element('strong',a),element('span',`${s.count}/${count} observed · ${pct(s.yes,s.count)} YES`));const cells=element('div',undefined,'lab-cells');cells.style.setProperty('--slots',String(count));
  for(const r of rows){const c=r.contracts[a],b=element('button',c?c.result==='yes'?'▲':'▼':'·','lab-cell');b.type='button';b.dataset.result=c?.result||'missing';b.setAttribute('aria-label',`${a}, closed ${time(r.at)}, ${c?c.result.toUpperCase():'missing archive'}`);b.setAttribute('aria-pressed',String(chosen===r.at));b.title=b.getAttribute('aria-label');b.addEventListener('click',()=>{detail(r);renderSelection()});cells.append(b)}
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
 source.append(sourceRow('CoinMarketCap',`${points.length} shared samples`,'Aggregate market context',latest?`Sample ${time(latest.t)}`:'No stored samples'),sourceRow('Blockworks','Access audit only','Not connected','Access unverified'));
 $('lab-export').disabled=!loadedAt||(!stats.total&&!points.length);
}
function sourceRow(...values){const tr=element('tr');for(const v of values)tr.append(element('td',v));return tr}
function renderSelection(){for(const b of $('lab-ribbons').querySelectorAll('button'))b.setAttribute('aria-pressed',String(b.getAttribute('aria-label').includes(time(chosen))))}
async function refresh(g){
 if(!active||g!==generation||flight)return;
 const f={controller:new AbortController()};flight=f;$('lab-refresh').disabled=true;
 const deadline=setTimeout(()=>f.controller.abort(),12000);
 try{
  const paths=[...ASSETS.map(a=>`/api/outcomes?asset=${a}`),'/api/context/history'];
  const results=await Promise.allSettled(paths.map(async p=>{const r=await fetch(p,{signal:f.controller.signal});if(!r.ok)throw Error('unavailable');return r.json()}));
  if(!active||g!==generation)return;
  ASSETS.forEach((a,i)=>{boards[a]=results[i].status==='fulfilled'?results[i].value:{status:'unavailable',contracts:[]}});
  context=results[3].status==='fulfilled'?results[3].value:{status:'unavailable',history:[]};loadedAt=Date.now();render();
  if(results.some(r=>r.status==='rejected'))$('lab-status').textContent+=' · some sources unavailable';
 }finally{clearTimeout(deadline);if(flight===f)flight=null;if(active&&g===generation){$('lab-refresh').disabled=false;timer=setTimeout(()=>refresh(g),document.hidden?300000:60000)}}
}
function leave(){active=false;generation++;clearTimeout(timer);flight?.controller.abort();flight=null;cancelCandles()}
for(const b of document.querySelectorAll('[data-lab-window]'))b.addEventListener('click',()=>{count=Number(b.dataset.labWindow);chosen=null;for(const p of document.querySelectorAll('[data-lab-window]'))p.setAttribute('aria-pressed',String(p===b));render()});
$('lab-refresh').addEventListener('click',()=>{clearTimeout(timer);refresh(generation)});
$('lab-export').addEventListener('click',()=>{const value={schema_version:1,exported_at:new Date().toISOString(),window_count:count,notes:'Official archived outcomes and CMC observations. Missing slots remain null. Not a training-ready point-in-time dataset. No Blockworks data.',windows:slots(boards,count),context};const url=URL.createObjectURL(new Blob([JSON.stringify(value,null,2)],{type:'application/json'}));const link=element('a');link.href=url;link.download=`crypto-data-lab-${new Date().toISOString().slice(0,10)}.json`;link.click();setTimeout(()=>URL.revokeObjectURL(url),1000)});
document.addEventListener('visibilitychange',()=>{if(active&&!document.hidden&&!flight){clearTimeout(timer);refresh(generation)}});
root.DataLab={enter(){leave();active=true;render();refresh(generation)},leave};
})(typeof window!=='undefined'?window:globalThis);
