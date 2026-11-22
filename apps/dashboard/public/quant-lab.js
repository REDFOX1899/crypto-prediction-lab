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

let homeActive=false,labActive=false,selectedAsset='BTC',signalMode='full',chartView='friction',distScope='top25',tourneyGen='gen2',selectedMinute=12;
let simData=null,simPromise=null,signalTimer=null,homeTimer=null,signalCtrl=null,homeCtrl=null,labGen=0,homeGen=0;
let latestSignals={};
const charts={};

function aa(){
 return root.AAChartTheme;
}

function themePalette(){
 if(aa())return aa().palette();
 const cs=getComputedStyle(document.documentElement);
 const get=(k,fb)=>(cs.getPropertyValue(k)||'').trim()||fb;
 const dark=document.documentElement.dataset.theme==='dark';
 return {
  dark,
  fontSans:'ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  text:get('--text',dark?'#F5F5F7':'#141414'),
  muted:get('--muted',dark?'#A0A0AB':'#5E5E66'),
  axisText:dark?'#A0A0AB':'#52525B',
  legendText:dark?'#E4E4E7':'#1F1F1F',
  grid:dark?'rgba(255,255,255,0.14)':'#CCCCCC',
  surface:get('--surface-elev',dark?'#131317':'#FFFFFF'),
  tooltipBg:dark?'#18181E':'#FFFFFF',
  tooltipBorder:dark?'#32323D':'#D9D9D9',
  neutralBar:dark?'#2B2B35':'#E4E6EB',
  accent:dark?'#38BDF8':'#0089F4',
  brand:dark?'#9D74F7':'#7F4BF3',
  up:dark?'#4ADE80':'#34A853',
  down:dark?'#F87171':'#E12D39',
  warn:dark?'#FB923C':'#FF7018',
  terracotta:dark?'#E08E73':'#CC785C',
  ink:dark?'#E5E7EB':'#1F1F1F',
  indigo:dark?'#948EF0':'#736CD3',
  cobalt:dark?'#60A5FA':'#2243E6',
  teal:dark?'#2DD4BF':'#1B818E',
  slate:dark?'#9CA3AF':'#676767',
  families:{},
 };
}

function famColor(family,tp){
 if(aa())return aa().familyColor(family,tp.dark);
 return tp.families?.[family]||tp.accent;
}

