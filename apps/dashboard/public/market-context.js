(function(root){'use strict';
const assets=['BTC','ETH','SOL'];
function valid(n){return typeof n==='number'&&Number.isFinite(n)}
function age(at,now){const t=Date.parse(at);return Number.isFinite(t)&&t<=now+120000?Math.max(0,now-t):Infinity}
function percent(n){return valid(n)?`${n>0?'+':''}${n.toFixed(2)}%`:'—'}
function trend(history,symbol){
 const points=(history||[]).map(s=>({t:Date.parse(s.at),q:s.quotes?.find(q=>q.symbol===symbol)})).filter(p=>p.q&&valid(p.q.price)&&p.q.price>0&&Number.isFinite(p.t)).sort((a,b)=>a.t-b.t);
 if(points.length<2)return {path:'',count:points.length};
 const first=points[0].q.price, lo=Math.min(...points.map(p=>p.q.price)),hi=Math.max(...points.map(p=>p.q.price)),span=Math.max(hi-lo,first*.0001),start=points[0].t,end=points.at(-1).t;
 let path='';for(let i=0;i<points.length;i++){const p=points[i],x=8+(p.t-start)/Math.max(1,end-start)*284,y=48-(p.q.price-lo)/span*38;path+=`${!i||p.t-points[i-1].t>450000?'M':'L'}${x.toFixed(1)},${y.toFixed(1)} `}return {path,count:points.length};
}
if(typeof module==='object'&&module.exports){module.exports={valid,age,percent,trend};return}
const $=id=>document.getElementById(id), money=new Intl.NumberFormat('en-US',{style:'currency',currency:'USD',maximumFractionDigits:2}),compact=new Intl.NumberFormat('en-US',{style:'currency',currency:'USD',notation:'compact',maximumFractionDigits:2});
let active=false,timer,flight,generation=0,data=null;
const put=(id,v)=>{const e=$(id);if(e&&e.textContent!==v)e.textContent=v};
const amount=(n,c=false)=>valid(n)?(c?compact:money).format(n):'—';
function freshness(at,limit){const ms=age(at,Date.now());return !Number.isFinite(ms)?'Unavailable':`${ms>limit?'Stale · ':''}${ms<60000?'less than 1m':Math.floor(ms/60000)+'m'} ago`}
function render(){
 const latest=data?.latest,available=data?.status==='available';
 put('context-status',available?`CMC · sampled every 5m · ${freshness(latest?.quotes_checked_at,660000)}`:'Context temporarily unavailable · live markets continue above');
 const global=latest?.global;
 put('context-cap',amount(global?.market_cap,true));put('context-volume',amount(global?.volume_24h,true));
 put('context-dominance',valid(global?.btc_dominance)?`${global.btc_dominance.toFixed(2)}%`:'—');
 put('context-global-age',`Global context · ${freshness(global?.source_at,7500000)} · checked hourly`);
 const sentiment=latest?.sentiment,value=valid(sentiment?.value)?Math.min(100,Math.max(0,sentiment.value)):null;
 put('context-sentiment',value===null?'—':`${Math.round(value)} / 100`);put('context-sentiment-label',value===null?'Sentiment unavailable':sentiment.label);
 $('context-sentiment-marker').setAttribute('cx',value===null?'10':String(10+value*1.8));$('context-sentiment-marker').setAttribute('visibility',value===null?'hidden':'visible');
 put('context-sentiment-age',`Sampled daily · ${freshness(sentiment?.source_at,172800000)}`);
 for(const a of assets){
  const q=latest?.quotes?.find(q=>q.symbol===a),stale=age(q?.source_at,Date.now())>660000;
  $(`context-card-${a}`).dataset.stale=String(stale);put(`context-price-${a}`,amount(q?.price));put(`context-age-${a}`,`CMC observation · ${freshness(q?.source_at,660000)}`);
  put(`context-mcap-${a}`,amount(q?.market_cap,true));put(`context-vol-${a}`,amount(q?.volume_24h,true));
  for(const [id,key]of [['1h','percent_change_1h'],['24h','percent_change_24h'],['7d','percent_change_7d']]){
   const v=q?.[key],e=$(`context-${id}-${a}`);e.textContent=percent(v);e.dataset.direction=valid(v)?v>=0?'up':'down':'unknown';
  }
  const value=q?.percent_change_24h,bar=$(`context-bar-${a}`),width=valid(value)?Math.min(48,Math.abs(value)/10*48):0;
  bar.setAttribute('x',valid(value)&&value<0?String(50-width):'50');bar.setAttribute('width',String(width));bar.dataset.direction=valid(value)&&value<0?'down':'up';
  $(`context-bar-svg-${a}`).setAttribute('aria-label',`${a} 24-hour change ${percent(value)}; bar scale capped at plus or minus 10 percent`);
  const plot=trend(data?.history,a);$(`context-trend-${a}`).setAttribute('d',plot.path);
  put(`context-trend-label-${a}`,plot.count<2?'History starts with collection':'Recorded CMC samples · auto scale');
 }
}
async function refresh(g){
 if(!active||g!==generation||flight)return;
 const f={controller:new AbortController()};flight=f;const deadline=setTimeout(()=>f.controller.abort(),10000);
 try{const r=await fetch('/api/context',{signal:f.controller.signal});if(!r.ok)throw Error();const next=await r.json();if(active&&g===generation){data=next;render()}}
 catch{if(active&&g===generation){render();put('context-status','Context refresh unavailable · previous observations retained')}}
 finally{clearTimeout(deadline);if(flight===f)flight=null;if(active&&g===generation)timer=setTimeout(()=>refresh(g),document.hidden?300000:60000)}
}
function leave(){active=false;generation++;clearTimeout(timer);flight?.controller.abort();flight=null}
root.MarketContext={enter(){leave();active=true;const g=generation;render();refresh(g)},leave};
document.addEventListener('visibilitychange',()=>{if(active&&!document.hidden&&!flight){clearTimeout(timer);refresh(generation)}});
})(typeof window!=='undefined'?window:globalThis);
