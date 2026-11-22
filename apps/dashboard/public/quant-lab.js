(function(root){
'use strict';

function formatPosture(posture){
 const map={
  YES_TAKER_CANDIDATE:{label:'YES Candidate (Taker)',tone:'up'},
  NO_TAKER_CANDIDATE:{label:'NO Candidate (Taker)',tone:'down'},
  YES_MAKER_WATCH:{label:'YES Limit Watch (Maker)',tone:'up'},
  NO_MAKER_WATCH:{label:'NO Limit Watch (Maker)',tone:'down'},
  WAIT_COMPRESSION:{label:'Wait · Min 10–13 Window',tone:'warn'},
  PASS:{label:'Pass · Below Fee Hurdle',tone:'neutral'},
 };
 return map[posture]||{label:'Waiting for live quotes',tone:'neutral'};
}

function signedCents(v){
 if(v==null||!Number.isFinite(Number(v)))return '—';
 const n=Number(v);
 return `${n>=0?'+':''}${n.toFixed(2)}¢`;
}

function signedPct(v){
 if(v==null||!Number.isFinite(Number(v)))return '—';
 const n=Number(v);
 return `${n>=0?'+':''}${n.toFixed(2)}%`;
}

function signedUsd(v){
 if(v==null||!Number.isFinite(Number(v)))return '—';
 const n=Number(v);
 return `${n>=0?'+$':'-$'}${Math.abs(n).toFixed(2)}`;
}

function pct(v){
 if(v==null||!Number.isFinite(Number(v)))return '—';
 return `${Number(v).toFixed(1)}%`;
}

function selectGridRow(sim,windowKey,mode,hurdleCents){
 const key=`${windowKey}|${mode}|${Number(hurdleCents).toFixed(1)}`;
 return sim?.interactive_grid?.[key]||null;
}

function filterAndSortAgents(agents,opts){
 const list=(agents||[]).filter(a=>{
  if(opts.family&&opts.family!=='ALL'&&a.family!==opts.family)return false;
  if(opts.mode&&opts.mode!=='ALL'&&a.execution_mode!==opts.mode)return false;
  if(opts.search){
   const q=opts.search.trim().toLowerCase();
   if(q&&!(`${a.id} ${a.name} ${a.family} ${a.execution_mode} ${a.sizing}`).toLowerCase().includes(q))return false;
  }
  return true;
 });
 const sortKey=opts.sort||'end_usd';
 list.sort((a,b)=>{
  if(sortKey==='max_drawdown_pct')return (a.max_drawdown_pct??999)-(b.max_drawdown_pct??999);
  return (b[sortKey]??-9999)-(a[sortKey]??-9999);
 });
 return list;
}

if(typeof module==='object'&&module.exports){
 module.exports={formatPosture,signedCents,signedPct,signedUsd,selectGridRow,filterAndSortAgents};
 return;
}

const $=id=>document.getElementById(id);
const el=(tag,text,cls)=>{const n=document.createElement(tag);if(text!=null)n.textContent=text;if(cls)n.className=cls;return n};

let homeActive=false,labActive=false,selectedAsset='BTC',signalMode='full',chartView='friction',selectedMinute=12;
let simData=null,simPromise=null,signalTimer=null,homeTimer=null,signalCtrl=null,homeCtrl=null,labGen=0,homeGen=0;
let latestSignals={};
const charts={};

const FAMILY_COLORS={
 'Multi-Factor Ensemble':'#2563eb',
 'Favorite-Longshot Bias':'#10b981',
 'Cross-Asset Lead-Lag':'#0ea5e9',
 'L2 Order-Book Microstructure':'#8b5cf6',
 'Session & News Catalysts':'#f59e0b',
 'Adverse-Selection Maker':'#14b8a6',
 'Impulse Mean-Reversion':'#ec4899',
 'Momentum & Volume Surge':'#f97316',
 'Black-Scholes Digital':'#6366f1',
 'Controls & Latency Stress':'#ef4444',
};

function themePalette(){
 const cs=getComputedStyle(document.documentElement);
 const get=(k,fb)=>(cs.getPropertyValue(k)||'').trim()||fb;
 const dark=document.documentElement.dataset.theme==='dark';
 return {
  dark,
  text:get('--text',dark?'#e8eef7':'#111827'),
  muted:get('--muted',dark?'#92a1b5':'#64748b'),
  grid:dark?'rgba(148,163,184,0.14)':'rgba(15,23,42,0.09)',
  surface:get('--surface-elev',dark?'#131b26':'#ffffff'),
  accent:get('--accent',dark?'#60a5fa':'#2563eb'),
  up:get('--up',dark?'#34d399':'#059669'),
  down:get('--down',dark?'#f87171':'#dc2626'),
  warn:get('--warn',dark?'#fbbf24':'#d97706'),
  purple:dark?'#a78bfa':'#7c3aed',
  cyan:dark?'#22d3ee':'#0891b2',
 };
}

function mountChart(key,canvasId,config){
 if(typeof root.Chart!=='function')return null;
 const canvas=$(canvasId);
 if(!canvas)return null;
 if(charts[key]){try{charts[key].destroy()}catch{}}
 charts[key]=new root.Chart(canvas.getContext('2d'),config);
 return charts[key];
}

function buildTimeline(host,activeMinute){
 if(!host)return;
 host.replaceChildren();
 for(let m=1;m<=15;m++){
  const slot=el('span',`M${m}`,'hq-min-slot');
  if(m>=10&&m<=13)slot.dataset.zone='optimal';
  else if(m>=14)slot.dataset.zone='final';
  if(activeMinute&&m===activeMinute)slot.dataset.active='true';
  else if(activeMinute&&m<activeMinute)slot.dataset.elapsed='true';
  slot.title=`Minute ${m}${m>=10&&m<=13?' · Optimal compression window (lowest spread & fee)':m>=14?' · Final-minute averaging window':' · Early/mid discovery'}`;
  host.append(slot);
 }
}

function renderHomeBanner(sig){
 const strip=$('home-quant-strip');
 if(!strip)return;
 if(!sig||!sig.hybrid){
  buildTimeline($('hq-min-track'),null);
  return;
 }
 const p=formatPosture(sig.hybrid.posture);
 const verdict=$('hq-verdict');
 verdict.textContent=p.label;
 verdict.dataset.side=sig.hybrid.side||'NONE';
 $('hq-reason').textContent=sig.hybrid.reason||'';

 const latPill=$('hq-gemini-pill');
 const lat=sig.latency||{};
 if(sig.gemini?.status==='live'){
  const srvTxt=lat.total_server_ms!=null?` · srv ${lat.total_server_ms.toFixed(1)}ms`:'';
  latPill.textContent=`gemini-decision-1 · ${sig.gemini.latency_ms}ms${srvTxt} · 0 out tok`;
  latPill.dataset.state='live';
 } else {
  latPill.textContent=`Quant fast-path (${lat.total_server_ms!=null?`${lat.total_server_ms.toFixed(1)}ms`:'active'}) · Gemini ${sig.gemini?.status||'standby'}`;
  latPill.dataset.state='fallback';
 }

 $('hq-market-mid').textContent=`${sig.market.midpoint_cents.toFixed(1)}¢`;
 $('hq-market-meta').textContent=`YES ${sig.market.yes_ask_cents.toFixed(1)}¢ · NO ${sig.market.no_ask_cents.toFixed(1)}¢ · spread ${sig.market.spread_cents.toFixed(2)}¢`;
 $('hq-hybrid-prob').textContent=pct(sig.hybrid.p_yes_pct);
 $('hq-hybrid-meta').textContent=`${signedCents(sig.hybrid.delta_vs_mid_pp)} vs mid · Quant ${pct(sig.quant.p_yes_pct)}`;
 const bestTkr=Math.max(sig.hybrid.taker_ev_yes_cents,sig.hybrid.taker_ev_no_cents);
 const bestMkr=Math.max(sig.hybrid.maker_ev_yes_cents,sig.hybrid.maker_ev_no_cents);
 $('hq-ev').textContent=signedCents(bestTkr);
 $('hq-ev').className=bestTkr>=0?'q-pos':'q-neg';
 $('hq-ev-meta').textContent=`Maker EV ${signedCents(bestMkr)} · Taker fee ${sig.fees.taker_yes_cents.toFixed(2)}¢`;
 $('hq-phase-text').textContent=`Minute ${sig.elapsed_minute} of 15 · ${sig.window_phase_label}`;
 buildTimeline($('hq-min-track'),sig.elapsed_minute);
}

async function fetchSignal(asset,ctrl,opts){
 const q=new URLSearchParams({asset});
 if(opts?.mode==='fast')q.set('mode','fast');
 if(opts?.refresh)q.set('refresh','1');
 const t0=performance.now();
 const r=await fetch(`/api/quant/signal?${q.toString()}`,{cache:'no-store',signal:ctrl.signal});
 if(!r.ok)throw new Error(`HTTP ${r.status}`);
 const data=await r.json();
 data._client_rtt_ms=Math.round(performance.now()-t0);
 return data;
}

async function pollHomeBanner(){
 clearTimeout(homeTimer);
 if(!homeActive)return;
 const g=homeGen;
 homeCtrl?.abort();
 homeCtrl=new AbortController();
 try{
  const sig=await fetchSignal('BTC',homeCtrl,{mode:'full'});
  if(homeActive&&g===homeGen){latestSignals.BTC=sig;renderHomeBanner(sig)}
 }catch{
  // Keep previous reading
 }finally{
  if(homeActive&&g===homeGen)homeTimer=setTimeout(pollHomeBanner,document.hidden?45000:12000);
 }
}

function renderLatencyStrip(sig){
 const strip=$('ql-lat-strip');
 if(!strip)return;
 strip.replaceChildren();
 const l=sig.latency;
 if(!l)return;
 const items=[
  {k:'Server Total',v:`${l.total_server_ms.toFixed(2)}ms`,fast:l.total_server_ms<15},
  {k:'Rust Quant',v:`${l.quant_compute_us}µs`,fast:true},
  {k:'Feed Fetch',v:`${l.data_fetch_ms.toFixed(1)}ms`,fast:l.data_fetch_ms<5},
  {k:'Gemini Overlay',v:sig.gemini?.latency_ms!=null?`${sig.gemini.latency_ms}ms (${l.cache_status})`:l.cache_status,fast:l.cache_status==='warm_overlay'||l.cache_status==='swr_refreshing'||l.cache_status==='fast_path'},
  {k:'Server p50 / p95',v:`${l.server_p50_ms.toFixed(1)} / ${l.server_p95_ms.toFixed(1)}ms (n=${l.sample_count})`,fast:l.server_p50_ms<20},
  {k:'Browser RTT',v:sig._client_rtt_ms!=null?`${sig._client_rtt_ms}ms`:'—',fast:(sig._client_rtt_ms||999)<220},
 ];
 for(const it of items){
  const chip=el('span',null,'q-lat-chip');
  if(it.fast)chip.dataset.fast='true';
  chip.append(document.createTextNode(`${it.k}: `),el('strong',it.v));
  strip.append(chip);
 }
}

function renderLiveDesk(sig){
 if(!sig||!sig.hybrid)return;
 const p=formatPosture(sig.hybrid.posture);
 $('ql-phase-label').textContent=`Minute ${sig.elapsed_minute} of 15 (${Math.floor(sig.remaining_seconds/60)}m ${sig.remaining_seconds%60}s remaining) · ${sig.window_phase_label}`;
 $('ql-ticker-pill').textContent=`${sig.ticker} · ${sig.quote_source.replaceAll('_',' ')}`;
 const post=$('ql-posture-pill');
 post.textContent=p.label;
 post.dataset.tone=p.tone;
 renderLatencyStrip(sig);
 buildTimeline($('ql-min-track'),sig.elapsed_minute);

 $('ql-mid-val').textContent=`${sig.market.midpoint_cents.toFixed(1)}¢`;
 $('ql-mid-meta').textContent=`YES ask ${sig.market.yes_ask_cents.toFixed(1)}¢ · NO ask ${sig.market.no_ask_cents.toFixed(1)}¢ · spread ${sig.market.spread_cents.toFixed(2)}¢`;
 $('ql-quant-val').textContent=pct(sig.quant.p_yes_pct);
 $('ql-quant-meta').textContent=`Residual ${signedCents(sig.quant.delta_vs_mid_pp)} vs mid · Minute ${sig.elapsed_minute}/15 weights`;

 const gPill=$('ql-gem-pill');
 if(sig.gemini?.status==='live'){
  gPill.textContent=`${sig.gemini.latency_ms}ms · ${sig.gemini.input_tokens||0} in / 0 out`;
  gPill.dataset.tone='up';
  $('ql-gem-val').textContent=pct((sig.gemini.settle_yes_prob||0)*100);
  const act=(sig.gemini.action_choice||'wait_or_pass').replaceAll('_',' ');
  const tim=sig.gemini.timing_score!=null?`${sig.gemini.timing_score.toFixed(2)}/2.0`:'—';
  const aln=(sig.gemini.catalyst_alignment||'neutral').replaceAll('_',' ');
  $('ql-gem-meta').textContent=`Action: ${act} · Timing ${tim} · Alignment: ${aln}`;
 } else {
  gPill.textContent=sig.gemini?.status||'standby';
  gPill.dataset.tone='warn';
  $('ql-gem-val').textContent=sig.gemini?.status==='warming_async'?'Warming…':'Standby';
  $('ql-gem-meta').textContent='Sub-millisecond Rust quant engine active; Gemini overlay warms asynchronously without blocking quotes.';
 }

 $('ql-hyb-val').textContent=pct(sig.hybrid.p_yes_pct);
 $('ql-hyb-meta').textContent=sig.hybrid.reason;

 const evHost=$('ql-ev-rows');
 evHost.replaceChildren();
 const evItems=[
  {label:'YES · Taker (Cross Ask)',price:`${sig.market.yes_ask_cents.toFixed(1)}¢`,fee:`Fee ${sig.fees.taker_yes_cents.toFixed(2)}¢`,ev:sig.hybrid.taker_ev_yes_cents},
  {label:'NO · Taker (Cross Ask)',price:`${sig.market.no_ask_cents.toFixed(1)}¢`,fee:`Fee ${sig.fees.taker_no_cents.toFixed(2)}¢`,ev:sig.hybrid.taker_ev_no_cents},
  {label:'YES · Maker (Mid Limit)',price:`${sig.market.midpoint_cents.toFixed(1)}¢`,fee:`Fee ${sig.fees.maker_yes_cents.toFixed(2)}¢`,ev:sig.hybrid.maker_ev_yes_cents},
  {label:'NO · Maker (Mid Limit)',price:`${(100-sig.market.midpoint_cents).toFixed(1)}¢`,fee:`Fee ${sig.fees.maker_no_cents.toFixed(2)}¢`,ev:sig.hybrid.maker_ev_no_cents},
 ];
 for(const item of evItems){
  const row=el('div',null,'q-ev-row');
  row.append(
   el('span',item.label),
   el('span',item.price),
   el('span',item.fee),
   el('strong',signedCents(item.ev),item.ev>=0?'q-pos':'q-neg')
  );
  evHost.append(row);
 }

 const facHost=$('ql-factors');
 facHost.replaceChildren();
 for(const f of (sig.factors||[])){
  const wrap=el('div',null,'q-factor');
  const top=el('div',null,'q-factor-top');
  top.append(el('span',f.label),el('code',`${f.raw_display} (${f.logit_delta>=0?'+':''}${f.logit_delta.toFixed(3)} logit)`));
  const track=el('div',null,'q-bar-track');
  track.append(el('span',null,'q-bar-center'));
  const fill=el('span',null,'q-bar-fill');
  const mag=Math.min(50,Math.abs(f.logit_delta)/0.25*50);
  if(f.logit_delta>=0){fill.style.insetInlineStart='50%';fill.style.inlineSize=`${mag}%`;fill.style.background='var(--up)'}
  else{fill.style.insetInlineStart=`${50-mag}%`;fill.style.inlineSize=`${mag}%`;fill.style.background='var(--down)'}
  track.append(fill);
  wrap.append(top,track);
  facHost.append(wrap);
 }
 $('ql-packet-pre').textContent=JSON.stringify(sig.blinded_packet,null,2);
}

/* =========================================================================
   02 / 100-AGENT $100 PARALLEL PAPER-TRADING TOURNAMENT (Chart.js 4.4.7)
   ========================================================================= */
function renderTournament100(sim){
 const t=sim?.tournament_100;
 if(!t)return;
 const tp=themePalette();
 $('ql-tourney-note').textContent=t.multiple_testing_note;

 const pills=$('ql-tourney-pills');
 pills.replaceChildren(
  el('span',`${t.total_agents} Agents · $${t.start_bankroll_usd} Start`,'q-pill'),
  el('span',`${t.profitable_agents}/100 Profitable · ${t.fdr_significant_agents_5pct}/100 FDR q≤0.05`,'q-pill'),
  el('span',`${t.simulation_wall_seconds}s on ${t.parallel_workers} CPU cores`,'q-pill')
 );

 const kpis=$('ql-tourney-kpis');
 kpis.replaceChildren();
 const bt=t.best_taker_30plus||t.best_overall;
 const bo=t.best_overall;
 const bm=t.best_touch_maker_20plus;
 const cards=[
  {lbl:'Best Realistic Taker (n≥30)',pill:`${bt.win_rate_pct}% Win`,tone:'up',val:`$${bt.end_usd.toFixed(2)}`,meta:`${bt.name} · ${signedPct(bt.roi_pct)} · ${bt.trades} trades · ${signedCents(bt.ev_per_trade_cents)}/tr · DD ${bt.max_drawdown_pct}% · Sharpe ${bt.sharpe}`},
  {lbl:'Best Optimistic Mid-Maker',pill:`Sharpe ${bo.sharpe}`,tone:'accent',val:`$${bo.end_usd.toFixed(2)}`,meta:`${bo.name} · ${signedPct(bo.roi_pct)} · assumes zero adverse-selection queue loss`},
  {lbl:'Adverse-Selection Touch-Maker',pill:'Realistic Limit',tone:'warn',val:`$${bm.end_usd.toFixed(2)}`,meta:`Best of 10 Touch-Maker agents (${bm.name}) · proves resting limit adverse selection costs ~3–4¢/tr`},
  {lbl:'100-Agent Distribution ($100 Start)',pill:`${t.profitable_agents}/100 > $100`,tone:'neutral',val:`$${t.median_end_usd.toFixed(2)}`,meta:`Median $${t.median_end_usd} · Mean $${t.mean_end_usd} · P90 $${t.p90_end_usd} · P10 $${t.p10_end_usd} · BH FDR q≤0.05: ${t.fdr_significant_agents_5pct}/100`},
 ];
 for(const c of cards){
  const art=el('article',null,'q-kpi');
  const top=el('div',null,'q-kpi-label');
  const pill=el('span',c.pill,'q-pill');pill.dataset.tone=c.tone;
  top.append(el('span',c.lbl),pill);
  art.append(top,el('div',c.val,'q-kpi-val'),el('div',c.meta,'q-kpi-meta'));
  kpis.append(art);
 }

 // Populate Family dropdown once
 const famSel=$('ql-filter-family');
 if(famSel&&famSel.options.length<=1){
  for(const fs of (t.family_summary||[])){
   const opt=document.createElement('option');
   opt.value=fs.family;
   opt.textContent=`${fs.family} (${fs.profitable_count}/${fs.agent_count} > $100)`;
   famSel.append(opt);
  }
 }

 // Chart 1: All 100 Agents Ending Bankroll Bar Distribution
 const agents=t.agents||[];
 mountChart('dist100','ql-chart-100dist',{
  type:'bar',
  data:{
   labels:agents.map(a=>`#${a.rank} ${a.id}`),
   datasets:[
    {
     label:'Ending Bankroll ($)',
     data:agents.map(a=>a.end_usd),
     backgroundColor:agents.map(a=>FAMILY_COLORS[a.family]||tp.accent),
     borderRadius:3,
     barPercentage:0.88,
     categoryPercentage:0.94,
    },
    {
     type:'line',
     label:'$100.00 Start Break-Even',
     data:agents.map(()=>100.0),
     borderColor:tp.text,
     borderWidth:1.5,
     borderDash:[5,4],
     pointRadius:0,
     fill:false,
    }
   ]
  },
  options:{
   responsive:true,
   maintainAspectRatio:false,
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:{labels:{color:tp.muted,boxWidth:12,font:{size:11}}},
    tooltip:{
     backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1,
     callbacks:{
      title:(items)=>{
       const a=agents[items[0].dataIndex];
       return a?`#${a.rank} ${a.name}`:'';
      },
      label:(ctx)=>{
       if(ctx.datasetIndex===1)return 'Start Bankroll: $100.00';
       const a=agents[ctx.dataIndex];
       if(!a)return '';
       return [
        `Family: ${a.family} (${a.execution_mode} · ${a.sizing})`,
        `End Bankroll: $${a.end_usd.toFixed(2)} (${signedPct(a.roi_pct)})`,
        `Trades: ${a.trades} (Pass ${a.pass_rate_pct}%) · Win: ${a.win_rate_pct}%`,
        `EV/Trade: ${signedCents(a.ev_per_trade_cents)} · Max DD: ${a.max_drawdown_pct}% · Sharpe: ${a.sharpe}`,
        `Bootstrap p=${a.p_value} · BH FDR q=${a.fdr_q_value}`
       ];
      }
     }
    }
   },
   scales:{
    x:{ticks:{color:tp.muted,maxTicksLimit:16,font:{size:10}},grid:{display:false}},
    y:{ticks:{color:tp.muted,callback:v=>`$${v}`,font:{size:10}},grid:{color:tp.grid},suggestedMin:50,suggestedMax:120}
   }
  }
 });

 // Charts 2 & 3: Top Paper Equity Curves ($100 Start) & Synchronized Underwater Drawdown Curves
 const curves=t.featured_curves||[];
 const picks=[];
 const wantSub=['FLB Fav 72-88c + XAsset Min11-13','6-Factor L2 Min12-14 Taker 0.6c','6-Factor L2 Min12-14 Mid-Maker 1.5c','Touch-Maker BS-Digital Min10-13 Mid','Naive Mid Favorite','Naive Early Favorite'];
 for(const sub of wantSub){
  const found=curves.find(c=>c.name.includes(sub));
  if(found)picks.push(found);
 }
 if(picks.length<4){
  for(const c of curves.slice(0,5))if(!picks.includes(c))picks.push(c);
 }
 const palette=[tp.up,tp.accent,tp.purple,tp.warn,'#f97316',tp.down];
 const maxLen=Math.max(2,...picks.map(c=>(c.equity_curve_usd||[]).length));
 const xLabels=Array.from({length:maxLen},(_,i)=>i===0?'Start':`${Math.round(i/(maxLen-1)*100)}%`);
 const resample=(arr,n)=>{
  if(!arr||!arr.length)return Array(n).fill(100);
  if(arr.length===n)return arr;
  return Array.from({length:n},(_,i)=>arr[Math.min(arr.length-1,Math.round(i/(n-1)*(arr.length-1)))]);
 };

 mountChart('equity','ql-chart-equity',{
  type:'line',
  data:{
   labels:xLabels,
   datasets:picks.map((c,i)=>({
    label:`${c.name.slice(0,32)} ($${c.end_usd.toFixed(2)})`,
    data:resample(c.equity_curve_usd,maxLen),
    borderColor:palette[i%palette.length],
    backgroundColor:'transparent',
    borderWidth:i<2?2.4:1.7,
    borderDash:c.name.includes('Naive')?[5,4]:[],
    pointRadius:0,
    tension:0.2,
   }))
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:{position:'bottom',labels:{color:tp.muted,boxWidth:10,font:{size:10}}},
    tooltip:{backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1,callbacks:{label:c=>`${c.dataset.label.split(' (')[0]}: $${Number(c.parsed.y).toFixed(2)}`}}
   },
   scales:{
    x:{ticks:{color:tp.muted,maxTicksLimit:8,font:{size:10}},grid:{display:false}},
    y:{ticks:{color:tp.muted,callback:v=>`$${v}`,font:{size:10}},grid:{color:tp.grid}}
   }
  }
 });

 mountChart('drawdown','ql-chart-dd',{
  type:'line',
  data:{
   labels:xLabels,
   datasets:picks.map((c,i)=>({
    label:c.name.slice(0,28),
    data:resample(c.drawdown_curve_pct,maxLen),
    borderColor:palette[i%palette.length],
    backgroundColor:i===0?`${palette[0]}1f`:'transparent',
    borderWidth:i<2?2.2:1.5,
    borderDash:c.name.includes('Naive')?[5,4]:[],
    pointRadius:0,
    fill:i===0,
    tension:0.2,
   }))
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:{position:'bottom',labels:{color:tp.muted,boxWidth:10,font:{size:10}}},
    tooltip:{backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1,callbacks:{label:c=>`${c.dataset.label}: ${Number(c.parsed.y).toFixed(1)}%`}}
   },
   scales:{
    x:{ticks:{color:tp.muted,maxTicksLimit:8,font:{size:10}},grid:{display:false}},
    y:{ticks:{color:tp.muted,callback:v=>`${v}%`,font:{size:10}},grid:{color:tp.grid},max:0}
   }
  }
 });

 render100Table(sim);

 // 10-Family Championship Summary
 const famBody=$('ql-family-rows');
 if(famBody){
  famBody.replaceChildren();
  for(const fs of (t.family_summary||[])){
   const tr=document.createElement('tr');
   const c1=document.createElement('td');c1.append(el('strong',fs.family));
   const c2=el('td',`${fs.profitable_count} / ${fs.agent_count}`);
   const c3=el('td',`$${fs.median_end_usd.toFixed(2)}`);
   const c4=el('td',`$${fs.mean_end_usd.toFixed(2)}`);
   const c5=el('td',fs.best_agent_name);
   const c6=el('td',`$${fs.best_end_usd.toFixed(2)} (${signedPct(fs.best_roi_pct)})`,fs.best_end_usd>=100?'q-pos':'q-neg');
   const c7=el('td',`${fs.best_win_rate_pct.toFixed(1)}%`);
   const c8=el('td',String(fs.best_trades));
   tr.append(c1,c2,c3,c4,c5,c6,c7,c8);
   famBody.append(tr);
  }
 }
}