function mountChart(key,canvasId,config){
 if(typeof root.Chart!=='function')return null;
 const canvas=$(canvasId);
 if(!canvas)return null;
 if(charts[key]){try{charts[key].destroy()}catch{}}
 const plugins=Array.isArray(config.plugins)?[...config.plugins]:[];
 if(aa()?.plugin&&!plugins.includes(aa().plugin))plugins.push(aa().plugin);
 charts[key]=new root.Chart(canvas.getContext('2d'),Object.assign({},config,{plugins}));
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
function activeTournament(sim){
 if(!sim)return null;
 const g2=sim.tournament_gen2;
 const g1=sim.tournament_100;
 if(tourneyGen==='gen1'||!g2)return g1;
 if(tourneyGen==='gen2')return g2;
 // Combined 200-agent arena
 const merged=[...(g2.agents||[]),...(g1?.agents||[])].sort((a,b)=>(b.end_usd-a.end_usd)||(b.sharpe-a.sharpe));
 const ranked=merged.map((a,i)=>Object.assign({},a,{rank:i+1}));
 const prof=ranked.filter(a=>a.net_pnl_usd>0).length;
 const vals=ranked.map(a=>a.end_usd).sort((a,b)=>a-b);
 const pctAt=q=>vals[Math.min(vals.length-1,Math.round((vals.length-1)*q))];
 return {
  start_bankroll_usd:100,
  total_agents:ranked.length,
  parallel_workers:g2.parallel_workers,
  simulation_wall_seconds:+((g2.simulation_wall_seconds||0)+(g1?.simulation_wall_seconds||0)).toFixed(2),
  profitable_agents:prof,
  fdr_significant_agents_5pct:g2.fdr_200_realistic_pass??0,
  fdr_200_realistic_pass:g2.fdr_200_realistic_pass??0,
  median_end_usd:+(pctAt(0.5)).toFixed(2),
  mean_end_usd:+(vals.reduce((s,v)=>s+v,0)/Math.max(1,vals.length)).toFixed(2),
  p90_end_usd:+(pctAt(0.9)).toFixed(2),
  p10_end_usd:+(pctAt(0.1)).toFixed(2),
  best_overall:ranked[0],
  best_taker_30plus:g2.best_taker_30plus||g1?.best_taker_30plus,
  best_balanced_taker:g2.best_balanced_taker,
  best_sol_taker:g2.best_sol_taker,
  best_touch_maker_20plus:g1?.best_touch_maker_20plus||g2.best_touch_maker_20plus,
  multiple_testing_note:g2.multiple_testing_note,
  family_summary:[...(g2.family_summary||[]),...(g1?.family_summary||[])].sort((a,b)=>b.best_end_usd-a.best_end_usd),
  featured_curves:[...(g2.featured_curves||[]),...(g1?.featured_curves||[])],
  agents:ranked,
  is_combined:true,
 };
}

function renderFamilyLegend(tp,t){
 const host=$('ql-family-legend');
 if(!host)return;
 host.replaceChildren();
 const fams=(t?.family_summary||[]).map(f=>f.family);
 const unique=fams.length?fams:(aa()?.FAMILY_ORDER||[]).map(x=>x[0]);
 for(const family of unique){
  const item=el('span',null);
  const dot=document.createElement('i');
  dot.style.background=famColor(family,tp);
  item.append(dot,document.createTextNode(family));
  host.append(item);
 }
}

function renderDistChart(sim){
 const t=activeTournament(sim);
 if(!t)return;
 const tp=themePalette();
 const A=aa();
 const allAgents=t.agents||[];
 const isTop=distScope==='top25';
 const shown=isTop?allAgents.slice(0,25):allAgents;
 renderFamilyLegend(tp,t);

 mountChart('dist100','ql-chart-100dist',{
  type:'bar',
  data:{
   labels:shown.map(a=>isTop?a.name.replace(/Min(\d+-\d+)/,'M$1').slice(0,24):`#${a.rank} ${a.id}`),
   datasets:[
    {
     label:'Ending Bankroll ($)',
     data:shown.map(a=>a.end_usd),
     backgroundColor:shown.map(a=>famColor(a.family,tp)),
     hoverBackgroundColor:shown.map(a=>famColor(a.family,tp)),
     borderRadius:{topLeft:isTop?5:2,topRight:isTop?5:2},
     borderSkipped:'bottom',
     barPercentage:isTop?0.78:0.86,
     categoryPercentage:isTop?0.9:0.94,
    }
   ]
  },
  options:{
   responsive:true,
   maintainAspectRatio:false,
   layout:{padding:{top:isTop?26:16,right:8}},
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:{display:false},
    aaEditorial:{
     watermark:'◈ Yes No Tape · Quant',
     refLine:{y:100,label:'$100 Break-Even',color:tp.ink,below:true},
     barLabels:isTop?{
      enabled:true,
      datasetIndices:[0],
      inside:true,
      minBarWidth:14,
      fontSize:9.5,
      format:'usd1'
     }:{enabled:false}
    },
    tooltip:A?A.tooltipConfig(tp,{
     callbacks:{
      title:(items)=>{
       const a=shown[items[0].dataIndex];
       return a?`#${a.rank} ${a.name}`:'';
      },
      label:(ctx)=>{
       const a=shown[ctx.dataIndex];
       if(!a)return '';
       return [
        `Family: ${a.family} (${a.execution_mode.replace('_',' ')} · ${a.sizing})`,
        `End Bankroll: $${a.end_usd.toFixed(2)} (${signedPct(a.roi_pct)})`,
        `Trades: ${a.trades} (Pass ${a.pass_rate_pct}%) · Win: ${a.win_rate_pct}%`,
        `EV/Trade: ${signedCents(a.ev_per_trade_cents)} · Max DD: ${a.max_drawdown_pct}% · Sharpe: ${a.sharpe}`,
        `Bootstrap p=${a.p_value} · BH FDR q=${a.fdr_q_value}`
       ];
      }
     }
    }):{}
   },
   scales:{
    x:A?A.cleanXScale(tp,{
     ticks:{
      maxRotation:isTop?42:0,
      minRotation:isTop?32:0,
      maxTicksLimit:isTop?25:16,
      font:{family:tp.fontSans,size:isTop?10:10}
     }
    }):{},
    y:A?A.dottedYScale(tp,{
     min:isTop?96:undefined,
     suggestedMin:isTop?96:50,
     suggestedMax:isTop?120:120,
     ticks:{callback:v=>`$${v}`}
    }):{}
   }
  }
 });
}

