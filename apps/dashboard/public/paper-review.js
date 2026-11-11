// Operator report stays in local memory. Public services do not receive the file.
(function () {
  'use strict';
  const labels = {validated:'Bounded policy',point_scenario:'Model point estimate',midpoint_scenario:'Market midpoint',market_favorite:'Buy market favorite',point_stress:'Model under stress'};
  const number = x => x == null || x === '' || !Number.isFinite(Number(x)) ? null : Number(x);
  const show = (v, factor=1, unit='') => number(v)===null?'—':(number(v)*factor).toLocaleString('en-US',{maximumFractionDigits:3})+unit;
  function visible(row, strategy, phase, asof) {
    const result = {status:'Waiting for the decision'};
    const at = phase==='review' ? Date.parse(asof) : phase==='book' ? (row.open_ts+55)*1000 : Date.parse(row.committed_at);
    if (!row.scorecard || !Number.isFinite(at) || Date.parse(row.committed_at)>at) return {status:row.forecast_status};
    const sim = row.strategies[strategy];
    result.card=row.scorecard; result.intent=sim?.side || 'Pass'; result.reason = sim?.status==='pass' ? sim.reason : null;
    result.cap=sim?.cap; result.at=row.committed_at;
    if (phase!=='decision' && Date.parse(row.candidate_at)<=at) result.candidate=row.candidate_at;
    if (phase!=='decision' && sim?.execution_at && Date.parse(sim.execution_at)<=at) {
      result.execution={status:sim.status,qty:sim.filled_qty,levels:sim.fills,remainder:sim.ioc_canceled_qty};
    } else if (phase==='review' && sim?.status==='unknown_execution') result.gap=sim.reason;
    if (phase==='review' && Date.parse(row.result_available_at)<=at) {
      result.outcome=row.official_result || 'Unclassified'; result.published=row.result_available_at;
      if (Date.parse(sim?.settled_at)<=at) result.net=sim.net_pnl;
    }
    return result;
  }
  if(typeof module!=='undefined') module.exports={visible};
  if(typeof document==='undefined') return;
  let report, version=0;
  const $=id=>document.getElementById(id);
  const make=(tag,text,cls)=>{const n=document.createElement(tag);if(text!=null)n.textContent=text;if(cls)n.className=cls;return n;};
  const pretty=code=>String(code||'').replaceAll('_',' ');
  function bar(title,value,unit,scale=1) {
    const line=make('div',null,'paper-meter');line.append(make('span',title),make('strong',show(value,scale,unit)));
    const track=make('div',null,'paper-meter-track'), mark=make('i');
    if(number(value)!==null) mark.style.width=Math.max(0,Math.min(100,100*number(value)))+'%';
    track.append(mark);line.append(track);return line;
  }
  function aggregate(target,key) {
    const totals=report.strategies[key], ev=report.forecast_evaluation.pooled;
    target.append(make('h3','Day review · '+labels[key]),make('p',totals.scheduled+' windows · '+totals.passes+' passes · '+totals.filled+' simulated fills · '+totals.verified_no_fill+' no-fills · '+totals.unknown_execution+' unknown executions.'));
    target.append(make('p','Net '+show(totals.net_pnl,1,' USD')+' · modeled fees '+show(totals.modeled_fees,1,' USD')+' · maximum settled drawdown '+show(totals.settled_drawdown,1,' USD')+'. Unknown-inclusive net range '+totals.net_bounds.map(x=>show(x,1,' USD')).join(' to ')+'.'));
    const path=totals.settled_equity_path||[];
    if(path.length){
      const values=[288,...path.map(e=>number(e.equity))], t=[report.cycles[0].open_ts*1000,...path.map(e=>Date.parse(e.at))];
      const low=Math.min(...values), high=Math.max(...values);
      const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');
      svg.setAttribute('viewBox','0 0 500 110');svg.setAttribute('role','img');svg.setAttribute('aria-label','Settled virtual equity by official result availability in UTC. Low '+show(low)+' and high '+show(high)+' dollars.');
      svg.classList.add('paper-equity');
      const line=document.createElementNS(svg.namespaceURI,'path');
      let d='';values.forEach((v,i)=>{const x=4+492*(t[i]-t[0])/Math.max(1,t.at(-1)-t[0]);const y=104-98*(v-low)/(high-low||1);d+=i?' H'+x+' V'+y:'M'+x+' '+y;});line.setAttribute('d',d);svg.append(line);target.append(svg);
      target.append(make('p','Settled equity · $288 starting balance → '+show(values.at(-1),1,' USD')+' · '+path.length+' settlements. Open positions excluded.'));
    }
    const bins=ev.strategies.logistic_calibrated.reliability_bins;
    target.append(make('h3','Calibration across '+ev.scored+' matched forecasts'));
    const table=make('table');const head=make('tr');
    for(const h of ['YES forecast band','Count','Mean forecast','Official YES'])head.append(make('th',h));table.append(head);
    for(const b of bins){const tr=make('tr');for(const v of [show(b.lower,100)+'–'+show(b.upper,100,'%'),b.rows,show(b.mean_probability,100,'%'),show(b.observed_yes_rate,100,'%')])tr.append(make('td',v));table.append(tr);}target.append(table);
  }
  function render(){
    const target=$('paper-detail');target.replaceChildren();if(!report)return;
    const r=report.cycles[Number($('paper-cycle').value)], key=$('paper-strategy').value, phase=$('paper-phase').value;
    const v=visible(r,key,phase,report.asof);
    if(v.card){
      const cols=make('div',null,'map-columns'), left=make('div'),right=make('div');
      left.append(make('h3','YES probability'),bar('Saved model',v.card.p_yes,'%',100),bar('Opening midpoint',v.card.midpoint,'%',100));
      right.append(make('h3','One-contract prices'));
      for(const s of ['YES','NO'])right.append(bar(s+' offer',v.card[s].offer,'¢',100),bar(s+' model watch price',v.card[s].point_watch_price,'¢',100));
      cols.append(left,right);target.append(cols);
      if(v.card.YES.estimated_margin!=null && v.card.NO.estimated_margin!=null){
        target.append(make('p','At the saved probabilities and opening offers, the one-contract margin after modeled fees was '+show(v.card.YES.estimated_margin,100,'¢')+' for YES and '+show(v.card.NO.estimated_margin,100,'¢')+' for NO. The model scenario requires 1.5¢ to cover its target margin and execution allowance.'));
      }
      if(key==='validated')target.append(make('p','The policy needs a tested lower bound on the chance of winning before it can price a side conservatively. This model currently supplies a point estimate.'));
      if(key==='market_favorite')target.append(make('p','This comparison buys the side favored by the midpoint, with a limit equal to its opening offer. A tie passes.'));
      if(key==='point_stress')target.append(make('p','The stressed scenario allows half the displayed size, worsens each price by one valid tick and requires a book requested at least ten seconds after commitment.'));
      target.append(make('p',labels[key]+' · '+v.intent+(v.reason?' · '+pretty(v.reason):' up to '+show(v.cap,100,'¢'))+' · committed '+v.at));
    }else target.append(make('p',pretty(v.status)));
    if(v.candidate) target.append(make('p','Later book available '+v.candidate));
    if(v.execution) target.append(make('p',pretty(v.execution.status)+' · '+show(v.execution.qty)+' contracts simulated · '+show(v.execution.remainder)+' canceled. '+v.execution.levels.map(l=>show(l.qty)+' at '+show(l.price,100,'¢')).join(', ')));
    if(v.gap)target.append(make('p',pretty(v.gap)));
    if(v.outcome)target.append(make('p','Official '+v.outcome.toUpperCase()+' · available '+v.published+(v.net!=null?' · net '+show(v.net,1,' USD'):'')));
    if(phase==='review')aggregate(target,key);
  }
  function clear(){
    version++;report=null;$('paper-file').value='';$('paper-controls').hidden=true;$('paper-cycle').replaceChildren();$('paper-detail').replaceChildren();$('paper-load').textContent='';
  }
  window.PaperReview={clear};
  $('paper-file').addEventListener('change',async()=>{
    const file=$('paper-file').files[0], id=++version;report=null;$('paper-detail').replaceChildren();$('paper-controls').hidden=true;
    try{
      if(!file||file.size>8*1024*1024)throw Error();
      const data=JSON.parse(await file.text());if(id!==version)return;
      if(data.schema!=='opening_ioc_study_v1'||!Array.isArray(data.cycles)||!data.cycles.length||data.cycles.length>288||
         !data.forecast_evaluation?.pooled?.strategies?.logistic_calibrated||Object.keys(labels).some(k=>!data.strategies?.[k]))throw Error();
      const select=$('paper-cycle');select.replaceChildren();data.cycles.forEach((r,i)=>select.add(new Option(r.asset+' · '+new Date(r.open_ts*1000).toISOString().slice(11,16)+' UTC',i)));
      report=data;$('paper-phase').value='decision';$('paper-controls').hidden=false;$('paper-load').textContent=data.day+' · '+data.cycles.length+' scheduled windows · retrospective paper study';render();
    }catch(error){if(id===version){clear();$('paper-load').textContent=error.name==='NotReadableError'?'Your browser could not read that file. Choose a copy in a folder it can access.':'Choose a valid daily paper report (up to 8 MB).';}}
  });
  for(const id of ['paper-cycle','paper-strategy','paper-phase'])$(id).addEventListener('change',()=>{if(id==='paper-cycle')$('paper-phase').value='decision';render();});
})();