function render100Table(sim){
 const tbody=$('ql-100-rows');
 const t=sim?.tournament_100;
 if(!tbody||!t)return;
 const rows=filterAndSortAgents(t.agents,{
  family:$('ql-filter-family')?.value||'ALL',
  mode:$('ql-filter-mode')?.value||'ALL',
  sort:$('ql-filter-sort')?.value||'end_usd',
  search:$('ql-filter-search')?.value||'',
 });
 tbody.replaceChildren();
 for(const a of rows){
  const tr=document.createElement('tr');
  const td1=document.createElement('td');
  td1.append(el('strong',`#${a.rank} ${a.name}`),el('small',`${a.id} · ${a.window}`));
  const td2=el('td',a.family);
  const td3=el('td',`${a.execution_mode.replace('_',' ')} · ${a.sizing}`);
  const td4=el('td',`$${a.end_usd.toFixed(2)}`,a.end_usd>100?'q-pos':a.end_usd<100?'q-neg':'');
  const td5=el('td',`${signedUsd(a.net_pnl_usd)} (${signedPct(a.roi_pct)})`,a.net_pnl_usd>=0?'q-pos':'q-neg');
  const td6=el('td',`${a.trades} (${a.pass_rate_pct}%)`);
  const td7=el('td',`${a.win_rate_pct.toFixed(1)}%`);
  const td8=el('td',signedCents(a.ev_per_trade_cents),a.ev_per_trade_cents>=0?'q-pos':'q-neg');
  const td9=el('td',`-${a.max_drawdown_pct.toFixed(1)}%`);
  const td10=el('td',`${a.sharpe.toFixed(2)} / ${a.profit_factor.toFixed(2)}`);
  const td11=el('td',`p=${a.p_value.toFixed(3)} · q=${a.fdr_q_value.toFixed(2)}`);
  tr.append(td1,td2,td3,td4,td5,td6,td7,td8,td9,td10,td11);
  tbody.append(tr);
 }
 if(!rows.length){
  const tr=document.createElement('tr');
  const td=el('td','No agents match the active filter.');
  td.colSpan=11;
  tr.append(td);
  tbody.append(tr);
 }
}

