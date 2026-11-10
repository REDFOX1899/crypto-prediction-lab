(function(root){
'use strict';
const interval=15*60*1000;
function slots(contracts,count,now){
 const byClose=new Map(contracts.map(c=>[Date.parse(c.close_time),c]));
 const end=Math.floor(now/interval)*interval;
 return Array.from({length:count},(_,i)=>({time:end-(count-1-i)*interval,contract:byClose.get(end-(count-1-i)*interval)||null}));
}
function statistics(cells){
 let up=0,down=0,run=0,longest=0,previous=null;
 for(const cell of cells){const r=cell.contract?.result;if(!['yes','no'].includes(r)){run=0;previous=null;continue}if(r==='yes')up++;else down++;run=r===previous?run+1:1;longest=Math.max(longest,run);previous=r;}
 return {up,down,longest,missing:cells.length-up-down};
}
if(typeof module==='object'&&module.exports){module.exports={slots,statistics};return}
let symbol='BTC',generation=0,windowSize=48,contracts=[],timer=null,chosen=null;
const $=id=>document.getElementById(id);
const clock=t=>new Date(t).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'});
const stamp=t=>new Date(t).toLocaleString([], {month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'});
const money=n=>Number(n).toLocaleString('en-US',{style:'currency',currency:'USD',maximumFractionDigits:4});
function detail(c){
 const host=$('contract-detail');host.replaceChildren();
 const h=document.createElement('h3');h.textContent=`${c.result==='yes'?'▲ UP / at target':'▼ DOWN'} · ${stamp(c.open_time)}–${clock(c.close_time)}`;host.append(h);
 const line=text=>{const p=document.createElement('p');p.textContent=text;host.append(p)};
 line(`${c.target||'Target unavailable'} · Official expiration value: ${c.expiration_value??'not supplied'}`);
 if(c.best_entry){
  const entry=c.best_entry,minute=Number(entry.minute_start)*1000;
  line(`Hindsight only: the winning ${c.result.toUpperCase()} side’s lowest observed ask was in the one-minute interval ${stamp(minute)}–${clock(minute+60000)}. This is a candle interval, not an exact order-entry time.`);
  const values=document.createElement('div');values.className='detail-values';
  for(const [value,label] of [[`${(Number(entry.ask)*100).toLocaleString(undefined,{maximumFractionDigits:3})}¢`,'Lowest winning-side ask'],[`${Number(entry.gross_multiple).toFixed(2)}×`,'Theoretical gross payout / cost'],[money(1),'Settlement payout per winning contract']]){const node=document.createElement('div'),b=document.createElement('b'),span=document.createElement('span');b.textContent=value;span.textContent=label;node.append(b,span);values.append(node)}host.append(values);
  line('Gross multiple = $1 ÷ quoted ask. Hypothetical profit per winning contract = $1 minus that ask, before fees. Quote size and queue position are not known; this was not a guaranteed trade.');
 }else line('No valid pre-close winning-side quote was available. We do not invent an entry price or payout multiple.');
 const replay=document.createElement('button');replay.type='button';replay.className='replay-launch';replay.textContent='Replay this market';replay.addEventListener('click',()=>root.MarketReplay.open(c));host.append(replay);
 line(`${c.candle_count} source candle records stored · ${c.ticker}`);
}
function render(){
 const cells=slots(contracts,windowSize,Date.now()),s=statistics(cells),host=$('outcome-cells');host.replaceChildren();
 $('board-summary').textContent=`${symbol} · ${s.up} UP / equal · ${s.down} DOWN · longest observed run ${s.longest} · ${s.missing} awaiting archive`;
 for(const {time,contract:c} of cells){
  const b=document.createElement('button');b.type='button';b.className=`result-cell ${c?.result==='yes'?'up':c?.result==='no'?'down':''}`;b.disabled=!c;
  const label=document.createElement('strong'),timeLabel=document.createElement('small');label.textContent=c?.result==='yes'?'▲':c?.result==='no'?'▼':'—';timeLabel.textContent=clock(time);b.append(label,timeLabel);
  b.setAttribute('aria-label',`${stamp(time)} ${c?.result==='yes'?'YES at or above target':c?.result==='no'?'NO below target':'awaiting archive'}`);
  b.setAttribute('aria-pressed',String(c?.ticker===chosen));
  if(c)b.addEventListener('click',()=>{chosen=c.ticker;render();detail(c)});host.append(b);
 }
 const latest=[...cells].reverse().find(x=>x.contract)?.contract;
 const selected=contracts.find(c=>c.ticker===chosen)||latest;
 if(selected){chosen=selected.ticker;detail(selected)}else $('contract-detail').textContent='No settled results are available for this display window yet.';
}
async function load(g){
 try{
  const r=await fetch(`/api/outcomes?asset=${encodeURIComponent(symbol)}`);if(!r.ok)throw Error('Archive unavailable');const data=await r.json();if(g!==generation)return;
  contracts=(data.contracts||[]).filter(c=>c.asset===symbol&&['yes','no'].includes(c.result));
  render();$('board-source').textContent=data.status==='not_configured'?'History storage is not configured in this environment.':`Kalshi official settled results · stored in our archive · fetched ${stamp(data.fetched_at)}. Collector runs every 15 minutes; recent or delayed settlements may appear later. Times use your device’s timezone. Oldest → newest.`;
 }catch{if(g!==generation)return;$('board-source').textContent='History archive is temporarily unavailable. Retrying in one minute.';if(!contracts.length)$('board-summary').textContent='Settled history unavailable';}
 finally{if(g===generation)timer=setTimeout(()=>load(g),60000)}
}
root.OutcomeBoard={pause(){generation++;clearTimeout(timer)},select(asset){symbol=asset;generation++;clearTimeout(timer);contracts=[];chosen=null;render();$('board-source').textContent='Loading archived results…';load(generation)}};
for(const b of document.querySelectorAll('[data-window]'))b.addEventListener('click',()=>{windowSize=Number(b.dataset.window);for(const other of document.querySelectorAll('[data-window]'))other.setAttribute('aria-pressed',String(other===b));render()});
})(typeof window!=='undefined'?window:globalThis);
