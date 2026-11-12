// One latest observation per receipt second; contract changes start a fresh tape.
(function(root) {
  'use strict';
  const duration=120000, maxJoin=12000;
  function empty(ticker=null) { return {ticker, points:[], interrupted:false}; }
  function record(previous,market,state,now) {
    if (!market?.ticker || !(now<Date.parse(market.close_time))) return empty();
    const tape=previous?.ticker===market.ticker?previous:empty(market.ticker);
    while (tape.points.length && tape.points[0].t<now-duration) tape.points.shift();
    const t=Date.parse(state.sampleAt), spread=state.spread, ask=state.yes;
    if (!['live','snapshot'].includes(state.mode) || !Number.isFinite(spread) || spread<=0 ||
        !Number.isFinite(ask) || ask>=1 || ask-spread<=0 || !Number.isFinite(t) ||
        t<Date.parse(market.open_time) || t<now-duration || t>now || now-t>(state.mode==='live'?15000:45000)) {
      tape.interrupted=true; return tape;
    }
    const prior=tape.points.at(-1);
    if (prior && t<=prior.t) return tape;
    const point={t,ask:ask*100,bid:(ask-spread)*100,mid:(ask-spread/2)*100,spread:spread*100,mode:state.mode,
      gap:!prior || tape.interrupted || t-prior.t>maxJoin};
    if (prior && Math.floor(prior.t/1000)===Math.floor(t/1000)) {
      point.gap=point.gap||prior.gap;
      tape.points[tape.points.length-1]=point;
    } else tape.points.push(point);
    tape.interrupted=false;
    return tape;
  }
  function segments(points) {
    const result=[];
    for(const point of points) {
      if(point.gap || !result.length) result.push([]);
      result.at(-1).push(point);
    }
    return result;
  }
  if(typeof module==='object' && module.exports){module.exports={record,segments};return;}
  const id=name=>document.getElementById('live-tape-'+name);
  const fmt=v=>v.toLocaleString(undefined,{maximumFractionDigits:2})+'¢';
  const time=t=>new Date(t).toLocaleTimeString([], {hour:'numeric',minute:'2-digit',second:'2-digit'});
  let tapes={},asset='BTC',lastNow=0,drawn='';
  function svg(tag,attrs,text) {
    const node=document.createElementNS('http://www.w3.org/2000/svg',tag);
    for(const [k,v] of Object.entries(attrs))node.setAttribute(k,v);
    if(text!==undefined)node.textContent=text;
    return node;
  }
  function draw(now) {
    lastNow=now;
    const section=id('section');
    if (document.hidden || section.closest('.hourly-selected')) return;
    const tape=tapes[asset]||empty(), points=tape.points.filter(p=>p.t>=now-duration);
    const last=points.at(-1),mode=id('mode').value;
    const key=[asset,tape.ticker,last?.t,points.length,tape.interrupted,mode,Math.floor(now/1000)].join('/');
    if(drawn===key)return;
    drawn=key;
    const plot=id('chart');
    plot.replaceChildren();
    const max=mode==='spread'?Math.max(5,...points.map(p=>Math.ceil(p.spread/5)*5)):100;
    const x=t=>60+(t-now+duration)/duration*637,y=v=>158-v/max*134;
    for(const mark of [0,.5,1]){
      plot.append(svg('line',{x1:60,x2:697,y1:y(mark*max),y2:y(mark*max),class:'qt-grid'}));
      plot.append(svg('text',{x:52,y:y(mark*max)+4,'text-anchor':'end'},fmt(mark*max)));
    }
    for(const [at,label] of [[now-duration,'−2 min'],[now-duration/2,'−1 min'],[now,'Now']]){
      plot.append(svg('text',{x:x(at),y:179,'text-anchor':at===now?'end':at===now-duration?'start':'middle'},label));
    }
    for(const segment of segments(points)){
      const coords=(p,key)=>x(p.t).toFixed(2)+' '+y(p[key]).toFixed(2);
      if(mode==='price' && segment.length>1){
        const edges=segment.map(p=>coords(p,'ask')).concat([...segment].reverse().map(p=>coords(p,'bid')));
        plot.append(svg('path',{d:'M'+edges.join(' L')+' Z',class:'qt-band'}));
      }
      const value=mode==='price'?'mid':'spread';
      if(segment.length>1)plot.append(svg('path',{d:'M'+segment.map(p=>coords(p,value)).join(' L'),class:'qt-line'}));
      const end=segment.at(-1);
      plot.append(svg('circle',{cx:x(end.t),cy:y(end[value]),r:end===last?3.8:2,class:'qt-dot'}));
    }
    const headline=last?fmt(mode==='price'?last.mid:last.spread):'—';
    id('value').textContent=headline;
    id('caption').textContent=mode==='price'?'Last observed YES midpoint':'Last observed YES spread';
    id('book').textContent=last?'Bid '+fmt(last.bid)+' · ask '+fmt(last.ask):'Waiting for a valid two-sided quote';
    id('source').textContent=last?time(last.t)+' · '+last.mode+' · '+points.length+' observed seconds':'No quotes for this window yet';
    id('age').textContent=last?Math.max(0,Math.floor((now-last.t)/1000))+'s ago':'';
    plot.setAttribute('aria-label',asset+' '+id('caption').textContent+' '+headline+'. '+id('book').textContent+'. '+id('source').textContent);
  }
  const controls=id('assets');
  controls.addEventListener('click',event=>{
    const button=event.target.closest('button[data-tape-symbol]');
    if(!button)return;
    asset=button.dataset.tapeSymbol;
    for(const b of controls.querySelectorAll('button'))b.setAttribute('aria-pressed',String(b===button));
    draw(lastNow||Date.now());
  });
  id('mode').onchange=()=>draw(lastNow||Date.now());
  root.LiveQuoteTape={
    record(a,m,s,now){tapes[a]=record(tapes[a],m,s,now);},
    draw,clear(){tapes={};drawn='';}
  };
})(typeof window==='undefined'?globalThis:window);