/* =========================================================================
   03 / MINUTE 1->14 CHART.JS DUAL-AXIS COMBO + 14x6 EV HEATMAP
   ========================================================================= */
function renderMinuteChart(sim){
 const rows=sim?.minute_curve||[];
 if(!rows.length)return;
 const tp=themePalette();
 const labels=rows.map(r=>`M${r.minute}`);

 let cfg;
 if(chartView==='friction'){
  cfg={
   type:'bar',
   data:{
    labels,
    datasets:[
     {
      type:'bar',label:'Avg 1-Min Contract Volume',
      data:rows.map(r=>r.avg_volume),
      yAxisID:'yVol',
      backgroundColor:rows.map(r=>(r.minute>=10&&r.minute<=13)?`${tp.up}44`:`${tp.accent}28`),
      borderColor:rows.map(r=>(r.minute>=10&&r.minute<=13)?tp.up:tp.accent),
      borderWidth:1,borderRadius:4,order:3,
     },
     {
      type:'line',label:'Total Taker Friction (½ Spread + Fee, ¢)',
      data:rows.map(r=>r.total_taker_friction_cents),
      yAxisID:'yCents',borderColor:tp.down,backgroundColor:'transparent',
      borderWidth:2.6,pointRadius:4,tension:0.25,order:1,
     },
     {
      type:'line',label:'Kalshi Quadratic Taker Fee (¢)',
      data:rows.map(r=>r.avg_taker_fee_cents),
      yAxisID:'yCents',borderColor:tp.warn,borderDash:[5,4],
      borderWidth:2,pointRadius:3,tension:0.25,order:1,
     },
     {
      type:'line',label:'Avg Bid-Ask Spread (¢)',
      data:rows.map(r=>r.avg_spread_cents),
      yAxisID:'yCents',borderColor:tp.accent,
      borderWidth:2,pointRadius:3,tension:0.25,order:2,
     },
    ]
   },
   options:{
    responsive:true,maintainAspectRatio:false,
    interaction:{mode:'index',intersect:false},
    onClick:(_,els)=>{if(els.length){selectedMinute=rows[els[0].index].minute;renderMinuteReadout(sim)}},
    plugins:{
     legend:{position:'bottom',labels:{color:tp.muted,boxWidth:12,font:{size:11}}},
     tooltip:{backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1}
    },
    scales:{
     x:{ticks:{color:tp.muted,font:{size:11}},grid:{display:false}},
     yCents:{position:'left',ticks:{color:tp.muted,callback:v=>`${v}¢`,font:{size:10}},grid:{color:tp.grid},title:{display:true,text:'Friction (¢)',color:tp.muted}},
     yVol:{position:'right',ticks:{color:tp.muted,callback:v=>`${Math.round(v/1000)}k`,font:{size:10}},grid:{display:false},title:{display:true,text:'1m Volume',color:tp.muted}}
    }
   }
  };
 } else if(chartView==='ev'){
  cfg={
   type:'bar',
   data:{
    labels,
    datasets:[
     {label:'Quant Taker OOS Net PnL (¢)',data:rows.map(r=>r.taker_net_cents),backgroundColor:rows.map(r=>r.taker_net_cents>=0?`${tp.up}cc`:`${tp.down}bb`),borderRadius:4},
     {label:'Quant Mid-Maker OOS Net PnL (¢)',data:rows.map(r=>r.maker_net_cents),backgroundColor:rows.map(r=>r.maker_net_cents>=0?`${tp.accent}bb`:`${tp.warn}aa`),borderRadius:4},
    ]
   },
   options:{
    responsive:true,maintainAspectRatio:false,
    interaction:{mode:'index',intersect:false},
    onClick:(_,els)=>{if(els.length){selectedMinute=rows[els[0].index].minute;renderMinuteReadout(sim)}},
    plugins:{
     legend:{position:'bottom',labels:{color:tp.muted,boxWidth:12,font:{size:11}}},
     tooltip:{backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1}
    },
    scales:{
     x:{ticks:{color:tp.muted},grid:{display:false}},
     y:{ticks:{color:tp.muted,callback:v=>`${v}¢`},grid:{color:tp.grid}}
    }
   }
  };
 } else {
  cfg={
   type:'line',
   data:{
    labels,
    datasets:[
     {label:'Market Favorite Accuracy (%)',data:rows.map(r=>+(r.favorite_accuracy*100).toFixed(1)),yAxisID:'yAcc',borderColor:tp.accent,borderWidth:2.6,pointRadius:4,tension:0.25},
     {label:'Cross-Asset Brier Skill Score (%)',data:rows.map(r=>r.bss_xasset_pct),yAxisID:'yBss',borderColor:tp.up,borderDash:[5,3],borderWidth:2.2,pointRadius:4,tension:0.25},
    ]
   },
   options:{
    responsive:true,maintainAspectRatio:false,
    interaction:{mode:'index',intersect:false},
    onClick:(_,els)=>{if(els.length){selectedMinute=rows[els[0].index].minute;renderMinuteReadout(sim)}},
    plugins:{
     legend:{position:'bottom',labels:{color:tp.muted,boxWidth:12,font:{size:11}}},
     tooltip:{backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1}
    },
    scales:{
     x:{ticks:{color:tp.muted},grid:{display:false}},
     yAcc:{position:'left',ticks:{color:tp.muted,callback:v=>`${v}%`},grid:{color:tp.grid},min:55,max:95},
     yBss:{position:'right',ticks:{color:tp.muted,callback:v=>`${v>=0?'+':''}${v}%`},grid:{display:false}}
    }
   }
  };
 }
 mountChart('minute','ql-chart-minute',cfg);
 renderMinuteReadout(sim);
}

