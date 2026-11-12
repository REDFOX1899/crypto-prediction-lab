// Indicators describe observed CMC samples, indexed by first collection time.
(function (root) {
  'use strict';
  const maxGap = 11 * 60000;
  const visibleHours = 6;
  function samples(history, asset, now) {
    const rows = [], seen = new Set();
    let prior, newestSource = -Infinity, segment = 0;
    const ordered = [...history].sort((a,b)=>Date.parse(a.at)-Date.parse(b.at));
    for (const sample of ordered) {
      const t=Date.parse(sample.at), q=sample.quotes?.find(q=>q.symbol===asset);
      if (!Number.isFinite(t) || t > now || seen.has(t)) continue;
      seen.add(t);
      const source=Date.parse(q?.source_at), price=q?.price;
      if (!(Number.isFinite(price) && price>0 && source<=t && t-source<=maxGap)) {
        prior=null;
        continue;
      }
      if (prior && source===prior.source && price===prior.price) continue;
      if (source<=newestSource) { prior=null; continue; }
      newestSource=source;
      if (!prior || t-prior.t>maxGap || source-prior.source>maxGap) segment++;
      const row={t, source, price, segment};
      rows.push(row); prior=row;
    }
    return rows;
  }
  function indicators(rows) {
    let run=[];
    return rows.map((row,i)=>{
      if (!i || rows[i-1].segment!==row.segment) run=[];
      run.push(row.price);
      if (run.length>20) run.shift();
      let mean=null, lower=null, upper=null, rsi=null, volatility=null;
      if (run.length===20) {
        mean=run.reduce((s,v)=>s+v,0)/20;
        const sd=Math.sqrt(run.reduce((s,v)=>s+(v-mean)**2,0)/20);
        lower=mean-2*sd; upper=mean+2*sd;
      }
      if (run.length>=15) {
        let up=0,down=0,sumSquares=0;
        for (let j=run.length-14; j<run.length; j++) {
          const delta=run[j]-run[j-1];
          up+=Math.max(0,delta); down+=Math.max(0,-delta);
          sumSquares+=Math.log(run[j]/run[j-1])**2;
        }
        rsi=up+down>0 ? 100*up/(up+down) : null;
        volatility=100*Math.sqrt(sumSquares);
      }
      return {...row,mean,lower,upper,rsi,volatility};
    });
  }
  if (typeof module==='object' && module.exports) { module.exports={samples,indicators}; return; }
  const id=k=>document.getElementById('tech-'+k);
  const stamp=t=>new Date(t).toLocaleString([], {month:'short',day:'numeric',hour:'2-digit',minute:'2-digit',timeZoneName:'short'});
  const clock=t=>new Date(t).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'});
  const price=(asset,v)=>v===null?'—':v.toLocaleString(undefined,{style:'currency',currency:'USD',minimumFractionDigits:2,maximumFractionDigits:asset==='SOL'?4:2});
  let history=[], visible=[], selected=null, layout;
  function node(tag,attrs,text) {
    const el=document.createElementNS('http://www.w3.org/2000/svg',tag);
    for (const [k,v] of Object.entries(attrs)) el.setAttribute(k,v);
    if (text!==undefined) el.textContent=text;
    return el;
  }
  function paths(rows,key,x,y) {
    let path='',previous;
    for (const row of rows) {
      if (row[key]===null) {previous=null;continue;}
      path+=(previous?.segment===row.segment?' L':' M')+x(row.t).toFixed(2)+' '+y(row[key]).toFixed(2);
      previous=row;
    }
    return path;
  }
  function axes(chart,x,y,lo,hi,from,to,isRsi,asset) {
    for (const fraction of [0,.5,1]) {
      const v=lo+(hi-lo)*fraction;
      chart.append(node('line',{x1:84,x2:714,y1:y(v),y2:y(v),class:'tech-grid'}));
      chart.append(node('text',{x:77,y:y(v)+4,'text-anchor':'end'},isRsi?Math.round(v):price(asset,v)));
      const t=from+(to-from)*fraction;
      chart.append(node('text',{x:x(t),y:194,'text-anchor':fraction===0?'start':fraction===1?'end':'middle'},clock(t)));
    }
  }
  function readout() {
    const row=visible[selected??visible.length-1], asset=id('asset').value;
    id('at').textContent=row?'Collected '+stamp(row.t)+' · source '+clock(row.source):'No eligible observation';
    id('price').textContent=row?price(asset,row.price):'—';
    id('rsi').textContent=row?.rsi!=null?row.rsi.toFixed(1):'—';
    id('vol').textContent=row?.volatility!=null?row.volatility.toFixed(2)+'%':'—';
    id('band').textContent=row?.lower!=null?price(asset,row.lower)+' – '+price(asset,row.upper):'—';
    const cursor=id('cursor'), dot=id('point');
    const value=id('mode').value==='rsi'?row?.rsi:row?.price;
    if (row && cursor && layout) {
      cursor.setAttribute('x1',layout.x(row.t)); cursor.setAttribute('x2',layout.x(row.t));
      dot.setAttribute('visibility',value==null?'hidden':'visible');
      if(value!=null){dot.setAttribute('cx',layout.x(row.t));dot.setAttribute('cy',layout.y(value));}
    }
    id('scrub').setAttribute('aria-valuetext',row?stamp(row.t)+' · '+price(asset,row.price):'No observations');
  }
  function draw() {
    const asset=id('asset').value, mode=id('mode').value, now=Date.now();
    const rows=indicators(samples(history,asset,now)), last=rows.at(-1);
    visible=last?rows.filter(r=>r.t>=last.t-visibleHours*3600000):[];
    const chart=id('chart'), slider=id('scrub');
    slider.max=Math.max(0,visible.length-1); slider.disabled=visible.length<2;
    if(selected!==null) selected=Math.min(selected,visible.length-1);
    slider.value=selected??Math.max(0,visible.length-1);
    chart.replaceChildren(); layout=null;
    if(!visible.length) {
      id('state').textContent='CoinMarketCap observations unavailable.';
      chart.setAttribute('aria-label','No eligible spot observations');
      id('legend').textContent=''; readout(); return;
    }
    id('state').textContent=visible.length+' observed prices · latest '+stamp(last.t)+(now-last.t>maxGap?' · stale':'');
    const rsi=mode==='rsi';
    id('legend').textContent=rsi?'Rolling RSI · 14 observed price changes':'Observed price · dashed mean and outer 20-sample bands';
    const values=rsi?[0,100]:visible.flatMap(r=>[r.price,r.lower,r.upper].filter(Number.isFinite));
    let lo=Math.min(...values),hi=Math.max(...values);
    if (!rsi) {const pad=Math.max((hi-lo)*.06,hi*.00001); lo-=pad;hi+=pad;}
    const from=visible[0].t, to=Math.max(from+1,last.t);
    const x=t=>84+(t-from)/(to-from)*630, y=v=>163-(v-lo)/(hi-lo)*139;
    layout={x,y,from,to}; axes(chart,x,y,lo,hi,from,to,rsi,asset);
    const keys=rsi?['rsi']:['lower','upper','mean','price'];
    for(const key of keys)chart.append(node('path',{d:paths(visible,key,x,y),class:'tech-line tech-'+key}));
    chart.append(node('line',{id:'tech-cursor',y1:20,y2:168,class:'tech-cursor'}),node('circle',{id:'tech-point',r:4,class:'tech-point'}));
    chart.setAttribute('aria-label',asset+' '+id('legend').textContent+'. '+visible.length+' observations. Use the time slider for exact values.');
    readout();
  }
  function show(index) {
    selected=Math.max(0,Math.min(visible.length-1,index));
    id('scrub').value=selected;readout();
  }
  id('scrub').oninput=event=>show(Number(event.target.value));
  id('chart').addEventListener('pointerdown',event=>{
    const chart=id('chart'),matrix=chart.getScreenCTM();
    if (!layout || !visible.length || !matrix) return;
    const point=new DOMPoint(event.clientX,event.clientY).matrixTransform(matrix.inverse());
    const target=layout.from+(point.x-84)/630*(layout.to-layout.from);
    show(visible.reduce((closest,r,i)=>Math.abs(r.t-target)<Math.abs(visible[closest].t-target)?i:closest,0));
  });
  id('asset').onchange=()=>{selected=null;draw();};
  id('mode').onchange=draw;
  root.TechnicalChart={update(next){
    if(history===next) return;
    const priorTime=selected===null?null:visible[selected]?.t;
    history=next; selected=null; draw();
    if(priorTime!=null){const index=visible.findIndex(r=>r.t===priorTime);if(index>=0)show(index);}
  }};
})(typeof window==='undefined'?globalThis:window);
