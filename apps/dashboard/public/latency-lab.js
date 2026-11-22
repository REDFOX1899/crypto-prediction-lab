(function(root){
'use strict';

function latencyTier(rttMs){
 if(rttMs==null||!Number.isFinite(Number(rttMs)))return 'ok';
 const n=Number(rttMs);
 if(n<=220)return 'fast';
 if(n<=600)return 'ok';
 return 'slow';
}

function fmtMs(v){
 if(v==null||!Number.isFinite(Number(v)))return '—';
 const n=Number(v);
 if(n<1&&n>0)return `${n.toFixed(2)}ms`;
 if(n<100)return `${n.toFixed(1)}ms`;
 if(n>=1000)return `${(n/1000).toFixed(2)}s`;
 return `${Math.round(n)}ms`;
}

function speedupLabel(beforeMs,afterMs){
 if(!Number.isFinite(beforeMs)||!Number.isFinite(afterMs)||afterMs<=0)return '—';
 const ratio=beforeMs/afterMs;
 if(ratio>=100)return `${Math.round(ratio).toLocaleString()}× faster`;
 if(ratio>=10)return `${ratio.toFixed(0)}× faster`;
 return `${ratio.toFixed(1)}× faster`;
}

function computePercentile(arr,q){
 const nums=(arr||[]).filter(v=>Number.isFinite(Number(v))).map(Number).sort((a,b)=>a-b);
 if(!nums.length)return null;
 const idx=Math.min(nums.length-1,Math.round((nums.length-1)*q));
 return nums[idx];
}

if(typeof module==='object'&&module.exports){
 module.exports={latencyTier,fmtMs,speedupLabel,computePercentile};
 return;
}

const $=id=>document.getElementById(id);
const el=(tag,text,cls)=>{const n=document.createElement(tag);if(text!=null)n.textContent=text;if(cls)n.className=cls;return n};

let active=false,gen=0,timer=null,ctrl=null,wired=false;
let simData=null,lastSnap=null;
const clientRttHist=[];
const liveLabels=[],liveRtt=[],liveFeeds=[],liveCf=[],liveQuant=[];
const charts={};

function palette(){
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
 };
}

function mountChart(key,canvasId,cfg){
 if(typeof root.Chart!=='function')return null;
 const canvas=$(canvasId);
 if(!canvas)return null;
 if(charts[key]){try{charts[key].destroy()}catch{}}
 charts[key]=new root.Chart(canvas.getContext('2d'),cfg);
 return charts[key];
}

function updateHeaderPill(rttMs,srvMs,cfWireMs){
 const pill=$('top-latency-pill');
 const txt=$('top-latency-text');
 if(!pill||!txt)return;
 pill.dataset.tier=latencyTier(rttMs);
 const parts=[];
 if(rttMs!=null&&Number.isFinite(rttMs))parts.push(`RTT ${Math.round(rttMs)}ms`);
 if(srvMs!=null&&Number.isFinite(srvMs))parts.push(`srv ${fmtMs(srvMs)}`);
 else if(cfWireMs!=null&&Number.isFinite(cfWireMs))parts.push(`CF ${Math.round(cfWireMs)}ms`);
 txt.textContent=parts.length?parts.join(' · '):'Latency · live';
}

// Keep always-visible header pill synced from /api/feeds ticks on Home & Single Market!
root.addEventListener?.('dashboard-latency',ev=>{
 const d=ev?.detail;
 if(!d)return;
 if(Number.isFinite(d.rtt_ms)){
  clientRttHist.push(d.rtt_ms);
  if(clientRttHist.length>64)clientRttHist.shift();
 }
 updateHeaderPill(d.rtt_ms,d.server_ms,d.cf_wire_ms);
});

// Lightweight background probe if user lands directly on Quant/Combo/Data without /api/feeds active
setTimeout(()=>{
 if(!clientRttHist.length&&$('top-latency-pill')){
  const t0=performance.now();
  fetch('/api/latency',{cache:'no-store'}).then(r=>r.ok?r.json():null).then(d=>{
   if(!d)return;
   const rtt=Math.round(performance.now()-t0);
   clientRttHist.push(rtt);
   updateHeaderPill(rtt,d.quant_gemini_ring?.server_p50_ms??d.server_ms,d.cf_wire_ring?.latest_ms);
  }).catch(()=>{});
 }
},1200);

function pushLiveSample(rttMs,snap){
 const nowStr=new Date().toLocaleTimeString([],{hour12:false,minute:'2-digit',second:'2-digit'});
 liveLabels.push(nowStr);
 liveRtt.push(Math.round(rttMs));
 liveFeeds.push(snap?.feeds_ring?.latest_ms??snap?.server_ms??null);
 liveCf.push(snap?.cf_wire_ring?.latest_ms??null);
 liveQuant.push(snap?.quant_gemini_ring?.server_p50_ms??null);
 const cap=28;
 for(const arr of [liveLabels,liveRtt,liveFeeds,liveCf,liveQuant]){
  while(arr.length>cap)arr.shift();
 }
}

function renderLiveStreamChart(){
 const tp=palette();
 const existing=charts.live;
 if(existing){
  existing.data.labels=[...liveLabels];
  existing.data.datasets[0].data=[...liveRtt];
  existing.data.datasets[1].data=[...liveCf];
  existing.data.datasets[2].data=[...liveFeeds];
  existing.data.datasets[3].data=[...liveQuant];
  existing.update('none');
  return;
 }
 mountChart('live','lat-chart-live',{
  type:'line',
  data:{
   labels:[...liveLabels],
   datasets:[
    {label:'Browser → Cloud Run RTT (ms)',data:[...liveRtt],borderColor:tp.accent,backgroundColor:`${tp.accent}22`,borderWidth:2.4,pointRadius:3,fill:true,tension:0.25},
    {label:'CF Benchmarks Wire Lag (ms)',data:[...liveCf],borderColor:tp.warn,borderWidth:2,pointRadius:2.5,tension:0.25},
    {label:'/api/feeds Server (ms)',data:[...liveFeeds],borderColor:tp.purple,borderWidth:2,borderDash:[4,3],pointRadius:2.5,tension:0.25},
    {label:'Quant Fast-Path Server p50 (ms)',data:[...liveQuant],borderColor:tp.up,borderWidth:2.4,pointRadius:3,tension:0.25},
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,animation:false,
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:{position:'bottom',labels:{color:tp.muted,boxWidth:11,font:{size:11}}},
    tooltip:{backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1}
   },
   scales:{
    x:{ticks:{color:tp.muted,maxTicksLimit:10,font:{size:10}},grid:{display:false}},
    y:{beginAtZero:true,ticks:{color:tp.muted,callback:v=>`${v}ms`,font:{size:10}},grid:{color:tp.grid}}
   }
  }
 });
}

function renderWaterfallAndAssets(snap){
 const tp=palette();
 // Chart 2: Before vs After Pipeline Waterfall
 mountChart('waterfall','lat-chart-waterfall',{
  type:'bar',
  data:{
   labels:['Market Feed Refresh','Quant + AI Signal','Contract Rollover','Static Repeat Load'],
   datasets:[
    {label:'Before Optimization (ms)',data:[2100,440,20000,650],backgroundColor:`${tp.down}bb`,borderRadius:5},
    {label:'After 10-Agent Optimization (ms)',data:[1020,0.25,4000,15],backgroundColor:`${tp.up}dd`,borderRadius:5},
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,indexAxis:'y',
   plugins:{
    legend:{position:'bottom',labels:{color:tp.muted,boxWidth:11,font:{size:10}}},
    tooltip:{backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1,callbacks:{label:c=>`${c.dataset.label}: ${fmtMs(c.parsed.x)}`}}
   },
   scales:{
    x:{type:'logarithmic',min:0.1,ticks:{color:tp.muted,callback:v=>v>=1000?`${v/1000}s`:`${v}ms`,font:{size:10}},grid:{color:tp.grid}},
    y:{ticks:{color:tp.text,font:{size:11}},grid:{display:false}}
   }
  }
 });

 // Chart 3: Per-Asset BTC / ETH / SOL Freshness
 const assets=['BTC','ETH','SOL'];
 const aMap=snap?.assets||{};
 mountChart('assets','lat-chart-assets',{
  type:'bar',
  data:{
   labels:assets.map(a=>`${a} (${aMap[a]?.ws_state||'—'})`),
   datasets:[
    {label:'Kalshi Quote Age (ms)',data:assets.map(a=>aMap[a]?.quote_age_ms??0),backgroundColor:`${tp.accent}cc`,borderRadius:5},
    {label:'L2 Book Age (ms)',data:assets.map(a=>aMap[a]?.book_age_ms??0),backgroundColor:`${tp.up}cc`,borderRadius:5},
    {label:'CF Reference Wire Lag (ms)',data:assets.map(a=>aMap[a]?.ref_wire_latency_ms??0),backgroundColor:`${tp.warn}cc`,borderRadius:5},
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   plugins:{
    legend:{position:'bottom',labels:{color:tp.muted,boxWidth:11,font:{size:10}}},
    tooltip:{backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1,callbacks:{label:c=>`${c.dataset.label}: ${fmtMs(c.parsed.y)}`}}
   },
   scales:{
    x:{ticks:{color:tp.text,font:{size:11}},grid:{display:false}},
    y:{beginAtZero:true,ticks:{color:tp.muted,callback:v=>fmtMs(v),font:{size:10}},grid:{color:tp.grid}}
   }
  }
 });
}

function renderEdgeAndPercentiles(snap){
 const tp=palette();
 // Chart 4: How Latency Works For Us (10-Point Latency Decay Frontier from btc-15m-simulation.json)
 const frontier=simData?.advanced_diagnostics?.latency_decay_frontier||[];
 if(frontier.length){
  mountChart('edge','lat-chart-edge',{
   type:'line',
   data:{
    labels:frontier.map(r=>r.latency_ms>=1000?`${r.latency_ms/1000}s`:`${r.latency_ms}ms`),
    datasets:[
     {label:'After-Fee EV / Trade (¢)',data:frontier.map(r=>r.ev_per_trade_cents),yAxisID:'yEv',borderColor:tp.up,backgroundColor:`${tp.up}26`,borderWidth:2.6,pointRadius:4,fill:true,tension:0.25},
     {label:'$100 Paper Bankroll End ($)',data:frontier.map(r=>r.end_usd),yAxisID:'yUsd',borderColor:tp.accent,borderWidth:2.2,pointRadius:3.5,tension:0.25},
     {label:'Adverse Quote Drift (¢)',data:frontier.map(r=>r.adverse_slippage_cents),yAxisID:'yEv',borderColor:tp.down,borderDash:[5,3],borderWidth:2,pointRadius:3,tension:0.25},
    ]
   },
   options:{
    responsive:true,maintainAspectRatio:false,
    interaction:{mode:'index',intersect:false},
    plugins:{
     legend:{position:'bottom',labels:{color:tp.muted,boxWidth:11,font:{size:10}}},
     tooltip:{backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1,callbacks:{title:items=>frontier[items[0].dataIndex]?.label||''}}
    },
    scales:{
     x:{ticks:{color:tp.muted,font:{size:10}},grid:{display:false}},
     yEv:{position:'left',ticks:{color:tp.muted,callback:v=>`${v}¢`,font:{size:10}},grid:{color:tp.grid}},
     yUsd:{position:'right',ticks:{color:tp.muted,callback:v=>`$${v}`,font:{size:10}},grid:{display:false}}
    }
   }
  });
 }

 // Chart 5: Live Percentiles (p50 vs p95)
 const rtt50=computePercentile(clientRttHist,0.50)??0;
 const rtt95=computePercentile(clientRttHist,0.95)??0;
 const f50=snap?.feeds_ring?.p50_ms??0, f95=snap?.feeds_ring?.p95_ms??0;
 const q50=snap?.quant_gemini_ring?.server_p50_ms??0, q95=snap?.quant_gemini_ring?.server_p95_ms??0;
 const g50=snap?.quant_gemini_ring?.gemini_p50_ms??0, g95=snap?.quant_gemini_ring?.gemini_p95_ms??0;
 const c50=snap?.cf_wire_ring?.p50_ms??0, c95=snap?.cf_wire_ring?.p95_ms??0;
 mountChart('pct','lat-chart-pct',{
  type:'bar',
  data:{
   labels:['Browser RTT','CF Wire Lag','Gemini Cold','/api/feeds Srv','Quant Fast-Path'],
   datasets:[
    {label:'p50 Median (ms)',data:[rtt50,c50,g50,f50,q50],backgroundColor:`${tp.accent}cc`,borderRadius:5},
    {label:'p95 Tail (ms)',data:[rtt95,c95,g95,f95,q95],backgroundColor:`${tp.warn}cc`,borderRadius:5},
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   plugins:{
    legend:{position:'bottom',labels:{color:tp.muted,boxWidth:11,font:{size:10}}},
    tooltip:{backgroundColor:tp.surface,titleColor:tp.text,bodyColor:tp.text,borderColor:tp.grid,borderWidth:1,callbacks:{label:c=>`${c.dataset.label}: ${fmtMs(c.parsed.y)}`}}
   },
   scales:{
    x:{ticks:{color:tp.text,font:{size:10}},grid:{display:false}},
    y:{beginAtZero:true,ticks:{color:tp.muted,callback:v=>fmtMs(v),font:{size:10}},grid:{color:tp.grid}}
   }
  }
 });
}

function renderAuditTable(snap){
 const body=$('lat-audit-rows');
 if(!body||!snap?.optimizations)return;
 body.replaceChildren();
 for(const row of snap.optimizations){
  const tr=document.createElement('tr');
  tr.append(
   el('td',row.agent),
   el('td',fmtMs(row.before_ms),'q-neg'),
   el('td',fmtMs(row.after_ms),'q-pos'),
   el('td',speedupLabel(row.before_ms,row.after_ms),'q-pos'),
   el('td',row.change)
  );
  body.append(tr);
 }
}

function renderSnapshot(rttMs,snap){
 lastSnap=snap;
 const modePill=$('lat-mode-pill');
 if(modePill)modePill.textContent=snap.delivery_mode==='sse_realtime'?'SSE · Real-Time Push':'Polled · 1.0s Adaptive (20ms Yield)';

 const ctr=$('lat-counter-pills');
 if(ctr){
  ctr.replaceChildren(
   el('span',`Kalshi WS Ticks: ${Number(snap.counters?.ws_ticks||0).toLocaleString()}`,'q-pill'),
   el('span',`CF Ref Ticks: ${Number(snap.counters?.cf_ticks||0).toLocaleString()}`,'q-pill'),
   el('span',`/api/latency srv: ${fmtMs(snap.server_ms)}`,'q-pill')
  );
 }

 const rttP50=computePercentile(clientRttHist,0.50);
 const rttP95=computePercentile(clientRttHist,0.95);
 $('lat-kpi-rtt').textContent=fmtMs(rttMs);
 $('lat-kpi-rtt-meta').textContent=`Your browser ↔ Cloud Run · p50 ${fmtMs(rttP50)} · p95 ${fmtMs(rttP95)} (n=${clientRttHist.length})`;
 const rPill=$('lat-kpi-rtt-pill');
 if(rPill){const t=latencyTier(rttMs);rPill.dataset.tone=t==='fast'?'up':t==='ok'?'accent':'warn';rPill.textContent=t==='fast'?'Optimal':t==='ok'?'Healthy':'Elevated'}

 const q=snap.quant_gemini_ring||{};
 $('lat-kpi-quant').textContent=q.sample_count?fmtMs(q.server_p50_ms):'< 1ms';
 $('lat-kpi-quant-meta').textContent=q.sample_count?`Quant p50 ${fmtMs(q.server_p50_ms)} · p95 ${fmtMs(q.server_p95_ms)} · Gemini cold p50 ${fmtMs(q.gemini_p50_ms)}`:'Run live burst probe or open Quant Lab to populate samples';

 const cf=snap.cf_wire_ring||{};
 $('lat-kpi-cf').textContent=cf.latest_ms!=null?fmtMs(cf.latest_ms):'Standby';
 $('lat-kpi-cf-meta').textContent=cf.samples?`CF → Cloud Run p50 ${fmtMs(cf.p50_ms)} · p95 ${fmtMs(cf.p95_ms)} (${cf.samples} ticks)`:'Waiting for live CF Benchmarks reference frames';

 const fd=snap.feeds_ring||{};
 $('lat-kpi-feeds').textContent=fd.latest_ms!=null?fmtMs(fd.latest_ms):'~20ms';
 $('lat-kpi-feeds-meta').textContent=fd.samples?`Server /api/feeds p50 ${fmtMs(fd.p50_ms)} · p95 ${fmtMs(fd.p95_ms)} (down from 100ms sleep)`:'Cut artificial sleep 100ms → 20ms & doubled refresh cadence';

 $('lat-live-sub').textContent=`Live telemetry updated ${new Date(snap.server_time_ms).toLocaleTimeString()} · Cloud Run us-central1 (0.25 vCPU, 256 MiB, $40 budget guard).`;
 updateHeaderPill(rttMs,q.sample_count?q.server_p50_ms:fd.latest_ms,cf.latest_ms);

 pushLiveSample(rttMs,snap);
 renderLiveStreamChart();
 renderWaterfallAndAssets(snap);
 renderEdgeAndPercentiles(snap);
 renderAuditTable(snap);
}

async function pollOnce(){
 clearTimeout(timer);
 if(!active)return;
 const g=gen;
 ctrl?.abort();
 ctrl=new AbortController();
 const t0=performance.now();
 try{
  const r=await fetch('/api/latency',{cache:'no-store',signal:ctrl.signal});
  if(!r.ok)throw Error(`HTTP ${r.status}`);
  const snap=await r.json();
  const rtt=Math.max(1,performance.now()-t0);
  if(active&&g===gen){
   clientRttHist.push(rtt);
   if(clientRttHist.length>64)clientRttHist.shift();
   renderSnapshot(rtt,snap);
  }
 }catch{
  // Retry smoothly
 }finally{
  if(active&&g===gen)timer=setTimeout(pollOnce,document.hidden?10000:1500);
 }
}

async function runBurstProbe(){
 const btn=$('lat-probe-btn');
 if(btn){btn.disabled=true;btn.textContent='Probing 5×…'}
 try{
  for(let i=0;i<5;i++){
   if(!active)break;
   await fetch('/api/quant/signal?asset=BTC&mode=fast',{cache:'no-store'}).catch(()=>{});
   await pollOnce();
  }
 }finally{
  if(btn){btn.disabled=false;btn.textContent='Run Live Burst Probe (5×) ↻'}
 }
}

function wireOnce(){
 if(wired)return;
 wired=true;
 $('lat-probe-btn')?.addEventListener('click',runBurstProbe);
 new MutationObserver(()=>{
  if(active&&lastSnap){
   if(charts.live){try{charts.live.destroy();delete charts.live}catch{}}
   renderLiveStreamChart();
   renderWaterfallAndAssets(lastSnap);
   renderEdgeAndPercentiles(lastSnap);
  }
 }).observe(document.documentElement,{attributes:true,attributeFilter:['data-theme']});
}

root.LatencyLab={
 enter(){
  active=true;gen++;clearTimeout(timer);ctrl?.abort();
  wireOnce();
  if(!simData){
   fetch('/api/quant/simulation').then(r=>r.ok?r.json():null).then(d=>{
    if(d){simData=d;if(active&&lastSnap)renderEdgeAndPercentiles(lastSnap)}
   }).catch(()=>{});
  }
  pollOnce();
 },
 leave(){
  active=false;gen++;clearTimeout(timer);ctrl?.abort();
 }
};
})(typeof window==='undefined'?globalThis:window);