function renderMinuteReadout(sim){
 const host=$('ql-min-readout');
 const r=(sim?.minute_curve||[]).find(x=>x.minute===selectedMinute)||sim?.minute_curve?.[11];
 if(!host||!r)return;
 host.replaceChildren();
 const cells=[
  ['Selected Minute',`Min ${r.minute} (${r.remaining_min}m left)`],
  ['Avg / Median Spread',`${r.avg_spread_cents.toFixed(2)}¢ / ${r.median_spread_cents.toFixed(2)}¢`],
  ['Taker / Maker Fee',`${r.avg_taker_fee_cents.toFixed(2)}¢ / ${r.avg_maker_fee_cents.toFixed(2)}¢`],
  ['Total Taker Friction',`${r.total_taker_friction_cents.toFixed(2)}¢`],
  ['Favorite Accuracy',`${(r.favorite_accuracy*100).toFixed(1)}% (Brier ${r.brier_mid.toFixed(4)})`],
  ['Cross-Asset BSS',`${r.bss_xasset_pct>=0?'+':''}${r.bss_xasset_pct.toFixed(2)}%`],
  ['OOS Maker Net',`${signedCents(r.maker_net_cents)} (${r.maker_fills} fills)`],
  ['Avg 1m Volume',Number(r.avg_volume).toLocaleString()],
 ];
 for(const [k,v] of cells){
  const d=document.createElement('div');
  d.append(el('span',k),el('strong',v));
  host.append(d);
 }
}