function renderParetoChart(sim){
 const t=activeTournament(sim);
 if(!t)return;
 const tp=themePalette();
 const A=aa();
 const agents=(t.agents||[]).filter(a=>(a.trades||0)>0);
 const frontier=A?A.computeParetoFrontier(agents):[];

 // Select clean non-cluttered direct callout labels for key Pareto & benchmark agents
 const calloutIds=new Set(frontier.map(a=>a.id));
 if(t.best_taker_30plus?.id)calloutIds.add(t.best_taker_30plus.id);
 if(t.best_balanced_taker?.id)calloutIds.add(t.best_balanced_taker.id);
 if(t.best_sol_taker?.id)calloutIds.add(t.best_sol_taker.id);
 if(t.best_touch_maker_20plus?.id)calloutIds.add(t.best_touch_maker_20plus.id);
 const pointItems=[];
 agents.forEach((a,idx)=>{
  if(calloutIds.has(a.id)){
   const shortName=a.name.replace('6-Factor L2 ','6F ').replace('FLB Fav ','FLB ').replace('Touch-Maker ','Touch ').replace('3-Asset ','3A ');
   pointItems.push({index:idx,text:`${shortName.slice(0,22)} ($${a.end_usd.toFixed(1)})`});
  }
 });

 mountChart('pareto','ql-chart-pareto',{
  type:'scatter',
  data:{
   datasets:[
    {
     label:`${t.total_agents} Paper Agents (Family Color)`,
     data:agents.map(a=>({x:a.max_drawdown_pct,y:a.end_usd,agent:a})),
     backgroundColor:agents.map(a=>famColor(a.family,tp)),
     borderColor:tp.surface,
     borderWidth:1.5,
     pointRadius:agents.map(a=>calloutIds.has(a.id)?6.5:4.8),
     pointHoverRadius:8.5,
    },
    {
     type:'line',
     label:'Pareto Frontier (Efficient Set)',
     data:frontier.map(a=>({x:a.max_drawdown_pct,y:a.end_usd})),
     borderColor:tp.ink,
     backgroundColor:'transparent',
     borderWidth:1.6,
     borderDash:[3,3],
     pointRadius:0,
     fill:false,
     tension:0,
     order:0,
    }
   ]
  },
  options:{
   responsive:true,
   maintainAspectRatio:false,
   layout:{padding:{top:18,right:18}},
   plugins:{
    legend:A?A.legendConfig(tp):{},
    aaEditorial:{
     watermark:'◈ Yes No Tape · Pareto',
     zone:{type:'quadrant',xMax:6.5,yMin:100,label:'Most attractive quadrant (≥$100 · ≤6.5% DD)'},
     refLine:{y:100,label:'$100 Break-Even',color:tp.muted},
     pointLabels:{datasetIndex:0,items:pointItems},
    },
    tooltip:A?A.tooltipConfig(tp,{
     callbacks:{
      title:(items)=>{
       const a=items[0]?.raw?.agent;
       return a?`#${a.rank} ${a.name}`:'Pareto Frontier';
      },
      label:(ctx)=>{
       const a=ctx.raw?.agent;
       if(!a)return `Frontier: $${Number(ctx.parsed.y).toFixed(2)} at ${Number(ctx.parsed.x).toFixed(1)}% DD`;
       return [
        `Family: ${a.family} (${a.execution_mode.replace('_',' ')})`,
        `Ending Bankroll: $${a.end_usd.toFixed(2)} (${signedPct(a.roi_pct)})`,
        `Max Drawdown: ${a.max_drawdown_pct}% · Win Rate: ${a.win_rate_pct}%`,
        `Trades: ${a.trades} · EV/Tr: ${signedCents(a.ev_per_trade_cents)} · Sharpe: ${a.sharpe}`
       ];
      }
     }
    }):{}
   },
   scales:{
    x:A?A.dottedYScale(tp,{
     min:0,
     title:{display:true,text:'Max Drawdown (% · Lower is safer →)',color:tp.axisText,font:{family:tp.fontSans,size:11,weight:'500'}},
     ticks:{callback:v=>`${v}%`}
    }):{},
    y:A?A.dottedYScale(tp,{
     suggestedMin:75,
     suggestedMax:122,
     title:{display:true,text:'Ending Bankroll ($100 Start · Higher is better)',color:tp.axisText,font:{family:tp.fontSans,size:11,weight:'500'}},
     ticks:{callback:v=>`$${v}`}
    }):{}
   }
  }
 });
}

