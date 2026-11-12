// Dated, public aggregate results. Individual research packets stay private.
(function (root) {
  'use strict';
  const groups = ['pooled', 'BTC', 'ETH', 'SOL'];
  const models = {logistic_calibrated: 'Opening model', market_midpoint: 'Market midpoint', constant_half: 'Always 50%'};
  const pct = n => (100 * n).toLocaleString(undefined, {maximumFractionDigits: 1}) + '%';
  const inUnit = n => Number.isFinite(n) && n >= 0 && n <= 1;

  function validate(study) {
    if (!study || study.protocol !== 'opening_logistic_sigmoid_v1' ||
        !Number.isFinite(Date.parse(study.read_time)) || !Number.isInteger(study.test?.cycles)) return false;
    return groups.every(group => {
      const coverage = study.evaluation?.[group];
      if (study.day && (!coverage || !Number.isInteger(coverage.expected) ||
          !Number.isInteger(coverage.timely_forecasts) || !Number.isInteger(coverage.scored) ||
          coverage.scored < 0 || coverage.scored > coverage.timely_forecasts ||
          coverage.timely_forecasts > coverage.expected || coverage.unscored !== coverage.expected - coverage.scored)) return false;
      return Object.keys(models).every(key => {
      const s = study.evaluation?.[group]?.strategies?.[key];
      if (!s || (s.rows === 0 ? s.brier !== null : !inUnit(s.brier)) || !Number.isInteger(s.rows) || s.rows < 0 || (study.day && s.rows !== coverage.scored) ||
          !Array.isArray(s.reliability_bins) || s.reliability_bins.length !== 5) return false;
      return s.reliability_bins.every((b, i) => b.lower === i / 5 && b.upper === (i + 1) / 5 &&
        Number.isInteger(b.rows) && b.rows >= 0 && (b.rows === 0 ?
          b.mean_probability === null && b.observed_yes_rate === null :
          inUnit(b.mean_probability) && inUnit(b.observed_yes_rate))) &&
        s.reliability_bins.reduce((sum,b) => sum + b.rows, 0) === s.rows;
    }); });
  }
  if (typeof module !== 'undefined' && module.exports) { module.exports = {validate}; return; }

  const id = name => document.getElementById('pilot-' + name);
  let report, flight;
  function svg(tag, attrs, value) {
    const node = document.createElementNS('http://www.w3.org/2000/svg',tag);
    for (const [key, val] of Object.entries(attrs)) node.setAttribute(key,val);
    if (value !== undefined) node.textContent = value;
    return node;
  }
  function reliability(strategy) {
    const chart = id('chart'), table = id('rows');
    chart.replaceChildren();
    table.replaceChildren();
    const x = p => 45 + p * 300, y = p => 183 - p * 150;
    for (const mark of [0,.25,.5,.75,1]) {
      chart.append(svg('line',{x1:x(0),x2:x(1),y1:y(mark),y2:y(mark),class:'pilot-grid'}));
      chart.append(svg('text',{x:35,y:y(mark)+4,'text-anchor':'end'},pct(mark)));
      chart.append(svg('text',{x:x(mark),y:202,'text-anchor':'middle'},pct(mark)));
    }
    chart.append(svg('line',{x1:x(0),x2:x(1),y1:y(0),y2:y(1),class:'pilot-diagonal'}));
    const selected = strategy.reliability_bins;
    selected.forEach((b,index) => {
      if (b.rows) {
        const dot = svg('circle',{cx:x(b.mean_probability),cy:y(b.observed_yes_rate),r:4+6*Math.sqrt(b.rows/strategy.rows),class:'pilot-dot'});
        dot.append(svg('title',{},pct(b.mean_probability)+' forecast · '+pct(b.observed_yes_rate)+' settled YES · '+b.rows+' contracts'));
        chart.append(dot);
      }
      const row = document.createElement('tr');
      for (const value of [20*index+'–'+20*(index+1)+'%',b.rows,b.rows?pct(b.mean_probability):'—',b.rows?pct(b.observed_yes_rate):'—']) {
        const cell = document.createElement('td'); cell.textContent = value; row.append(cell);
      }
      table.append(row);
    });
    chart.setAttribute('aria-label','Forecast versus observed YES frequency. '+selected.filter(b=>b.rows).map(b=>pct(b.mean_probability)+' forecast, '+pct(b.observed_yes_rate)+' observed, '+b.rows+' contracts').join('. '));
  }
  function draw() {
    if (!report) return;
    const live = id('test').value === 'prospective';
    const study = live ? report.prospective : report;
    if (!study) { id('status').textContent='Live results await the first evaluation.'; id('body').hidden=true; return; }
    const coverage = study.evaluation[id('group').value];
    const cohort = coverage.strategies;
    const strategy = cohort[id('model').value];
    const delta = cohort.logistic_calibrated.brier-cohort.market_midpoint.brier;
    id('period').textContent = live ? 'Live forecasts · '+study.day+' UTC' : 'Historical pilot · November 10–11, 2026 UTC';
    id('status').textContent = strategy.rows+' scored contracts · '+study.test.cycles+' shared closing times · as of '+new Date(study.read_time).toLocaleString()+(live ? ' · '+coverage.timely_forecasts+'/'+coverage.expected+' timely forecasts · '+coverage.unscored+' unscored' : ' · retrospective opening forecasts');
    id('note').textContent = live ? 'Partial day. The frozen numerical opening model is compared with market odds saved at the same opening. Results refresh when the operator publishes a report. Thirty complete qualifying days are required by the registered study before its uncertainty analysis. Gemini has no scored results here.' : 'The dotted diagonal marks forecasts that matched observed YES frequency. Larger dots contain more contracts. This earlier two-date pilot used chronological holdout data.';
    id('value').textContent = strategy.rows ? Math.abs(delta).toFixed(4) : '—';
    id('reading').textContent = !strategy.rows ? 'Awaiting scored contracts' : delta > 0 ? 'higher model error than the market midpoint' :
      delta < 0 ? 'lower model error than the market midpoint' : 'difference in model error';
    id('scores').replaceChildren();
    for (const [key,name] of Object.entries(models)) {
      const row = document.createElement('div'), label=document.createElement('span'), val=document.createElement('b');
      label.textContent=name; val.textContent=cohort[key].rows ? cohort[key].brier.toFixed(4) : '—'; row.append(label,val); id('scores').append(row);
    }
    reliability(strategy);
    id('body').hidden=false;
    id('retry').hidden=true;
  }
  function leave() { flight?.abort(); flight=null; }
  async function enter() {
    if (report) { draw(); return; }
    if (flight) return;
    const request = new AbortController();
    flight=request; id('retry').hidden=true; id('status').textContent='Loading the historical forecast study…';
    const timeout = setTimeout(()=>request.abort(),8000);
    try {
      const response=await fetch('/assets/forecast-pilot-v1.json',{signal:request.signal});
      if (!response.ok) throw Error();
      const data=await response.json();
      if (flight!==request) return;
      if (!validate(data) || (data.prospective && !validate(data.prospective))) throw Error();
      report=data; draw();
    } catch {
      if (flight===request) { id('status').textContent='Forecast study unavailable.'; id('retry').hidden=false; }
    } finally { clearTimeout(timeout); if (flight===request) flight=null; }
  }
  id('test').onchange=draw; id('group').onchange=draw; id('model').onchange=draw; id('retry').onclick=enter;
  root.ForecastStudy={enter,leave};
})(typeof window==='undefined'?globalThis:window);