function renderHeatmap(sim){
 const host=$('ql-heatmap-grid');
 const hm=sim?.advanced_diagnostics?.minute_hurdle_heatmap;
 if(!host||!hm?.length)return;
 host.replaceChildren();
 const tbl=el('table',null,'q-hm-table');
 const thead=document.createElement('thead');
 const htr=document.createElement('tr');
 htr.append(el('th','Minute'));
 for(const c of hm[0].cells)htr.append(el('th',`≥ ${c.hurdle_cents.toFixed(1)}¢`));
 thead.append(htr);
 const tbody=document.createElement('tbody');
 for(const row of hm){
  const tr=document.createElement('tr');
  tr.append(el('th',`Min ${row.minute}${row.minute>=10&&row.minute<=13?' ★':''}`));
  for(const c of row.cells){
   const td=el('td',null,'q-hm-cell');
   if(c.ev_cents==null){
    td.textContent='—';
    td.style.background='var(--surface-elev)';
    td.style.color='var(--muted)';
   } else {
    const clamped=Math.max(-6,Math.min(6,c.ev_cents));
    const alpha=0.14+Math.abs(clamped)/6*0.42;
    td.style.background=c.ev_cents>=0?`rgba(16,185,129,${alpha.toFixed(2)})`:`rgba(239,68,68,${alpha.toFixed(2)})`;
    td.style.color='var(--text)';
    td.append(document.createTextNode(signedCents(c.ev_cents)),el('small',`n=${c.trades}`));
    td.title=`Min ${row.minute}, hurdle ≥${c.hurdle_cents}¢: ${c.trades} trades, win ${c.win_rate_pct}%, net ${signedCents(c.net_cents)}`;
   }
   tr.append(td);
  }
  tbody.append(tr);
 }
 tbl.append(thead,tbody);
 host.append(tbl);
}