function renderTournament100(sim){
 const t=activeTournament(sim);
 if(!t)return;
 const tp=themePalette();
 const A=aa();
 $('ql-tourney-note').textContent=t.multiple_testing_note;

 const pills=$('ql-tourney-pills');
 const fdrCount=t.fdr_200_realistic_pass!=null?t.fdr_200_realistic_pass:t.fdr_significant_agents_5pct;
 const totalLabel=t.total_agents;
 pills.replaceChildren(
  el('span',`${totalLabel} Agents · $${t.start_bankroll_usd} Start`,'q-pill'),
  el('span',`${t.profitable_agents}/${totalLabel} Profitable · ${fdrCount}/200 Realistic FDR q≤0.05`,'q-pill'),
  el('span',`${t.simulation_wall_seconds}s on ${t.parallel_workers} CPU cores`,'q-pill')
 );

 const kpis=$('ql-tourney-kpis');
 kpis.replaceChildren();
 const bt=t.best_taker_30plus||t.best_overall;
 const bb=t.best_balanced_taker;
 const bs=t.best_sol_taker;
 const bo=t.best_overall;
 const bm=t.best_touch_maker_20plus;
 const cards=bb&&bs?[
  {lbl:'Top Realistic Taker (Both Halves +)',pill:`${bb.win_rate_pct}% Win · ${bb.max_drawdown_pct}% DD`,tone:'up',val:`$${bb.end_usd.toFixed(2)}`,meta:`${bb.name} · ${signedPct(bb.roi_pct)} · ${bb.trades} tr · ${signedCents(bb.ev_per_trade_cents)}/tr · H1 ${signedUsd(bb.half1_pnl_usd)} / H2 ${signedUsd(bb.half2_pnl_usd)} · 1d Win ${bb.pct_days_profitable}%`},
  {lbl:'SOL Lead-Lag Discovery (10/10 Win)',pill:`Sharpe ${bs.sharpe}`,tone:'up',val:`$${bs.end_usd.toFixed(2)}`,meta:`${bs.name} · ${signedCents(bs.ev_per_trade_cents)}/tr (${bs.trades} tr) · H1 ${signedUsd(bs.half1_pnl_usd)} / H2 ${signedUsd(bs.half2_pnl_usd)} · 1d Win ${bs.pct_days_profitable}% · Worst 1d ${signedUsd(bs.worst_day_pnl_usd)}`},
  {lbl:'Honest 1-Day ("Tomorrow") Risk',pill:'0/200 FDR q≤0.05',tone:'warn',val:`${bb.pct_days_loss}% Lose`,meta:`Even top realistic takers lost money on ${bb.pct_days_loss}%–38.5% of individual days (Worst 1d ${signedUsd(bb.worst_day_pnl_usd)} to -$5.11 at 1-ct). No strategy guarantees profit tomorrow.`},
  {lbl:`${totalLabel}-Agent Arena ($100 Start)`,pill:`${t.profitable_agents}/${totalLabel} > $100`,tone:'neutral',val:`$${t.median_end_usd.toFixed(2)}`,meta:`Median $${t.median_end_usd} · Mean $${t.mean_end_usd} · P90 $${t.p90_end_usd} · P10 $${t.p10_end_usd} · Pure FLB (No XAsset) Falsification: $86.59 (-13.4%)`},
 ]:[
  {lbl:'Best Realistic Taker (n≥30)',pill:`${bt.win_rate_pct}% Win`,tone:'up',val:`$${bt.end_usd.toFixed(2)}`,meta:`${bt.name} · ${signedPct(bt.roi_pct)} · ${bt.trades} trades · ${signedCents(bt.ev_per_trade_cents)}/tr · DD ${bt.max_drawdown_pct}% · Sharpe ${bt.sharpe}`},
  {lbl:'Best Optimistic Mid-Maker',pill:`Sharpe ${bo.sharpe}`,tone:'accent',val:`$${bo.end_usd.toFixed(2)}`,meta:`${bo.name} · ${signedPct(bo.roi_pct)} · assumes zero adverse-selection queue loss`},
  {lbl:'Adverse-Selection Touch-Maker',pill:'Realistic Limit',tone:'warn',val:`$${bm.end_usd.toFixed(2)}`,meta:`Best of 10 Touch-Maker agents (${bm.name}) · proves resting limit adverse selection costs ~3–4¢/tr`},
  {lbl:`${totalLabel}-Agent Distribution ($100 Start)`,pill:`${t.profitable_agents}/${totalLabel} > $100`,tone:'neutral',val:`$${t.median_end_usd.toFixed(2)}`,meta:`Median $${t.median_end_usd} · Mean $${t.mean_end_usd} · P90 $${t.p90_end_usd} · P10 $${t.p10_end_usd} · BH FDR q≤0.05: ${t.fdr_significant_agents_5pct}/${totalLabel}`},
 ];
 for(const c of cards){
  const art=el('article',null,'q-kpi');
  const top=el('div',null,'q-kpi-label');
  const pill=el('span',c.pill,'q-pill');pill.dataset.tone=c.tone;
  top.append(el('span',c.lbl),pill);
  art.append(top,el('div',c.val,'q-kpi-val'),el('div',c.meta,'q-kpi-meta'));
  kpis.append(art);
 }

 // Repopulate Family dropdown whenever generation changes
 const famSel=$('ql-filter-family');
 if(famSel){
  famSel.replaceChildren();
  const allOpt=document.createElement('option');
  allOpt.value='ALL';
  allOpt.textContent=`All Families (${t.total_agents} Agents)`;
  famSel.append(allOpt);
  for(const fs of (t.family_summary||[])){
   const opt=document.createElement('option');
   opt.value=fs.family;
   opt.textContent=`${fs.family} (${fs.profitable_count}/${fs.agent_count} > $100)`;
   famSel.append(opt);
  }
 }

 renderDistChart(sim);
 renderParetoChart(sim);

 // Charts 3 & 4: Top Paper Equity Curves ($100 Start) & Synchronized Underwater Drawdown Curves
 const curves=t.featured_curves||[];
 const picks=[];
 const wantSub=tourneyGen==='gen1'
  ?['FLB Fav 72-88c + XAsset Min11-13','6-Factor L2 Min12-14 Taker 0.6c','6-Factor L2 Min12-14 Mid-Maker 1.5c','Touch-Maker BS-Digital Min10-13 Mid','Naive Mid Favorite','Naive Early Favorite']
  :['3-Asset Best-EV FLB 72-88c + XAsset','EU+US Liquid Hours FLB 72-88c 3-Asset','SOL BTC+ETH Lead Min11-13 50ms','SOL FLB Fav 72-88c + XAsset Min11-13','3-Asset Optimistic Mid-Maker FLB','Falsification: Pure FLB 72-88c'];
 for(const sub of wantSub){
  const found=curves.find(c=>c.name.includes(sub));
  if(found)picks.push(found);
 }
 if(picks.length<4){
  for(const c of curves.slice(0,5))if(!picks.includes(c))picks.push(c);
 }
 const palette=[tp.up,tp.accent,tp.brand,tp.terracotta,tp.warn,tp.ink];
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
    label:`${c.name.slice(0,28)} ($${c.end_usd.toFixed(1)})`,
    data:resample(c.equity_curve_usd,maxLen),
    borderColor:palette[i%palette.length],
    backgroundColor:palette[i%palette.length],
    borderWidth:i<2?2.5:1.8,
    borderDash:(c.name.includes('Naive')||c.name.includes('Falsification'))?[4,4]:[],
    pointRadius:0,
    pointHoverRadius:4,
    tension:0.2,
    fill:false,
   }))
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   layout:{padding:{top:12}},
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:A?A.legendConfig(tp,{labels:{font:{family:tp.fontSans,size:10.5}}}):{},
    aaEditorial:{watermark:'◈ Yes No Tape',refLine:{y:100,label:'$100',color:tp.muted}},
    tooltip:A?A.tooltipConfig(tp,{callbacks:{label:c=>`${c.dataset.label.split(' (')[0]}: $${Number(c.parsed.y).toFixed(2)}`}}):{}
   },
   scales:{
    x:A?A.cleanXScale(tp,{ticks:{maxTicksLimit:8}}):{},
    y:A?A.dottedYScale(tp,{ticks:{callback:v=>`$${v}`}}):{}
   }
  }
 });

 mountChart('drawdown','ql-chart-dd',{
  type:'line',
  data:{
   labels:xLabels,
   datasets:picks.map((c,i)=>({
    label:c.name.slice(0,26),
    data:resample(c.drawdown_curve_pct,maxLen),
    borderColor:palette[i%palette.length],
    backgroundColor:i===0?`${palette[0]}1f`:palette[i%palette.length],
    borderWidth:i<2?2.4:1.7,
    borderDash:c.name.includes('Naive')?[4,4]:[],
    pointRadius:0,
    pointHoverRadius:4,
    fill:i===0,
    tension:0.2,
   }))
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   layout:{padding:{top:12}},
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:A?A.legendConfig(tp,{labels:{font:{family:tp.fontSans,size:10.5}}}):{},
    aaEditorial:{watermark:'◈ Yes No Tape'},
    tooltip:A?A.tooltipConfig(tp,{callbacks:{label:c=>`${c.dataset.label}: ${Number(c.parsed.y).toFixed(1)}%`}}):{}
   },
   scales:{
    x:A?A.cleanXScale(tp,{ticks:{maxTicksLimit:8}}):{},
    y:A?A.dottedYScale(tp,{max:0,ticks:{callback:v=>`${v}%`}}):{}
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
   const c1=document.createElement('td');
   const sw=document.createElement('span');
   sw.className='aa-swatch';
   sw.style.background=famColor(fs.family,tp);
   sw.style.marginInlineEnd='8px';
   c1.append(sw,el('strong',fs.family));
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
 const t=activeTournament(sim);
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
  td1.append(el('strong',`#${a.rank} ${a.name}`),el('small',`${a.id} · ${a.window}${a.latency_ms!=null?` · ${a.latency_ms}ms`:''}`));
  const td2=el('td',a.family);
  const td3=el('td',`${a.execution_mode.replace('_',' ')} · ${a.universe||'BTC'}`);
  const td4=el('td',`$${a.end_usd.toFixed(2)}`,a.end_usd>100?'q-pos':a.end_usd<100?'q-neg':'');
  const td5=el('td',`${signedUsd(a.net_pnl_usd)} (${signedPct(a.roi_pct)})`,a.net_pnl_usd>=0?'q-pos':'q-neg');
  const td6=el('td',`${a.trades} (${a.pass_rate_pct}%)`);
  const td7=el('td',`${a.win_rate_pct.toFixed(1)}%`);
  const td8=el('td',signedCents(a.ev_per_trade_cents),a.ev_per_trade_cents>=0?'q-pos':'q-neg');
  const td9=el('td',`-${a.max_drawdown_pct.toFixed(1)}%`);
  const td10=el('td',a.pct_days_profitable!=null?`${a.pct_days_profitable}% / ${signedUsd(a.worst_day_pnl_usd)}`:`Sh ${a.sharpe.toFixed(2)}`);
  const td11=el('td',a.half1_pnl_usd!=null?`${signedUsd(a.half1_pnl_usd)} / ${signedUsd(a.half2_pnl_usd)}`:'—',a.both_halves_positive?'q-pos':'');
  const qShow=a.fdr_q_200!=null?a.fdr_q_200:a.fdr_q_value;
  const td12=el('td',`p=${a.p_value.toFixed(3)} · q=${qShow.toFixed(2)}`);
  tr.append(td1,td2,td3,td4,td5,td6,td7,td8,td9,td10,td11,td12);
  tbody.append(tr);
 }
 if(!rows.length){
  const tr=document.createElement('tr');
  const td=el('td','No agents match the active filter.');
  td.colSpan=12;
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
 const A=aa();
 const labels=rows.map(r=>`M${r.minute}`);
 const m10Idx=rows.findIndex(r=>r.minute===10);
 const m13Idx=rows.findIndex(r=>r.minute===13);
 const optZone=m10Idx>=0&&m13Idx>=m10Idx?{type:'xBand',fromIndex:m10Idx,toIndex:m13Idx,label:'Optimal zone (M10–M13)'}:null;

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
      backgroundColor:rows.map(r=>(r.minute>=10&&r.minute<=13)?`${tp.up}44`:tp.neutralBar),
      borderColor:rows.map(r=>(r.minute>=10&&r.minute<=13)?tp.up:'transparent'),
      borderWidth:1,borderRadius:{topLeft:4,topRight:4},borderSkipped:'bottom',order:3,
     },
     {
      type:'line',label:'Total Taker Friction (½ Spread + Fee, ¢)',
      data:rows.map(r=>r.total_taker_friction_cents),
      yAxisID:'yCents',borderColor:tp.down,backgroundColor:tp.down,
      borderWidth:2.6,pointRadius:4,pointBackgroundColor:tp.down,pointBorderColor:tp.surface,pointBorderWidth:1.5,tension:0.22,order:1,
     },
     {
      type:'line',label:'Kalshi Quadratic Taker Fee (¢)',
      data:rows.map(r=>r.avg_taker_fee_cents),
      yAxisID:'yCents',borderColor:tp.warn,backgroundColor:tp.warn,borderDash:[4,4],
      borderWidth:2,pointRadius:3.2,pointBackgroundColor:tp.warn,pointBorderColor:tp.surface,pointBorderWidth:1.2,tension:0.22,order:1,
     },
     {
      type:'line',label:'Avg Bid-Ask Spread (¢)',
      data:rows.map(r=>r.avg_spread_cents),
      yAxisID:'yCents',borderColor:tp.accent,backgroundColor:tp.accent,
      borderWidth:2.2,pointRadius:3.5,pointBackgroundColor:tp.accent,pointBorderColor:tp.surface,pointBorderWidth:1.2,tension:0.22,order:2,
     },
    ]
   },
   options:{
    responsive:true,maintainAspectRatio:false,
    layout:{padding:{top:12}},
    interaction:{mode:'index',intersect:false},
    onClick:(_,els)=>{if(els.length){selectedMinute=rows[els[0].index].minute;renderMinuteReadout(sim)}},
    plugins:{
     legend:A?A.legendConfig(tp):{},
     aaEditorial:{watermark:'◈ Yes No Tape',zone:optZone},
     tooltip:A?A.tooltipConfig(tp):{}
    },
    scales:{
     x:A?A.cleanXScale(tp):{},
     yCents:A?A.dottedYScale(tp,{position:'left',ticks:{callback:v=>`${v}¢`},title:{display:true,text:'Friction (¢ · Lower is cheaper)',color:tp.axisText,font:{family:tp.fontSans,size:11}}}):{},
     yVol:{position:'right',border:{display:false},grid:{display:false},ticks:{color:tp.axisText,callback:v=>`${Math.round(v/1000)}k`,font:{family:tp.fontSans,size:10.5}},title:{display:true,text:'1m Volume',color:tp.axisText,font:{family:tp.fontSans,size:11}}}
    }
   }
  };
 } else if(chartView==='ev'){
  cfg={
   type:'bar',
   data:{
    labels,
    datasets:[
     {label:'Quant Taker OOS Net PnL (¢)',data:rows.map(r=>r.taker_net_cents),backgroundColor:rows.map(r=>r.taker_net_cents>=0?tp.up:tp.terracotta),borderRadius:{topLeft:4,topRight:4}},
     {label:'Quant Mid-Maker OOS Net PnL (¢)',data:rows.map(r=>r.maker_net_cents),backgroundColor:rows.map(r=>r.maker_net_cents>=0?tp.accent:tp.warn),borderRadius:{topLeft:4,topRight:4}},
    ]
   },
   options:{
    responsive:true,maintainAspectRatio:false,
    layout:{padding:{top:20}},
    interaction:{mode:'index',intersect:false},
    onClick:(_,els)=>{if(els.length){selectedMinute=rows[els[0].index].minute;renderMinuteReadout(sim)}},
    plugins:{
     legend:A?A.legendConfig(tp):{},
     aaEditorial:{watermark:'◈ Yes No Tape',zone:optZone,refLine:{y:0,label:'0¢',color:tp.ink}},
     tooltip:A?A.tooltipConfig(tp):{}
    },
    scales:{
     x:A?A.cleanXScale(tp):{},
     y:A?A.dottedYScale(tp,{ticks:{callback:v=>`${v}¢`}}):{}
    }
   }
  };
 } else {
  cfg={
   type:'line',
   data:{
    labels,
    datasets:[
     {label:'Market Favorite Accuracy (%)',data:rows.map(r=>+(r.favorite_accuracy*100).toFixed(1)),yAxisID:'yAcc',borderColor:tp.accent,backgroundColor:tp.accent,borderWidth:2.6,pointRadius:4,pointBackgroundColor:tp.accent,pointBorderColor:tp.surface,pointBorderWidth:1.5,tension:0.22},
     {label:'Cross-Asset Brier Skill Score (%)',data:rows.map(r=>r.bss_xasset_pct),yAxisID:'yBss',borderColor:tp.up,backgroundColor:tp.up,borderDash:[4,3],borderWidth:2.2,pointRadius:4,pointBackgroundColor:tp.up,pointBorderColor:tp.surface,pointBorderWidth:1.5,tension:0.22},
    ]
   },
   options:{
    responsive:true,maintainAspectRatio:false,
    layout:{padding:{top:12}},
    interaction:{mode:'index',intersect:false},
    onClick:(_,els)=>{if(els.length){selectedMinute=rows[els[0].index].minute;renderMinuteReadout(sim)}},
    plugins:{
     legend:A?A.legendConfig(tp):{},
     aaEditorial:{watermark:'◈ Yes No Tape',zone:optZone},
     tooltip:A?A.tooltipConfig(tp):{}
    },
    scales:{
     x:A?A.cleanXScale(tp):{},
     yAcc:A?A.dottedYScale(tp,{position:'left',min:55,max:95,ticks:{callback:v=>`${v}%`}}):{},
     yBss:{position:'right',border:{display:false},grid:{display:false},ticks:{color:tp.axisText,callback:v=>`${v>=0?'+':''}${v}%`,font:{family:tp.fontSans,size:10.5}}}
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
    const dark=document.documentElement.dataset.theme==='dark';
    const intensity=0.14+Math.abs(clamped)/6*(dark?0.36:0.30);
    td.style.background=c.ev_cents>=0
     ?(dark?`rgba(74,222,128,${intensity.toFixed(2)})`:`rgba(52,168,83,${intensity.toFixed(2)})`)
     :(dark?`rgba(248,113,113,${intensity.toFixed(2)})`:`rgba(204,120,92,${intensity.toFixed(2)})`);
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
 const A=aa();

 // Latency Decay Frontier
 const lc=diag.latency_decay_frontier||[];
 const targetTo=lc.reduce((best,r,i)=>r.latency_ms<=250?i:best,-1);
 mountChart('latency','ql-chart-latency',{
  type:'line',
  data:{
   labels:lc.map(r=>r.latency_ms>=1000?`${r.latency_ms/1000}s`:`${r.latency_ms}ms`),
   datasets:[
    {
     label:'After-Fee EV / Trade (¢)',
     data:lc.map(r=>r.ev_per_trade_cents),
     yAxisID:'yEv',
     borderColor:tp.up,backgroundColor:`${tp.up}1c`,
     borderWidth:2.6,pointRadius:4.5,pointBackgroundColor:tp.up,pointBorderColor:tp.surface,pointBorderWidth:1.5,fill:true,tension:0.22,
    },
    {
     label:'Adverse Quote Drift Slippage (¢)',
     data:lc.map(r=>r.adverse_slippage_cents),
     yAxisID:'yEv',
     borderColor:tp.down,backgroundColor:tp.down,borderDash:[4,4],
     borderWidth:2,pointRadius:3.5,pointBackgroundColor:tp.down,pointBorderColor:tp.surface,pointBorderWidth:1.2,fill:false,tension:0.22,
    },
    {
     label:'$100 Paper Bankroll End ($)',
     data:lc.map(r=>r.end_usd),
     yAxisID:'yUsd',
     borderColor:tp.accent,backgroundColor:tp.accent,borderWidth:2.2,pointRadius:3.8,pointBackgroundColor:tp.accent,pointBorderColor:tp.surface,pointBorderWidth:1.2,fill:false,tension:0.22,
    },
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   layout:{padding:{top:12}},
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:A?A.legendConfig(tp):{},
    aaEditorial:{
     watermark:'◈ Yes No Tape',
     zone:targetTo>=0?{type:'xBand',fromIndex:0,toIndex:targetTo,label:'Target <250ms zone'}:null
    },
    tooltip:A?A.tooltipConfig(tp,{callbacks:{title:items=>lc[items[0].dataIndex]?.label||''}}):{}
   },
   scales:{
    x:A?A.cleanXScale(tp):{},
    yEv:A?A.dottedYScale(tp,{position:'left',ticks:{callback:v=>`${v}¢`}}):{},
    yUsd:{position:'right',border:{display:false},grid:{display:false},ticks:{color:tp.axisText,callback:v=>`$${v}`,font:{family:tp.fontSans,size:10.5}}}
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
    {label:'Perfect Calibration (45°)',data:[5,15,25,35,45,55,65,75,85,95],borderColor:tp.slate,backgroundColor:tp.slate,borderDash:[4,4],borderWidth:1.4,pointRadius:0},
    {label:'Min 12 Compression',data:(cal.late_m12||[]).map(b=>b.empirical_yes_pct),borderColor:tp.up,backgroundColor:tp.up,borderWidth:2.6,pointRadius:4.2,pointBackgroundColor:tp.up,pointBorderColor:tp.surface,pointBorderWidth:1.5,tension:0.15},
    {label:'Min 8 Mid Discovery',data:(cal.mid_m8||[]).map(b=>b.empirical_yes_pct),borderColor:tp.accent,backgroundColor:tp.accent,borderWidth:2.1,pointRadius:3.6,pointBackgroundColor:tp.accent,pointBorderColor:tp.surface,pointBorderWidth:1.3,tension:0.15},
    {label:'Min 3 Early Friction',data:(cal.early_m3||[]).map(b=>b.empirical_yes_pct),borderColor:tp.terracotta,backgroundColor:tp.terracotta,borderWidth:2,pointRadius:3.2,pointBackgroundColor:tp.terracotta,pointBorderColor:tp.surface,pointBorderWidth:1.2,tension:0.15},
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   layout:{padding:{top:12}},
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:A?A.legendConfig(tp):{},
    aaEditorial:{watermark:'◈ Yes No Tape'},
    tooltip:A?A.tooltipConfig(tp,{callbacks:{label:c=>c.parsed.y!=null?`${c.dataset.label}: ${c.parsed.y.toFixed(1)}% YES`:`${c.dataset.label}: n/a`}}):{}
   },
   scales:{
    x:A?A.cleanXScale(tp):{},
    y:A?A.dottedYScale(tp,{min:0,max:100,ticks:{callback:v=>`${v}%`}}):{}
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
 const A=aa();
 mountChart('sessions','ql-chart-sessions',{
  type:'bar',
  data:{
   labels:hrs.map(h=>h.label),
   datasets:[
    {
     type:'bar',label:'Avg Min-12 Volume (Blue=US · Purple=EU)',
     data:hrs.map(h=>h.avg_volume_m12),
     yAxisID:'yVol',
     backgroundColor:hrs.map(h=>h.session==='US Cash'?tp.accent:h.session==='Europe'?tp.brand:tp.neutralBar),
     borderRadius:{topLeft:4,topRight:4},
     borderSkipped:'bottom',
    },
    {
     type:'line',label:'Avg Min-12 Spread (¢)',
     data:hrs.map(h=>h.avg_spread_m12_cents),
     yAxisID:'ySprd',
     borderColor:tp.warn,backgroundColor:tp.warn,borderWidth:2.4,pointRadius:3.5,pointBackgroundColor:tp.warn,pointBorderColor:tp.surface,pointBorderWidth:1.2,tension:0.22,
    },
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   layout:{padding:{top:12}},
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:A?A.legendConfig(tp):{},
    aaEditorial:{watermark:'◈ Yes No Tape'},
    tooltip:A?A.tooltipConfig(tp,{callbacks:{title:items=>`${hrs[items[0].dataIndex]?.label} UTC · ${hrs[items[0].dataIndex]?.session} Session`}}):{}
   },
   scales:{
    x:A?A.cleanXScale(tp):{},
    yVol:A?A.dottedYScale(tp,{position:'left',ticks:{callback:v=>`${Math.round(v/1000)}k`}}):{},
    ySprd:{position:'right',border:{display:false},grid:{display:false},ticks:{color:tp.axisText,callback:v=>`${v}¢`,font:{family:tp.fontSans,size:10.5}}}
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
 const A=aa();
 mountChart('gemini','ql-chart-gemini',{
  type:'bar',
  data:{
   labels:['Minute 4 (Early)','Minute 9 (Mid)','Minute 12 (Compression)'],
   datasets:[
    {label:'Kalshi Midpoint Brier',data:mins.map(m=>g.by_minute[m]?.brier_mid),backgroundColor:tp.ink,borderRadius:{topLeft:4,topRight:4}},
    {label:'6-Factor Quant Brier',data:mins.map(m=>g.by_minute[m]?.brier_quant),backgroundColor:tp.accent,borderRadius:{topLeft:4,topRight:4}},
    {label:'Gemini Decision 1 Raw',data:mins.map(m=>g.by_minute[m]?.brier_gemini_raw),backgroundColor:tp.terracotta,borderRadius:{topLeft:4,topRight:4}},
    {label:'Calibrated Hybrid (Lower = Better)',data:mins.map(m=>g.by_minute[m]?.brier_hybrid),backgroundColor:tp.up,borderRadius:{topLeft:4,topRight:4}},
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   layout:{padding:{top:24}},
   plugins:{
    legend:A?A.legendConfig(tp):{},
    aaEditorial:{
     watermark:'◈ Yes No Tape',
     barLabels:{enabled:true,datasetIndices:[0,1,2,3],inside:false,minBarWidth:14,fontSize:9.5,format:'brier4'}
    },
    tooltip:A?A.tooltipConfig(tp):{}
   },
   scales:{
    x:A?A.cleanXScale(tp):{},
    y:A?A.dottedYScale(tp,{min:0.12,title:{display:true,text:'Brier Score (Lower is Better)',color:tp.axisText,font:{family:tp.fontSans,size:11}}}):{}
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

 for(const btn of document.querySelectorAll('[data-tourney-gen]')){
  btn.addEventListener('click',()=>{
   tourneyGen=btn.dataset.tourneyGen||'gen2';
   for(const b of document.querySelectorAll('[data-tourney-gen]'))b.setAttribute('aria-pressed',String(b===btn));
   if(simData)renderTournament100(simData);
  });
 }
 for(const btn of document.querySelectorAll('[data-dist-scope]')){
  btn.addEventListener('click',()=>{
   distScope=btn.dataset.distScope||'top25';
   for(const b of document.querySelectorAll('[data-dist-scope]'))b.setAttribute('aria-pressed',String(b===btn));
   if(simData)renderDistChart(simData);
  });
 }
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
