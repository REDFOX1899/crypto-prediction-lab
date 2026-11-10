/* Real observations only. No forecast curves or inferred ticks. */
(function(root){'use strict';
function reference(m,snapshot,u,received,now){
 const open=Date.parse(m?.open_time),close=Date.parse(m?.close_time),meta=Date.parse(snapshot?.fetched_at);
 const valid=m?.status==='active'&&close-open===900000&&now>=open&&now<close&&now-meta<=45000&&meta<=now+2000;
 const phase=!valid?(Number.isFinite(close)&&now>=close?'closed':'waiting'):now>=close-60000?'final':'building';
 const number=v=>v==null||v===''?null:Number.isFinite(Number(v))?Number(v):null;
 const target=valid&&m.strike_type==='greater_or_equal'&&number(m.floor_strike)>0?number(m.floor_strike):null;
 const value=number(u?.value),source=u?.source_time_ms;
 const observed=valid&&value>0&&Number.isFinite(source)&&source>=open&&source<close&&source<=now+2000;
 const fresh=observed&&u.state==='live'&&now-source<=10000&&Number.isFinite(received)&&now-received<=8000;
 return {phase,target,value:observed?value:null,fresh,source:observed?source:null,age:observed?Math.max(0,now-source):null,
  distance:fresh&&target!==null?value-target:null,percent:fresh&&target!==null?(value-target)/target*100:null,
  seconds:valid?Math.max(0,Math.ceil((close-now)/1000)):null,startsIn:valid?Math.max(0,Math.ceil((close-60000-now)/1000)):null,
  progress:valid?Math.max(0,Math.min(1,(now-open)/900000)):0};
}
function sparkline(samples,target,now){
 const unique=new Map();for(const p of samples||[])if(Number.isFinite(p.t)&&Number.isFinite(p.v)&&p.v>0&&p.t>=now-120000&&p.t<=now)unique.set(p.t,p);
 const points=[...unique.values()].sort((a,b)=>a.t-b.t).slice(-120);if(!points.length)return {path:'',targetY:null,last:null,min:null,max:null,count:0};
 const values=points.map(p=>p.v);if(Number.isFinite(target)&&target>0)values.push(target);
 let min=Math.min(...values),max=Math.max(...values);const pad=Math.max((max-min)*.16,max*.000005);min-=pad;max+=pad;
 const y=v=>+(82-(v-min)/(max-min)*68).toFixed(2),x=t=>+(10+(t-(now-120000))/120000*300).toFixed(2);
 let previous=null;const path=points.map(p=>{const move=!previous||p.t-previous.t>6000;previous=p;return `${move?'M':'L'}${x(p.t)},${y(p.v)}`}).join(' ');
 const last=points.at(-1);return {path,targetY:Number.isFinite(target)&&target>0?y(target):null,last:{x:x(last.t),y:y(last.v)},min,max,count:points.length};
}
const api={reference,sparkline};if(typeof module!=='undefined')module.exports=api;else root.HomeVisuals=api;
})(typeof window!=='undefined'?window:globalThis);
