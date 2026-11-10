// One shared request per tab; never opens a provider connection for each card.
(function(root){
'use strict';
const listeners=new Set();let timer=null,request=null,failures=0,retryUntil=0;
function schedule(delay){clearTimeout(timer);timer=null;if(listeners.size&&!request)timer=setTimeout(()=>{timer=null;poll()},Math.max(delay,retryUntil-Date.now()))}
async function poll(){
 if(request||!listeners.size)return;
 const flight={controller:new AbortController(),started:Date.now(),listeners:[...listeners]};request=flight;
 const timeout=setTimeout(()=>flight.controller.abort(),8000);let retry=0;
 try{
  const response=await fetch('/api/feeds',{cache:'no-store',signal:flight.controller.signal});
  if(!response.ok){if(response.status===429){const seconds=Number(response.headers?.get('Retry-After'));if(Number.isFinite(seconds)&&seconds>0)retry=Math.min(60000,seconds*1000)}throw Error('Feed unavailable')}
  const data=await response.json();if(request!==flight)return;failures=0;retryUntil=0;
  for(const listener of flight.listeners){
   if(!listeners.has(listener))continue;
   const value=data.feeds?.[listener.asset]?.[listener.channel];
   try{if(value)listener.onmessage?.({data:JSON.stringify(value)});else listener.onerror?.()}catch{try{listener.onerror?.()}catch{}}
  }
 }catch{
  if(request===flight){failures=Math.min(failures+1,5);for(const listener of flight.listeners)if(listeners.has(listener)){try{listener.onerror?.()}catch{}}}
 }finally{
  clearTimeout(timeout);
  if(request===flight){request=null;const interval=document.hidden?15000:2000;
   const backoff=Math.max(retry,failures?Math.min(30000,1000*2**failures):0);retryUntil=Date.now()+backoff;
   schedule(Math.max(250,interval-(Date.now()-flight.started),backoff));}
 }
}
root.DashboardFeed=class{
 constructor(url){
  if(!root.DASHBOARD_POLLING)return new EventSource(url);
  const parsed=new URL(url,location.href);this.asset=parsed.searchParams.get('asset')||'BTC';this.channel=parsed.pathname==='/api/reference'?'reference':'market';listeners.add(this);
  if(!timer&&!request)schedule(0);
 }
 close(){listeners.delete(this);if(!listeners.size){clearTimeout(timer);timer=null;const pending=request;request=null;pending?.controller.abort();failures=0}}
};
document.addEventListener('visibilitychange',()=>{if(!listeners.size||request)return;schedule(document.hidden?15000:0)});
})(window);
