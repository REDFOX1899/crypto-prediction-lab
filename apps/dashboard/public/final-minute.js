/* Presentation only: the server supplies per-second coverage and the observed mean. */
(function(root){'use strict';
function model(s,elapsedMs=0){
 if(!s||!Number.isFinite(s.server_time_ms))return null;
 const now=s.server_time_ms+Math.max(0,elapsedMs),close=s.close_time_ms;
 const phase=s.phase==='metadata_stale'||!Number.isFinite(close)?s.phase:now>=close?'closed':now>=close-60000?'final_minute':'before_final';
 const fresh=s.source_state==='live'&&Number.isFinite(s.source_time_ms)&&now-s.source_time_ms<=10000&&now>=s.source_time_ms-2000&&elapsedMs<=15000;
 const coverage=/^[10-]{60}$/.test(s.coverage||'')?s.coverage:'-'.repeat(60);
 return {...s,phase,fresh,coverage,seconds:Number.isFinite(close)?Math.max(0,Math.ceil((close-now)/1000)):null,
  startsIn:Number.isFinite(close)?Math.max(0,Math.ceil((close-60000-now)/1000)):null,
  distance_usd:fresh&&phase!=='closed'&&phase!=='metadata_stale'?s.distance_usd:null,
  required_remaining_average:fresh&&phase==='final_minute'&&s.phase==='final_minute'&&elapsedMs<=3000?s.required_remaining_average:null,
  threshold_status:phase!=='final_minute'?phase:!fresh?'stale_reference':elapsedMs>3000?'updates_delayed':s.threshold_status,
  distance_percent:fresh&&phase!=='closed'&&phase!=='metadata_stale'?s.distance_percent:null};
}
if(typeof module!=='undefined')module.exports={model};
if(typeof document==='undefined')return;
const $=id=>document.getElementById(id);let data=null,received=0,asset='BTC',active=false;
const precision=()=>asset==='SOL'?4:2;
const money=v=>v===null||v===undefined||!Number.isFinite(Number(v))?'—':Number(v).toLocaleString('en-US',{style:'currency',currency:'USD',minimumFractionDigits:precision(),maximumFractionDigits:precision()});
const clock=n=>n===null?'--:--':`${String(Math.floor(n/60)).padStart(2,'0')}:${String(n%60).padStart(2,'0')}`;
function render(){
 if(!active)return; const m=model(data,performance.now()-received),panel=$('final-minute');
 panel.dataset.phase=m?.phase||'waiting_market';
 $('final-title').textContent=`${asset} · the final minute`;
 $('final-clock').textContent=clock(m?.seconds??null);
 $('final-phase').textContent=!m?'Waiting for market & reference feed':m.phase==='closed'?'Window closed · awaiting next market':m.phase==='metadata_stale'?'Market details stale · reconnecting':m.phase==='waiting_market'?'Waiting for an active 15-minute market':m.phase==='final_minute'?'Final 60 seconds · observation in progress':`Final minute starts in ${clock(m.startsIn)}`;
 $('final-target').textContent=money(m?.target);
 $('final-latest').textContent=money(m?.latest_value);
 $('final-source').textContent=m?.source_time_ms?`${m.fresh?'Source age':'Last observed'} · ${Math.max(0,Math.floor((m.server_time_ms+Math.max(0,performance.now()-received)-m.source_time_ms)/1000))}s ago`:`Reference ${m?.source_state||'connecting'}`;
 const distance=m?.distance_usd,delta=distance==null?null:Number(distance);
 $('final-distance').textContent=delta===null?'—':`${delta>0?'+':delta<0?'−':''}${money(Math.abs(delta))}`;
 $('final-distance').dataset.direction=delta===null?'unknown':delta>=0?'up':'down';
 $('final-distance-note').textContent=delta===null?(m?.phase==='closed'?'Closed · not an official result':m?.target==null?'Numeric target unavailable':'Waiting for a fresh reference value'):`${delta>=0?'At or above':'Below'} target · ${Number(m.distance_percent).toFixed(4)}%`;
 $('final-average').textContent=money(m?.observed_average);
 const observed=m?.observed_seconds||0,elapsed=m?.elapsed_seconds||0;
 $('final-coverage').textContent=`${observed}/60 seconds observed · ${Math.round(observed/60*100)}% of full minute`;
 $('final-coverage-detail').textContent=`${observed} of ${elapsed} completed seconds received · ${elapsed-observed} missing`;
 const strip=$('final-coverage-strip');if(strip.childElementCount!==60){strip.replaceChildren(...Array.from({length:60},()=>document.createElement('span')))}
 const coverage=m?.coverage||'-'.repeat(60);
 Array.from(strip.children).forEach((cell,i)=>{const state=coverage[i]==='1'?'observed':coverage[i]==='0'?'missing':'future';cell.dataset.state=state;cell.title=`Second ${i+1}: ${state==='future'?'not completed at last update':state}`});
 strip.setAttribute('aria-label',`${observed} observed, ${elapsed-observed} missing, ${60-elapsed} not completed at last update`);
 const threshold=m?.required_remaining_average,status=m?.threshold_status;
 $('final-required').textContent=threshold==null?'—':status==='any_positive_average'?'Any positive average':money(threshold)+' or higher';
 const reasons={before_final:'Available during the final 60 seconds',closed:'Window closed',metadata_stale:'Market details stale',waiting_market:'Waiting for an active market',invalid_target:'Numeric at-or-above target unavailable',stale_reference:'Waiting for fresh reference data',missing_seconds:'Unavailable — elapsed reference seconds are missing',updates_delayed:'Waiting for the next feed update',calculation_unavailable:'Calculation unavailable'};
 $('final-required-note').textContent=threshold==null?(reasons[status]||'Waiting for reference observations'):
   `${m.remaining_seconds} remaining seconds · as of ${new Date(m.server_time_ms).toLocaleTimeString()} · rounded upward`;
 $('final-asof').textContent=m?`Coverage as of ${new Date(m.server_time_ms).toLocaleTimeString()}${performance.now()-received>15000?' · updates delayed':''}`:'Coverage appears as source seconds complete.';
}
root.FinalMinuteView={select(symbol){asset=symbol;data=null;active=true;render()},pause(){active=false;data=null},update(summary){if(!active||summary?.asset!==asset)return;if(data&&summary.server_time_ms<data.server_time_ms)return;data=summary;received=performance.now();render()},disconnect(){if(data)data={...data,source_state:'disconnected',distance_usd:null,distance_percent:null};render()}};
setInterval(render,250);
})(typeof window!=='undefined'?window:globalThis);
