(function(root){
'use strict';
const assets=['BTC','ETH','SOL'],period=900000;
function legs(picks){return assets.filter(a=>['yes','no'].includes(picks[a]))}
function alignedWindow(markets,picks,now){
 const selected=legs(picks);
 if(selected.length<2)return {ready:false,reason:'Choose UP or DOWN for at least two assets.'};
 const windows=selected.map(a=>markets[a]);
 if(windows.some(m=>!m||m.status!=='active'))return {ready:false,reason:'Waiting for active markets for every pick.'};
 const first=windows[0],open=Date.parse(first.open_time),close=Date.parse(first.close_time);
 if(!Number.isFinite(open)||!Number.isFinite(close)||close-open!==period||now<open||now>=close)return {ready:false,reason:'This window has closed or is not yet open. Wait for the next 15-minute window.'};
 if(windows.some(m=>Date.parse(m.open_time)!==open||Date.parse(m.close_time)!==close))return {ready:false,reason:'Picks belong to different windows. Wait until their opening and closing times match.'};
 return {ready:true,open,close};
}
function jointHistory(history,picks,now,count=96){
 const selected=legs(picks);if(selected.length<2)return [];
 const end=Math.floor(now/period)*period;
 const maps=Object.fromEntries(selected.map(a=>[a,new Map((history[a]||[]).filter(c=>c.asset===a&&['yes','no'].includes(c.result)&&Date.parse(c.close_time)-Date.parse(c.open_time)===period).map(c=>[Date.parse(c.close_time),c]))]));
 return Array.from({length:count},(_,i)=>{
  const close=end-(count-1-i)*period;
  const records=selected.map(a=>maps[a].get(close));
  const complete=records.every(Boolean)&&records.every(c=>Date.parse(c.open_time)===close-period);
  return {close,complete,matched:complete?records.every((c,j)=>c.result===picks[selected[j]]):null};
 });
}
function scenario(cents,budget){
 const price=Number(cents),amount=Number(budget);
 if(cents===''||budget===''||!Number.isFinite(price)||!Number.isFinite(amount)||price<0.1||price>99.9||amount<=0||amount>1000)return null;
 const contracts=Math.floor((amount*100+1e-9)/price),cost=contracts*price/100;
 return {contracts,cost,payout:contracts,profit:contracts-cost,multiple:100/price};
}
// Preview only the displayed public ask quantity. Never manufacture a price from legs.
function quotedPreview(book,markets,picks,budget,now){
 const w=alignedWindow(markets,picks,now),selected=legs(picks),amount=Number(budget),ask=Number(book?.ask),size=Number(book?.ask_size),checked=Date.parse(book?.checked_at);
 if(!w.ready||!book||book.status!=='available'||budget===''||!Number.isFinite(amount)||amount<=0||amount>1000||!Number.isFinite(ask)||ask<=0||ask>=1||!Number.isFinite(size)||size<=0||!Number.isFinite(checked)||now-checked>15000||checked>now+1000||Date.parse(book.window_close)!==w.close)return null;
 if(book.legs?.length!==selected.length||!selected.every(a=>book.legs.filter(l=>l.market_ticker===markets[a].ticker&&l.side===picks[a]).length===1))return null;
 const requested=Math.floor((amount+1e-9)/ask),contracts=Math.min(requested,Math.floor(size)),cost=contracts*ask;
 return {contracts,cost,payout:contracts,limited:contracts<requested};
}
if(typeof module==='object'&&module.exports){module.exports={legs,alignedWindow,jointHistory,scenario,quotedPreview};return}
const $=id=>document.getElementById(id),money=n=>Number(n).toLocaleString('en-US',{style:'currency',currency:'USD',maximumFractionDigits:2});
const clock=t=>new Date(t).toLocaleTimeString([], {hour:'numeric',minute:'2-digit'});
const cents=n=>n==null||!Number.isFinite(Number(n))?'—':`${(Number(n)*100).toLocaleString(undefined,{maximumFractionDigits:2})}¢`;
let picks={BTC:null,ETH:null,SOL:null},markets={},snapshots={},quotes={},history={},historyState={},errors={},active=false,generation=0,timer=null,historyTimer=null,clockTimer=null,streams=[];
let currentClose=null,notice='',book=null,bookKey='',bookRequest=0,bookTimer=null,bookLoading=false;
function pickKey(){return assets.filter(a=>picks[a]).map(a=>`${a}:${picks[a]}:${markets[a]?.ticker||''}`).join('|')}
function resetBook(){bookLoading=false;book=null;bookKey='';bookRequest++;clearTimeout(bookTimer);bookTimer=null}
function renderComboBook(){
 const ready=alignedWindow(markets,picks,Date.now()).ready;
 const matches=book&&bookKey===pickKey()&&book.legs?.every(l=>assets.some(a=>picks[a]===l.side&&markets[a]?.ticker===l.market_ticker));
 const fresh=matches&&Date.now()-Date.parse(book.checked_at)<15000;
 const preview=fresh?quotedPreview(book,markets,picks,$('combo-budget').value,Date.now()):null;
 const amount=Number($('combo-budget').value);
 text('combo-summary-budget',preview&&preview.contracts>0?money(preview.cost):Number.isFinite(amount)&&amount>0&&amount<=1000?money(amount):'Your budget');
 text('combo-summary-payout',preview&&preview.contracts>0?money(preview.payout):'—');
 text('combo-summary-caption',preview?(preview.contracts?`${preview.contracts} whole contracts at the public ask · ${money(preview.cost)} before fees.${preview.limited?' Limited to displayed size.':''} Pays only if every pick wins.`:'The displayed ask size or budget is too small for one whole contract.'):ready?'Potential payout appears when a fresh public ask and quantity are available.':'Choose two or three directions to check the public quote.');
 const coverage=book?.markets_found?` · ${book.markets_checked}/${book.markets_found} matching books checked${book.book_errors?' · some checks failed':''}`:'';
 const state=!ready?'Choose at least two active picks':!book?'Checking existing markets…':!fresh?'Quote unavailable / stale':({available:'Public ask available',bid_only:'No ask available',empty:'No quote available',not_found:'No existing match found',search_limited:'Search incomplete',unavailable:'Source unavailable',waiting_window:'Waiting for next window'})[book.status]||'No quote available';
 text('combo-book-state',state);text('combo-book-message',!ready?'Picks must share the same active 15-minute window.':fresh?book.message:'Waiting for a verified current order-book snapshot.');
 text('combo-book-bid',fresh?cents(book.bid):'—');text('combo-book-ask',fresh?cents(book.ask):'—');
 text('combo-book-source',fresh?`${book.ticker||'Kalshi public API'} · checked ${Math.max(0,Math.floor((Date.now()-Date.parse(book.checked_at))/1000))}s ago${coverage}${book.ask_size!=null?` · ${Number(book.ask_size).toLocaleString()} contracts at best ask`:''}`:'');
 for(const [id,levels]of [['combo-bids',fresh?book.yes_bids:[]],['combo-asks',fresh?book.yes_asks:[]]]){
  const host=$(id);host.replaceChildren();const max=Math.max(0,...(levels||[]).map(l=>Number(l[1])));
  for(const [p,q]of levels||[]){const row=document.createElement('div');row.className=`book-row ${id==='combo-bids'?'yes':'no'}`;const bar=document.createElement('span');bar.className='depth-bar';bar.style.width=`${max?Number(q)/max*100:0}%`;bar.setAttribute('aria-hidden','true');const price=document.createElement('span'),size=document.createElement('span');price.className=size.className='depth-text';price.textContent=cents(p);size.textContent=Number(q).toLocaleString();row.append(bar,price,size);host.append(row)}
 }
}
async function loadComboBook(){
 clearTimeout(bookTimer);bookTimer=null;if(!active)return;
 const g=generation,key=pickKey(),request=++bookRequest;bookLoading=true;
 if(!alignedWindow(markets,picks,Date.now()).ready){resetBook();renderComboBook();return}
 if(key!==bookKey){book=null;bookKey=key}renderComboBook();
 const params=new URLSearchParams();for(const a of assets)if(picks[a])params.set(a.toLowerCase(),picks[a]);
 try{const r=await fetch(`/api/combos/quote?${params}`,{cache:'no-store',signal:AbortSignal.timeout(15000)});if(!r.ok)throw Error();const data=await r.json();if(!active||g!==generation||request!==bookRequest||key!==pickKey())return;book=data;}
 catch{if(!active||g!==generation||request!==bookRequest||key!==pickKey())return;book={status:'unavailable',message:'The public combo lookup failed. Retrying shortly.',checked_at:new Date().toISOString(),legs:[]};}
 if(!active||g!==generation||request!==bookRequest)return;bookLoading=false;renderComboBook();bookTimer=setTimeout(loadComboBook,document.hidden?20000:5000);
}

function text(id,value){$(id).textContent=value}
function render(){
 const selected=legs(picks),now=Date.now(),window=alignedWindow(markets,picks,now);
 text('combo-count',selected.length?`${selected.length} MARKET${selected.length===1?'':'S'} COMBO`:'CHOOSE 2+ MARKETS');
 const slip=$('combo-picks'),selectionKey=selected.join('|');
 if(slip.dataset.assets!==selectionKey){
  slip.dataset.assets=selectionKey;slip.replaceChildren();
  for(const asset of selected){
   const row=document.createElement('div');row.className='combo-slip-leg';
   const info=document.createElement('div'),label=document.createElement('strong'),detail=document.createElement('span'),side=document.createElement('span'),remove=document.createElement('button');
   label.textContent=`${asset} 15 min`;detail.className='combo-leg-detail';detail.id=`combo-leg-detail-${asset}`;side.className='combo-leg-side';side.id=`combo-leg-side-${asset}`;remove.type='button';remove.className='combo-remove';remove.dataset.remove=asset;remove.setAttribute('aria-label',`Remove ${asset} pick`);remove.textContent='×';info.append(label,detail);row.append(info,side,remove);slip.append(row);
  }
  if(!selected.length){const p=document.createElement('p');p.className='combo-slip-empty';p.textContent='Your picks will appear here. Choose directions on two or three markets.';slip.append(p)}
 }
 for(const asset of assets){
  const m=markets[asset],fresh=snapshots[asset]&&now-Date.parse(snapshots[asset])<45000;
  text(`combo-target-${asset}`,m?.yes_sub_title?.replace(/^target price:\s*/i,'')||'—');
 $(`combo-card-${asset}`).dataset.selected=picks[asset]||'';
  text(`combo-time-${asset}`,m?`${clock(m.open_time)}–${clock(m.close_time)}`:'15-minute window');
 $(`combo-time-${asset}`).title=m?.ticker||'';
  const quote=quotes[asset],matching=!!(m&&quote&&quote.ticker===m.ticker);
  const up=matching?quote.yes_ask:m?.yes_ask_dollars;
  const down=matching&&quote.yes_bid!=null?1-Number(quote.yes_bid):m?.no_ask_dollars;
  const percent=n=>n==null||!Number.isFinite(Number(n))?'—':`${(Number(n)*100).toLocaleString(undefined,{maximumFractionDigits:2})}%`;
  text(`combo-up-${asset}`,`UP ${percent(up)}`);text(`combo-down-${asset}`,`DOWN ${percent(down)}`);
  const mode=errors[asset]?'snapshot unavailable':quote?.state==='live'&&matching?(root.DASHBOARD_POLLING?'live · polled 2s':'live ticker'):'snapshot';
  const live=mode.startsWith('live');text(`combo-source-${asset}`,live?'● LIVE':errors[asset]?'UNAVAILABLE':'SNAPSHOT');$(`combo-source-${asset}`).dataset.live=String(live);
  text(`combo-checked-${asset}`,snapshots[asset]?`Checked ${Math.max(0,Math.floor((now-Date.parse(snapshots[asset]))/1000))}s ago`:'');
  for(const b of $(`combo-card-${asset}`).querySelectorAll('[data-pick]')){
   b.setAttribute('aria-pressed',String(picks[asset]===b.dataset.pick));
   b.disabled=!(m&&fresh&&m.status==='active'&&now>=Date.parse(m.open_time)&&now<Date.parse(m.close_time));
  }
  if(picks[asset]){
   text(`combo-leg-detail-${asset}`,`${picks[asset]==='yes'?'UP':'DOWN'} · ${m?.yes_sub_title?.replace(/^target price:\s*/i,'')||'Target unavailable'}`);
   text(`combo-leg-side-${asset}`,percent(picks[asset]==='yes'?up:down));
   $(`combo-leg-side-${asset}`).className=`combo-leg-side ${picks[asset]==='yes'?'combo-yes':'combo-no'}`;
  }
 }
 const fresh=selected.every(a=>snapshots[a]&&now-Date.parse(snapshots[a])<45000&&!errors[a]);
 const ready=window.ready&&fresh;
 text('combo-window',ready?`Same window · ${clock(window.open)}–${clock(window.close)}`:window.ready?'Market discovery is stale. Waiting for a fresh check.':window.reason);
 $('combo-window').className=ready?'combo-window':'combo-window warning';
 text('combo-rollover',notice);
 text('combo-clock',ready?`${String(Math.floor((window.close-now)/60000)).padStart(2,'0')}:${String(Math.floor((window.close-now)/1000)%60).padStart(2,'0')} remaining`:'15-minute windows only');
 $('combo-assumed-price').disabled=!ready;
 const assumption=ready?scenario($('combo-assumed-price').value,$('combo-budget').value):null;
 text('combo-payout',assumption?money(assumption.payout):'—');
 text('combo-profit',assumption?`${money(assumption.profit)} hypothetical profit before fees`:'Enter an assumed combo price to explore a scenario.');
 text('combo-math',assumption?`${assumption.contracts} whole contracts · hypothetical cost ${money(assumption.cost)} · ${assumption.multiple.toFixed(2)}× gross payout / cost. Unspent balance is excluded.`:'This calculator uses only your assumed price. The public combo order book is shown separately; individual asks are not a combo price.');
 renderComboBook();renderHistory();
}
function renderHistory(){
 const selected=legs(picks),host=$('combo-history-cells');
 if(selected.length<2){host.replaceChildren();text('combo-history-summary','Choose at least two directions to inspect their shared history.');text('combo-history-source','');return}
 const rows=jointHistory(history,picks,Date.now()),complete=rows.filter(r=>r.complete),wins=complete.filter(r=>r.matched).length;
 text('combo-history-summary',`${wins} / ${complete.length} complete windows matched all your picks${complete.length?` · ${(wins/complete.length*100).toFixed(1)}% observed`:''} · ${rows.length-complete.length} incomplete`);
 const version=JSON.stringify([picks,rows]);if(host.dataset.version!==version){
  host.replaceChildren();host.dataset.version=version;
  for(const row of rows){const cell=document.createElement('span');cell.className=`combo-history-cell ${row.complete?(row.matched?'match':'miss'):''}`;cell.textContent=row.complete?(row.matched?'✓':'×'):'—';cell.title=`${new Date(row.close).toLocaleString()} · ${row.complete?(row.matched?'all picks matched':'at least one pick differed'):'incomplete archive'}`;host.append(cell)}
 }
 const unavailable=selected.filter(a=>historyState[a]!=='available');
 text('combo-history-source',`Last 24 hours · equal opening and closing times only · ${selected.join(' + ')}.${unavailable.length?` Archive unavailable or loading: ${unavailable.join(', ')}.`:''} Historical frequency is not a forecast or fair combo price.`);
}
async function refresh(g){
 const results=await Promise.allSettled(assets.map(async asset=>{
  const r=await fetch(`/api/snapshot?asset=${asset}`,{cache:'no-store'});if(!r.ok)throw Error();return {asset,data:await r.json()};
 }));if(!active||g!==generation)return;
 for(let i=0;i<assets.length;i++){
  const result=results[i],asset=assets[i];
  if(result.status==='fulfilled'){
   const data=result.value.data,m=data.market;
   if(picks[asset]&&markets[asset]?.ticker&&markets[asset].ticker!==m?.ticker){picks={BTC:null,ETH:null,SOL:null};$('combo-assumed-price').value='';notice='A market rolled over. Choose new picks for the current window.';currentClose=null;}
   markets[asset]=m;snapshots[asset]=data.fetched_at;errors[asset]=false;
   if(quotes[asset]?.ticker!==m?.ticker)delete quotes[asset];
  }else errors[asset]=true;
 }
 render();if(legs(picks).length>=2&&!bookTimer&&!bookLoading)loadComboBook();timer=setTimeout(()=>refresh(g),20000);
}
async function loadHistory(g){
 await Promise.allSettled(assets.map(async asset=>{
  try{const r=await fetch(`/api/outcomes?asset=${asset}`);if(!r.ok)throw Error();const data=await r.json();if(!active||g!==generation)return;history[asset]=data.contracts||[];historyState[asset]=data.status}
  catch{if(active&&g===generation)historyState[asset]='unavailable'}
 }));if(!active||g!==generation)return;renderHistory();historyTimer=setTimeout(()=>loadHistory(g),60000);
}
function leave(){resetBook();active=false;generation++;clearTimeout(timer);clearTimeout(historyTimer);clearInterval(clockTimer);for(const stream of streams)stream.close();streams=[]}
root.ComboLab={enter(){
 leave();active=true;const g=generation;markets={};snapshots={};quotes={};errors={};history={};historyState={};picks={BTC:null,ETH:null,SOL:null};$('combo-assumed-price').value='';currentClose=null;notice='';
 render();refresh(g);loadHistory(g);
 streams=assets.map(asset=>{
  const stream=new DashboardFeed(`/api/live?asset=${asset}`);
  stream.onmessage=event=>{if(!active||g!==generation)return;try{const u=JSON.parse(event.data),previous=quotes[asset];quotes[asset]=u;if(!previous||['state','ticker','received_at','yes_ask','yes_bid'].some(k=>previous[k]!==u[k]))render()}catch{quotes[asset]={state:'invalid'};render()}};
  stream.onerror=()=>{if(active&&g===generation){quotes[asset]={state:'reconnecting'};render()}};return stream;
 });
 clockTimer=setInterval(()=>{if(currentClose&&Date.now()>=currentClose){picks={BTC:null,ETH:null,SOL:null};$('combo-assumed-price').value='';currentClose=null;notice='The selected window closed. Choose new picks when the next markets appear.'}render()},1000);
},leave};
for(const asset of assets){
 for(const button of $(`combo-card-${asset}`).querySelectorAll('[data-pick]'))button.addEventListener('click',()=>{
  picks[asset]=picks[asset]===button.dataset.pick?null:button.dataset.pick;
  const window=alignedWindow(markets,picks,Date.now());currentClose=window.ready?window.close:null;notice='';resetBook();render();loadComboBook();
 });
}
$('combo-picks').addEventListener('click',event=>{const asset=event.target.closest('[data-remove]')?.dataset.remove;if(!assets.includes(asset))return;picks[asset]=null;const w=alignedWindow(markets,picks,Date.now());currentClose=w.ready?w.close:null;resetBook();render();loadComboBook();});
$('combo-add-five').addEventListener('click',()=>{const value=Number($('combo-budget').value);$('combo-budget').value=String(Math.min(1000,Math.max(0,Number.isFinite(value)?value:0)+5));render()});
$('combo-clear').addEventListener('click',()=>{picks={BTC:null,ETH:null,SOL:null};currentClose=null;notice='';resetBook();render()});
for(const id of ['combo-assumed-price','combo-budget'])$(id).addEventListener('input',render);
})(typeof window!=='undefined'?window:globalThis);
