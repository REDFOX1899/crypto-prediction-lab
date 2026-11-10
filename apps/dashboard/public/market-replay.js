(function(root){
'use strict';
function visibleCandles(data,minute){
 const start=Date.parse(data.open_time)/1000;
 return (data.candles||[]).filter(c=>c.time>=start&&c.time+60<=start+minute*60);
}
if(typeof module==='object'&&module.exports){module.exports={visibleCandles};return}
const $=id=>document.getElementById(id),dialog=$('market-replay');
let generation=0,flight=null,timer=null,data=null,checkpoint=null;
const cents=v=>v==null?'Missing':(Number(v)*100).toFixed(2)+'¢';
function stop(){clearInterval(timer);timer=null;$('replay-play').textContent='Play'}
function cancel(){generation++;flight?.abort();flight=null;stop()}
function svg(tag,attrs){const n=document.createElementNS('http://www.w3.org/2000/svg',tag);for(const [k,v]of Object.entries(attrs))n.setAttribute(k,v);return n}
function render(){
 if(!data)return;
 const minute=Number($('replay-minute').value),candles=visibleCandles(data,minute),start=Date.parse(data.open_time)/1000;
 $('replay-time').textContent='Minute '+minute+' of 15';
 $('replay-outcome').textContent=minute===15?'Official outcome · '+data.result.toUpperCase():'Outcome revealed at minute 15';
 const latest=candles.at(-1);
 $('replay-quote').textContent=latest?'Last recorded YES ask '+cents(latest.close)+' · minute '+Math.round((latest.time+60-start)/60):'No completed quote minute yet';
 $('replay-coverage').textContent=candles.length+' of '+minute+' elapsed minutes recorded · retrospective Kalshi YES ask candles';
 const graph=$('replay-chart');graph.replaceChildren();
 for(const p of [0,25,50,75,100]){const y=195-p*1.6;graph.append(svg('line',{x1:42,x2:880,y1:y,y2:y,stroke:'var(--border)'}));const t=svg('text',{x:1,y:y+4});t.textContent=p+'¢';graph.append(t)}
 for(let m=0;m<=15;m+=5){const t=svg('text',{x:42+m/15*830,y:220,'text-anchor':'middle'});t.textContent=m+'m';graph.append(t)}
 let previous=null,path='';
 for(const c of candles){const x=42+(c.time+60-start)/900*830,y=195-Number(c.close)*160;
 path+=(previous!==null&&c.time-previous===60?'L':'M')+x+','+y+' ';previous=c.time;
 graph.append(svg('circle',{cx:x,cy:y,r:4,fill:'var(--accent)'}))}
 graph.append(svg('path',{d:path,fill:'none',stroke:'var(--accent)','stroke-width':2}),svg('line',{x1:42+minute/15*830,x2:42+minute/15*830,y1:25,y2:195,stroke:'var(--muted)','stroke-dasharray':'3 4'}));
 graph.setAttribute('aria-label',candles.length+' recorded minute quotes through minute '+minute+'; future quotes hidden');
 const values=$('replay-values');values.replaceChildren();
 for(const c of candles){const row=document.createElement('tr');for(const text of [String((c.time+60-start)/60),cents(c.open),cents(c.high),cents(c.low),cents(c.close)]){const cell=document.createElement('td');cell.textContent=text;row.append(cell)}values.append(row)}
 const note=$('replay-checkpoint');note.replaceChildren();
 if(minute<10){note.textContent='Minute 10 snapshot appears after its actual receipt time.'}
 else if(checkpoint?.observation){
  const received=Date.parse(checkpoint.received_at)/1000;
  if(start+minute*60<received){note.textContent='Minute 10 capture had not arrived at this replay time.';return}
  const o=checkpoint.observation;
  note.textContent='Minute 10 · '+checkpoint.status+' · received '+new Date(checkpoint.received_at).toLocaleTimeString()+' · '+checkpoint.seconds_after_cutoff.toFixed(3)+'s after cutoff. ';
  const table=document.createElement('table');
  for(const [side,levels]of [['YES bids',o.yes_bids],['NO bids',o.no_bids]]){
   const row=document.createElement('tr');const h=document.createElement('th');h.textContent=side;const c=document.createElement('td');
   c.textContent=levels.length?levels.map(([price,size])=>cents(price)+' × '+size).join(' · '):'No displayed bids';row.append(h,c);table.append(row)}
  note.append(table);
 }else note.textContent=checkpoint?.status==='unavailable'?'Minute 10 collection attempted; market/book unavailable.':checkpoint?.status==='fetch_failed'?'Saved snapshot could not be loaded.':'No minute 10 snapshot was recorded for this window.';
}
async function open(c){
 cancel();data=null;checkpoint=null;const g=generation;
 $('replay-title').textContent=c.asset+' · market replay';
 $('replay-window').textContent=new Date(c.open_time).toLocaleString()+' · '+c.ticker;
 $('replay-outcome').textContent='Outcome revealed at minute 15';
 $('replay-chart').replaceChildren();$('replay-values').replaceChildren();$('replay-quote').textContent='Loading archived minute quotes…';$('replay-coverage').textContent='';$('replay-checkpoint').textContent='';
 $('replay-time').textContent='Minute 0 of 15';$('replay-minute').value='0';$('replay-minute').disabled=true;$('replay-play').disabled=true;
 if(!dialog.open)dialog.showModal();
 flight=new AbortController();const controller=flight,timeout=setTimeout(()=>controller.abort(),12000);
 const q='?asset='+encodeURIComponent(c.asset)+'&ticker='+encodeURIComponent(c.ticker);
 try{
  const [quotes,saved]=await Promise.allSettled(['/api/archive/window','/api/research/checkpoint'].map(async path=>{const r=await fetch(path+q,{signal:controller.signal});if(!r.ok)throw Error();return r.json()}));
  if(g!==generation)return;
  if(quotes.status!=='fulfilled'||quotes.value.ticker!==c.ticker||quotes.value.asset!==c.asset||quotes.value.status!=='available')throw Error();
  data=quotes.value;checkpoint=saved.status==='fulfilled'?saved.value:{status:'fetch_failed'};
  $('replay-minute').disabled=false;$('replay-play').disabled=false;render();
 }catch{if(g===generation)$('replay-quote').textContent='Replay unavailable. Close and reopen to retry.'}
 finally{clearTimeout(timeout);if(flight===controller)flight=null}
}
$('replay-close').addEventListener('click',()=>dialog.close());
dialog.addEventListener('close',cancel);
$('replay-minute').addEventListener('input',()=>{stop();render()});
$('replay-play').addEventListener('click',()=>{
 if(timer){stop();return}if(Number($('replay-minute').value)===15)$('replay-minute').value='0';
 $('replay-play').textContent='Pause';timer=setInterval(()=>{$('replay-minute').value=String(Number($('replay-minute').value)+1);render();if(Number($('replay-minute').value)===15)stop()},1000);
});
document.addEventListener('visibilitychange',()=>{if(document.hidden)stop()});
root.MarketReplay={open,close(){cancel();if(dialog.open)dialog.close()},async latest(asset){
 cancel();const g=generation,controller=new AbortController();flight=controller;const timeout=setTimeout(()=>controller.abort(),10000);
 try{const r=await fetch('/api/outcomes?asset='+asset+'&view=recent',{signal:controller.signal});if(!r.ok)throw Error();const d=await r.json();if(g!==generation)return;
 const c=d.contracts?.find(c=>c.asset===asset&&c.ticker);if(c)open(c);
 }finally{clearTimeout(timeout);if(flight===controller)flight=null}
}};
})(typeof window!=='undefined'?window:globalThis);