/* =========================================================================
   04 / LATENCY DECAY FRONTIER & 10-DECILE CALIBRATION RELIABILITY
   ========================================================================= */
function renderLatencyAndCalibration(sim){
 const diag=sim?.advanced_diagnostics;
 if(!diag)return;
 const tp=themePalette();

 // Latency Decay Frontier
 const lc=diag.latency_decay_frontier||[];
 mountChart('latency','ql-chart-latency',{
  type:'line',
  data:{
   labels:lc.map(r=>r.latency_ms>=1000?`${r.latency_ms/1000}s`:`${r.latency_ms}ms`),
   datasets:[
    {
     label:'After-Fee EV / Trade (¢)',
     data:lc.map(r=>r.ev_per_trade_cents),
     yAxisID:'yEv',
     borderColor:tp.up,backgroundColor:`${tp.up}26`,
     borderWidth:2.6,pointRadius:4.5,fill:true,tension:0.25,
    },
    {
     label:'Adverse Quote Drift Slippage (¢)',
     data:lc.map(r=>r.adverse_slippage_cents),
     yAxisID:'yEv',
     borderColor:tp.down,borderDash:[5,4],
     borderWidth:2,pointRadius:3.5,fill:false,tension:0.25,
    },
    {
     label:'$100 Paper Bankroll End ($)',
     data:lc.map(r=>r.end_usd),
     yAxisID:'yUsd',
     borderColor:tp.accent,borderWidth:2,pointRadius:3.5,fill:false,tension:0.25,
    },
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:{position:'bottom',labels:{color:tp.muted,boxWidth:11,font:{size:10}}},
    tooltip:{
     backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1,
     callbacks:{title:items=>lc[items[0].dataIndex]?.label||''}
    }
   },
   scales:{
    x:{ticks:{color:tp.muted,font:{size:10}},grid:{display:false}},
    yEv:{position:'left',ticks:{color:tp.muted,callback:v=>`${v}¢`,font:{size:10}},grid:{color:tp.grid}},
    yUsd:{position:'right',ticks:{color:tp.muted,callback:v=>`$${v}`,font:{size:10}},grid:{display:false}}
   }
  }
 });

 // 10-Decile Calibration & Favorite-Longshot Bias
 const cal=diag.calibration_curves||{};
 const bLabels=(cal.late_m12||[]).map(b=>b.bucket);
 mountChart('calibration','ql-chart-calibration',{
  type:'line',
  data:{
   labels:bLabels,
   datasets:[
    {label:'Perfect Calibration (45°)',data:[5,15,25,35,45,55,65,75,85,95],borderColor:tp.muted,borderDash:[4,4],borderWidth:1.5,pointRadius:0},
    {label:'Min 12 Compression',data:(cal.late_m12||[]).map(b=>b.empirical_yes_pct),borderColor:tp.up,borderWidth:2.6,pointRadius:4,tension:0.15},
    {label:'Min 8 Mid Discovery',data:(cal.mid_m8||[]).map(b=>b.empirical_yes_pct),borderColor:tp.accent,borderWidth:2,pointRadius:3.5,tension:0.15},
    {label:'Min 3 Early Friction',data:(cal.early_m3||[]).map(b=>b.empirical_yes_pct),borderColor:tp.warn,borderWidth:1.8,pointRadius:3,tension:0.15},
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:{position:'bottom',labels:{color:tp.muted,boxWidth:11,font:{size:10}}},
    tooltip:{backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1,callbacks:{label:c=>c.parsed.y!=null?`${c.dataset.label}: ${c.parsed.y.toFixed(1)}% YES`:`${c.dataset.label}: n/a`}}
   },
   scales:{
    x:{ticks:{color:tp.muted,font:{size:10}},grid:{display:false}},
    y:{min:0,max:100,ticks:{color:tp.muted,callback:v=>`${v}%`,font:{size:10}},grid:{color:tp.grid}}
   }
  }
 });
}

/* =========================================================================
   05 / 24-HOUR UTC SESSION PROFILE & CROSS-ASSET COINTEGRATION
   ========================================================================= */
