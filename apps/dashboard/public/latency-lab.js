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

function aa(){
 return root.AAChartTheme;
}

function palette(){
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
  accent:dark?'#38BDF8':'#0089F4',
  brand:dark?'#9D74F7':'#7F4BF3',
  up:dark?'#4ADE80':'#34A853',
  down:dark?'#F87171':'#E12D39',
  warn:dark?'#FB923C':'#FF7018',
  terracotta:dark?'#E08E73':'#CC785C',
  ink:dark?'#E5E7EB':'#1F1F1F',
 };
}

function mountChart(key,canvasId,cfg){
 if(typeof root.Chart!=='function')return null;
 const canvas=$(canvasId);
 if(!canvas)return null;
 if(charts[key]){try{charts[key].destroy()}catch{}}
 const plugins=Array.isArray(cfg.plugins)?[...cfg.plugins]:[];
 if(aa()?.plugin&&!plugins.includes(aa().plugin))plugins.push(aa().plugin);
 charts[key]=new root.Chart(canvas.getContext('2d'),Object.assign({},cfg,{plugins}));
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
 const A=aa();
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
    {label:'Browser → Cloud Run RTT (ms)',data:[...liveRtt],borderColor:tp.accent,backgroundColor:`${tp.accent}1c`,borderWidth:2.4,pointRadius:3.2,pointBackgroundColor:tp.accent,pointBorderColor:tp.surface,pointBorderWidth:1.2,fill:true,tension:0.22},
    {label:'CF Benchmarks Wire Lag (ms)',data:[...liveCf],borderColor:tp.warn,backgroundColor:tp.warn,borderWidth:2.1,pointRadius:2.8,pointBackgroundColor:tp.warn,pointBorderColor:tp.surface,pointBorderWidth:1.1,tension:0.22},
    {label:'/api/feeds Server (ms)',data:[...liveFeeds],borderColor:tp.brand,backgroundColor:tp.brand,borderWidth:2,borderDash:[4,3],pointRadius:2.6,pointBackgroundColor:tp.brand,pointBorderColor:tp.surface,pointBorderWidth:1.1,tension:0.22},
    {label:'Quant Fast-Path Server p50 (ms)',data:[...liveQuant],borderColor:tp.up,backgroundColor:tp.up,borderWidth:2.4,pointRadius:3.2,pointBackgroundColor:tp.up,pointBorderColor:tp.surface,pointBorderWidth:1.2,tension:0.22},
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,animation:false,
   layout:{padding:{top:12}},
   interaction:{mode:'index',intersect:false},
   plugins:{
    legend:A?A.legendConfig(tp):{},
    aaEditorial:{
     watermark:'◈ Yes No Tape · Live',
     zone:{type:'yBand',yMin:0,yMax:220,label:'Optimal zone (<220ms)'}
    },
    tooltip:A?A.tooltipConfig(tp):{}
   },
   scales:{
    x:A?A.cleanXScale(tp,{ticks:{maxTicksLimit:10}}):{},
    y:A?A.dottedYScale(tp,{beginAtZero:true,ticks:{callback:v=>`${v}ms`}}):{}
   }
  }
 });
}

