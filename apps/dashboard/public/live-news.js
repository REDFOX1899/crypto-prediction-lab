(function(root){
'use strict';
let active=false,flight=null,timer=null,version=0,selected='All',data=null;
const el=(tag,text,cls)=>{const n=document.createElement(tag);if(text!=null)n.textContent=text;if(cls)n.className=cls;return n};
function draw(){
 for(const host of document.querySelectorAll('[data-live-news]')){
  host.replaceChildren();
  if(!data?.stories?.length){host.append(el('p','News is temporarily unavailable.','news-updated'));continue}
  const age=Math.max(0,Math.floor(Date.now()/1000-data.checked_at));
  const scorer=data.news_scorer==='gemini-decision-1'
   ? ` · Scored by gemini-decision-1${data.scoring_latency_ms!=null?` (${data.scoring_latency_ms}ms)`:''}`
   : '';
  const filtered=data.filtered_noise?` · ${data.filtered_noise} SEO/noise filtered`:'';
  const status=el('p',`${new Date(data.checked_at*1000).toLocaleString()} · ${age>1800?'Delayed check':'Checked every 15 min'} · ${data.stories.length} headlines${scorer}${filtered}`,'news-updated');
  host.append(status);
  const controls=el('div',null,'news-filters');
  controls.setAttribute('role','group');controls.setAttribute('aria-label','Filter headlines by asset');
  for(const a of ['All','BTC','ETH','SOL']){
   const b=el('button',a);b.type='button';b.setAttribute('aria-pressed',String(selected===a));
   b.onclick=()=>{selected=a;draw()};controls.append(b);
  }
  host.append(controls);
  const list=el('ul',null,'live-news-list');
  const rows=data.stories.filter(r=>selected==='All'||(r.assets||[]).includes(selected));
  for(const r of rows){
   const li=el('li'),top=el('div',null,'news-item-top');
   const dirLabel=r.catalyst_direction==='bullish'?'Bullish':r.catalyst_direction==='bearish'?'Bearish':r.tone==='positive'?'Positive':r.tone==='negative'?'Negative':'Neutral';
   const tag=el('span',dirLabel,'news-tone');tag.dataset.tone=r.tone||'mixed';
   const a=el('a',r.title);
   try{const u=new URL(r.url);if(u.protocol!=='https:')continue;a.href=u.href}catch{continue}
   a.target='_blank';a.rel='noopener noreferrer';
   top.append(tag);
   if(typeof r.impact_score==='number'){
    const imp=el('span',r.impact_score>=1.1?'Catalyst':'Context','news-impact');
    imp.dataset.level=r.impact_score>=1.1?'high':'mid';
    top.append(imp);
   }
   top.append(a);
   const meta=`${r.publisher} · ${r.via} · ${r.published_at?new Date(r.published_at*1000).toLocaleString():'Publication time unavailable'}`;
   li.append(top,el('p',r.reason,'news-reason'),el('small',meta));
   list.append(li);
  }
  if(!rows.length)list.append(el('li','No matching headlines in the current snapshot.'));
  host.append(list);
  const detail=el('details'),summary=el('summary','Sources, Tavily cadence & Gemini Decision 1 filter');
  const tav=(data.tavily||'unavailable').replaceAll('_',' '),cd=(data.coindesk||'unavailable').replaceAll('_',' '),fed=(data.fed||'unavailable').replaceAll('_',' ');
  detail.append(summary,el('p',`Tavily ${tav} · CoinDesk ${cd} · Federal Reserve ${fed}. SEO price-converter pages are stripped automatically; when Gemini Decision 1 is enabled on the collector, all headlines are scored in one single pass (0 output tokens).`));
  host.append(detail);
 }
}
async function load(){
 clearTimeout(timer);if(!active)return;
 const id=version,ctrl=new AbortController();flight=ctrl;
 const timeout=setTimeout(()=>ctrl.abort(),10000);
 try{const r=await fetch('/api/news',{signal:ctrl.signal});if(!r.ok)throw Error();const v=await r.json();if(active&&id===version){data=v;draw()}}
 catch{if(active&&id===version){if(data)draw();else for(const host of document.querySelectorAll('[data-live-news]'))host.textContent='News is temporarily unavailable.'}}
 finally{clearTimeout(timeout);if(flight===ctrl)flight=null;if(active&&id===version)timer=setTimeout(load,60000)}
}
root.LiveNews={show(mode){version++;clearTimeout(timer);flight?.abort();active=['home','data','quant'].includes(mode);if(active){if(data)draw();load()}}};
})(window);