function renderSessionsAndCrossAsset(sim){
 const diag=sim?.advanced_diagnostics;
 if(!diag)return;
 const tp=themePalette();
 const xa=diag.cross_asset_structure;
 const host=$('ql-xasset-cards');
 if(host&&xa){
  host.replaceChildren();
  const cards=[
   ['Shared 3-Asset Windows',String(xa.common_windows)],
   ['BTC ↔ ETH Settlement Corr',`${xa.settlement_corr.BTC_ETH} (${xa.same_outcome_pct.BTC_ETH}% same)`],
   ['BTC ↔ SOL Settlement Corr',`${xa.settlement_corr.BTC_SOL} (${xa.same_outcome_pct.BTC_SOL}% same)`],
   ['ETH ↔ SOL Settlement Corr',`${xa.settlement_corr.ETH_SOL} (${xa.same_outcome_pct.ETH_SOL}% same)`],
   ['All 3 Assets Same Side',`${xa.same_outcome_pct.ALL_3}%`],
   ['Avg 1m BTC Move (Min 10–13)',`${xa.empirical_1m_btc_move_cents}¢ (${xa.empirical_45s_open_drift_cents}¢ / 45s)`],
  ];
  for(const [k,v] of cards){
   const d=document.createElement('div');
   d.append(el('span',k),el('strong',v));
   host.append(d);
  }
 }

 const hrs=diag.utc_session_profile||[];
 mountChart('sessions','ql-chart-sessions',{
  type:'bar',
  data:{
   labels:hrs.map(h=>h.label),
   datasets:[
    {
     type:'bar',label:'Avg Min-12 Volume',
     data:hrs.map(h=>h.avg_volume_m12),
     yAxisID:'yVol',
     backgroundColor:hrs.map(h=>h.session==='US Cash'?`${tp.accent}55`:h.session==='Europe'?`${tp.up}44`:`${tp.muted}33`),
     borderRadius:3,
    },
    {
     type:'line',label:'Avg Min-12 Spread (¢)',
     data:hrs.map(h=>h.avg_spread_m12_cents),
     yAxisID:'ySprd',
     borderColor:tp.warn,borderWidth:2.2,pointRadius:3,tension:0.25,
    },
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:{position:'bottom',labels:{color:tp.muted,boxWidth:11,font:{size:11}}},
    tooltip:{
     backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1,
     callbacks:{title:items=>`${hrs[items[0].dataIndex]?.label} UTC · ${hrs[items[0].dataIndex]?.session} Session`}
    }
   },
   scales:{
    x:{ticks:{color:tp.muted,font:{size:10}},grid:{display:false}},
    yVol:{position:'left',ticks:{color:tp.muted,callback:v=>`${Math.round(v/1000)}k`,font:{size:10}},grid:{color:tp.grid}},
    ySprd:{position:'right',ticks:{color:tp.muted,callback:v=>`${v}¢`,font:{size:10}},grid:{display:false}}
   }
  }
 });
}

/* =========================================================================
   06 / INTERACTIVE SIMULATOR & 07 / GEMINI DECISION 1 + TAVILY AUDIT
   ========================================================================= */
const HURDLE_STEPS=[0.0,0.3,0.6,1.0,1.5,2.0];
function renderSimulator(sim){
 const win=$('ql-sim-window')?.value||'compression_11_13';
 const mode=$('ql-sim-mode')?.value||'taker';
 const idx=Number($('ql-sim-hurdle')?.value??1);
 const hurdle=HURDLE_STEPS[idx]??0.3;
 if($('ql-sim-hurdle-val'))$('ql-sim-hurdle-val').textContent=`${hurdle.toFixed(1)}¢`;
 const row=selectGridRow(sim,win,mode,hurdle);
 const host=$('ql-sim-readout');
 if(!host||!row)return;
 host.replaceChildren();
 const items=[
  ['Trades Taken',`${row.trades} / 1,050 (PASS ${row.pass_rate_pct.toFixed(1)}%)`],
  ['Avg Entry Min',`Min ${row.avg_entry_minute.toFixed(1)}`],
  ['Win Rate',`${row.win_rate_pct.toFixed(1)}%`],
  ['Fees Paid',`$${(row.fees_paid_cents/100).toFixed(2)}`],
  ['Net PnL (1-ct)',signedUsd(row.net_pnl_cents/100),row.net_pnl_cents>=0?'q-pos':'q-neg'],
  ['EV / Trade',signedCents(row.ev_per_trade_cents),row.ev_per_trade_cents>=0?'q-pos':'q-neg'],
  ['95% Bootstrap CI',`[${signedCents(row.ci95_lo_cents)}, ${signedCents(row.ci95_hi_cents)}]`],
  ['Max Drawdown',`$${(row.max_drawdown_cents/100).toFixed(2)} (ROI ${signedPct(row.roi_pct)})`],
 ];
 for(const [k,v,cls] of items){
  const d=document.createElement('div');
  d.append(el('span',k),el('strong',v,cls));
  host.append(d);
 }
}