function renderWaterfallAndAssets(snap){
 const tp=palette();
 const A=aa();
 // Chart 2: Before vs After Pipeline Waterfall
 mountChart('waterfall','lat-chart-waterfall',{
  type:'bar',
  data:{
   labels:['Market Feed Refresh','Quant + AI Signal','Contract Rollover','Static Repeat Load'],
   datasets:[
    {label:'Before Optimization (ms)',data:[2100,440,20000,650],backgroundColor:tp.terracotta,borderRadius:{topRight:4,bottomRight:4},borderSkipped:'left'},
    {label:'After 10-Agent Optimization (ms)',data:[1020,0.25,4000,15],backgroundColor:tp.up,borderRadius:{topRight:4,bottomRight:4},borderSkipped:'left'},
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,indexAxis:'y',
   layout:{padding:{top:10,right:46}},
   plugins:{
    legend:A?A.legendConfig(tp):{},
    aaEditorial:{
     watermark:'◈ Yes No Tape',
     barLabels:{enabled:true,horizontal:true,datasetIndices:[0,1],fontSize:10,format:'ms'}
    },
    tooltip:A?A.tooltipConfig(tp,{callbacks:{label:c=>`${c.dataset.label}: ${fmtMs(c.parsed.x)}`}}):{}
   },
   scales:{
    x:A?A.dottedYScale(tp,{type:'logarithmic',min:0.1,ticks:{callback:v=>v>=1000?`${v/1000}s`:`${v}ms`}}):{},
    y:A?A.cleanXScale(tp,{ticks:{color:tp.text,font:{family:tp.fontSans,size:11.5,weight:'500'}}}):{}
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
    {label:'Kalshi Quote Age (ms)',data:assets.map(a=>aMap[a]?.quote_age_ms??0),backgroundColor:tp.accent,borderRadius:{topLeft:4,topRight:4}},
    {label:'L2 Book Age (ms)',data:assets.map(a=>aMap[a]?.book_age_ms??0),backgroundColor:tp.up,borderRadius:{topLeft:4,topRight:4}},
    {label:'CF Reference Wire Lag (ms)',data:assets.map(a=>aMap[a]?.ref_wire_latency_ms??0),backgroundColor:tp.warn,borderRadius:{topLeft:4,topRight:4}},
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   layout:{padding:{top:24}},
   plugins:{
    legend:A?A.legendConfig(tp):{},
    aaEditorial:{
     watermark:'◈ Yes No Tape',
     barLabels:{enabled:true,datasetIndices:[0,1,2],inside:false,minBarWidth:14,fontSize:10,format:'ms'}
    },
    tooltip:A?A.tooltipConfig(tp,{callbacks:{label:c=>`${c.dataset.label}: ${fmtMs(c.parsed.y)}`}}):{}
   },
   scales:{
    x:A?A.cleanXScale(tp,{ticks:{color:tp.text,font:{family:tp.fontSans,size:11.5,weight:'500'}}}):{},
    y:A?A.dottedYScale(tp,{beginAtZero:true,ticks:{callback:v=>fmtMs(v)}}):{}
   }
  }
 });
}

function renderEdgeAndPercentiles(snap){
 const tp=palette();
 const A=aa();
 // Chart 4: How Latency Works For Us (10-Point Latency Decay Frontier from btc-15m-simulation.json)
 const frontier=simData?.advanced_diagnostics?.latency_decay_frontier||[];
 if(frontier.length){
  const targetTo=frontier.reduce((best,r,i)=>r.latency_ms<=250?i:best,-1);
  mountChart('edge','lat-chart-edge',{
   type:'line',
   data:{
    labels:frontier.map(r=>r.latency_ms>=1000?`${r.latency_ms/1000}s`:`${r.latency_ms}ms`),
    datasets:[
     {label:'After-Fee EV / Trade (¢)',data:frontier.map(r=>r.ev_per_trade_cents),yAxisID:'yEv',borderColor:tp.up,backgroundColor:`${tp.up}1c`,borderWidth:2.6,pointRadius:4.2,pointBackgroundColor:tp.up,pointBorderColor:tp.surface,pointBorderWidth:1.5,fill:true,tension:0.22},
     {label:'$100 Paper Bankroll End ($)',data:frontier.map(r=>r.end_usd),yAxisID:'yUsd',borderColor:tp.accent,backgroundColor:tp.accent,borderWidth:2.2,pointRadius:3.6,pointBackgroundColor:tp.accent,pointBorderColor:tp.surface,pointBorderWidth:1.2,tension:0.22},
     {label:'Adverse Quote Drift (¢)',data:frontier.map(r=>r.adverse_slippage_cents),yAxisID:'yEv',borderColor:tp.down,backgroundColor:tp.down,borderDash:[4,4],borderWidth:2,pointRadius:3.2,pointBackgroundColor:tp.down,pointBorderColor:tp.surface,pointBorderWidth:1.2,tension:0.22},
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
     tooltip:A?A.tooltipConfig(tp,{callbacks:{title:items=>frontier[items[0].dataIndex]?.label||''}}):{}
    },
    scales:{
     x:A?A.cleanXScale(tp):{},
     yEv:A?A.dottedYScale(tp,{position:'left',ticks:{callback:v=>`${v}¢`}}):{},
     yUsd:{position:'right',border:{display:false},grid:{display:false},ticks:{color:tp.axisText,callback:v=>`$${v}`,font:{family:tp.fontSans,size:10.5}}}
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
    {label:'p50 Median (ms)',data:[rtt50,c50,g50,f50,q50],backgroundColor:tp.accent,borderRadius:{topLeft:4,topRight:4}},
    {label:'p95 Tail (ms)',data:[rtt95,c95,g95,f95,q95],backgroundColor:tp.brand,borderRadius:{topLeft:4,topRight:4}},
   ]
  },
  options:{
   responsive:true,maintainAspectRatio:false,
   layout:{padding:{top:24}},
   plugins:{
    legend:A?A.legendConfig(tp):{},
    aaEditorial:{
     watermark:'◈ Yes No Tape',
     barLabels:{enabled:true,datasetIndices:[0,1],inside:false,minBarWidth:14,fontSize:10,format:'ms'}
    },
    tooltip:A?A.tooltipConfig(tp,{callbacks:{label:c=>`${c.dataset.label}: ${fmtMs(c.parsed.y)}`}}):{}
   },
   scales:{
    x:A?A.cleanXScale(tp,{ticks:{color:tp.text,font:{family:tp.fontSans,size:10.5}}}):{},
    y:A?A.dottedYScale(tp,{beginAtZero:true,ticks:{callback:v=>fmtMs(v)}}):{}
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