function renderGeminiAudit(sim){
 const g=sim?.gemini_decisions_benchmark;
 if(!g)return;
 const tp=themePalette();
 const pills=$('ql-bench-pills');
 pills.replaceChildren(
  el('span',`${g.total_live_calls} live calls · ${g.errors} errors`,'q-pill'),
  el('span',`p50 ${g.latency_ms.p50}ms · p95 ${g.latency_ms.p95}ms`,'q-pill'),
  el('span',`0 output tokens · $${g.usage.cost_usd_total.toFixed(4)} total`,'q-pill')
 );

 const mins=['4','9','12'];
 mountChart('gemini','ql-chart-gemini',{
  type:'bar',
  data:{
   labels:['Minute 4 (Early)','Minute 9 (Mid)','Minute 12 (Compression)'],
   datasets:[
    {label:'Kalshi Midpoint Brier',data:mins.map(m=>g.by_minute[m]?.brier_mid),backgroundColor:`${tp.muted}88`,borderRadius:4},
    {label:'6-Factor Quant Brier',data:mins.map(m=>g.by_minute[m]?.brier_quant),backgroundColor:`${tp.accent}bb`,borderRadius:4},
    {label:'Gemini Decision 1 Raw',data:mins.map(m=>g.by_minute[m]?.brier_gemini_raw),backgroundColor:`${tp.warn}bb`,borderRadius:4},
    {label:'Calibrated Hybrid (Lower=Better)',data:mins.map(m=>g.by_minute[m]?.brier_hybrid),backgroundColor:`${tp.up}dd`,borderRadius:4},
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   plugins:{
    legend:{position:'bottom',labels:{color:tp.muted,boxWidth:11,font:{size:10}}},
    tooltip:{backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1}
   },
   scales:{
    x:{ticks:{color:tp.muted},grid:{display:false}},
    y:{ticks:{color:tp.muted},grid:{color:tp.grid},min:0.12,title:{display:true,text:'Brier Score (Lower is Better)',color:tp.muted}}
   }
  }
 });

 const tbody=$('ql-gemini-rows');
 tbody.replaceChildren();
 for(const m of mins){
  const r=g.by_minute[m];
  if(!r)continue;
  const tr=document.createElement('tr');
  tr.append(
   el('td',`Minute ${m} (n=${r.n} unseen)`),
   el('td',r.brier_mid.toFixed(5)),
   el('td',r.brier_quant.toFixed(5)),
   el('td',r.brier_gemini_raw.toFixed(5)),
   el('td',r.brier_hybrid.toFixed(5),r.brier_hybrid<r.brier_mid?'q-pos':''),
   el('td',`${(r.acc_hybrid*100).toFixed(1)}%`),
   el('td',`${r.avg_timing_score.toFixed(2)} / 2.0`),
   el('td',`PASS ${r.actions?.wait_or_pass||0} · YES ${r.actions?.bet_yes||0} · NO ${r.actions?.bet_no||0}`)
  );
  tbody.append(tr);
 }

 const tn=g.tavily_news_scoring;
 if(tn){
  $('ql-news-bench-summary').textContent=`Single-pass Tavily screening: ${tn.questions_in_single_pass} questions across ${tn.stories_scored} live stories in ONE ${tn.latency_ms}ms call (${tn.input_tokens.toLocaleString()} input tokens, 0 output tokens) — caught and removed ${tn.spam_filtered}/${tn.stories_scored} SEO converter & off-topic false-positive stories.`;
  const nb=$('ql-news-bench-rows');
  nb.replaceChildren();
  for(const ex of (tn.examples||[])){
   const tr=document.createElement('tr');
   const td1=document.createElement('td');
   td1.append(el('strong',ex.title),el('small',ex.publisher));
   const td2=el('td',ex.old_tone);
   const badge=el('span',ex.filtered_spam?'Filtered Noise':ex.direction,'q-pill');
   badge.dataset.tone=ex.filtered_spam?'warn':ex.direction==='bullish'?'up':ex.direction==='bearish'?'down':'neutral';
   const td3=document.createElement('td');td3.append(badge);
   tr.append(td1,td2,td3,el('td',`${(ex.relevant_prob*100).toFixed(1)}%`),el('td',`${ex.impact_score.toFixed(2)} / 2.0`));
   nb.append(tr);
  }
 }
}

function renderSimulationAll(sim){
 if(!sim)return;
 const ds=sim.dataset||{};
 $('ql-dataset-meta').textContent=`${Number(ds.total_settled_windows||0).toLocaleString()} settled BTC 15m contracts (${ds.date_start_utc} → ${ds.date_end_utc}) · ${Number(ds.oos_windows||0).toLocaleString()} strictly out-of-sample walk-forward windows · 1h embargo`;
 const fg=$('ql-findings');
 fg.replaceChildren();
 const kf=sim.key_findings||{};
 const cards=[
  ['Optimal Window: '+kf.optimal_window,kf.why_optimal],
  ['The Early-Window Fee Trap (Min 1–6)',kf.early_window_trap],
  ['Strongest Structural Signal',kf.strongest_alpha_signal],
  ['Honest Risk & Latency Reality',kf.honest_risk_note],
 ];
 for(const [title,body] of cards){
  const c=el('article',null,'q-finding');
  c.append(el('strong',title),el('p',body));
  fg.append(c);
 }
 renderTournament100(sim);
 renderMinuteChart(sim);
 renderHeatmap(sim);
 renderLatencyAndCalibration(sim);
 renderSessionsAndCrossAsset(sim);
 renderSimulator(sim);
 renderGeminiAudit(sim);
}

async function ensureSimulation(){
 if(simData)return simData;
 if(!simPromise){
  simPromise=fetch('/api/quant/simulation').then(r=>{
   if(!r.ok)throw new Error(`HTTP ${r.status}`);
   return r.json();
  }).then(d=>{simData=d;return d}).catch(err=>{simPromise=null;throw err});
 }
 return simPromise;
}

async function pollLabSignal(forceRefresh){
 clearTimeout(signalTimer);
 if(!labActive)return;
 const g=labGen;
 signalCtrl?.abort();
 signalCtrl=new AbortController();
 try{
  const sig=await fetchSignal(selectedAsset,signalCtrl,{mode:signalMode,refresh:forceRefresh?1:0});
  if(labActive&&g===labGen){
   latestSignals[selectedAsset]=sig;
   renderLiveDesk(sig);
   if(selectedAsset==='BTC')renderHomeBanner(sig);
  }
 }catch(err){
  if(err?.name!=='AbortError'&&labActive&&g===labGen){
   $('ql-phase-label').textContent='Live market feed temporarily unavailable (retrying automatically)…';
  }
 }finally{
  if(labActive&&g===labGen)signalTimer=setTimeout(()=>pollLabSignal(false),document.hidden?30000:signalMode==='fast'?4000:10000);
 }
}

function wireControls(){
 for(const btn of document.querySelectorAll('[data-quant-asset]')){
  btn.addEventListener('click',()=>{
   selectedAsset=btn.dataset.quantAsset||'BTC';
   for(const b of document.querySelectorAll('[data-quant-asset]'))b.setAttribute('aria-pressed',String(b===btn));
   if(latestSignals[selectedAsset])renderLiveDesk(latestSignals[selectedAsset]);
   pollLabSignal(false);
  });
 }
 for(const btn of document.querySelectorAll('[data-quant-mode]')){
  btn.addEventListener('click',()=>{
   signalMode=btn.dataset.quantMode||'full';
   for(const b of document.querySelectorAll('[data-quant-mode]'))b.setAttribute('aria-pressed',String(b===btn));
   pollLabSignal(false);
  });
 }
 $('ql-refresh')?.addEventListener('click',()=>pollLabSignal(true));

 for(const btn of document.querySelectorAll('[data-minute-view]')){
  btn.addEventListener('click',()=>{
   chartView=btn.dataset.minuteView||'friction';
   for(const b of document.querySelectorAll('[data-minute-view]'))b.setAttribute('aria-pressed',String(b===btn));
   if(simData)renderMinuteChart(simData);
  });
 }
 for(const id of ['ql-sim-window','ql-sim-mode','ql-sim-hurdle']){
  $(id)?.addEventListener('input',()=>{if(simData)renderSimulator(simData)});
  $(id)?.addEventListener('change',()=>{if(simData)renderSimulator(simData)});
 }
 for(const id of ['ql-filter-family','ql-filter-mode','ql-filter-sort','ql-filter-search']){
  $(id)?.addEventListener('input',()=>{if(simData)render100Table(simData)});
  $(id)?.addEventListener('change',()=>{if(simData)render100Table(simData)});
 }

 // Automatically re-theme all Chart.js canvases when Light/Dark theme toggles!
 new MutationObserver(()=>{
  if(labActive&&simData)renderSimulationAll(simData);
 }).observe(document.documentElement,{attributes:true,attributeFilter:['data-theme']});
}

let wired=false;
const api={
 enterHome(){this.setHomeActive(true)},
 leaveHome(){this.setHomeActive(false)},
 enter(){this.setLabActive(true)},
 leave(){this.setLabActive(false)},
 setHomeActive(on){
  homeActive=Boolean(on);
  homeGen++;
  clearTimeout(homeTimer);
  homeCtrl?.abort();
  if(homeActive){
   if(latestSignals.BTC)renderHomeBanner(latestSignals.BTC);
   else buildTimeline($('hq-min-track'),null);
   pollHomeBanner();
  }
 },
 setLabActive(on){
  labActive=Boolean(on);
  labGen++;
  clearTimeout(signalTimer);
  signalCtrl?.abort();
  if(!wired){wireControls();wired=true}
  if(labActive){
   if(latestSignals[selectedAsset])renderLiveDesk(latestSignals[selectedAsset]);
   pollLabSignal(false);
   ensureSimulation().then(d=>{if(labActive)renderSimulationAll(d)}).catch(()=>{});
  }
 }
};
root.QuantLab=api;
})(typeof window==='undefined'?globalThis:window);
