// Hourly thresholds have their own close, quote source and scenario.
(function () {
  'use strict';
  function cents(value) {
    if (typeof value !== 'string' || !value.trim()) return null;
    const n = Number(value);
    return n > 0 && n < 1 ? Math.round(n * 10000) / 100 : null;
  }
  function quantity(value) {
    if (typeof value !== 'string' || !/^\d+(?:\.\d+)?$/.test(value)) return null;
    const n = Number(value);
    return Number.isFinite(n) && n <= Number.MAX_SAFE_INTEGER ? n : null;
  }
  function nearest(markets) {
    const distance = m => {
      const yes = cents(m.yes_ask_dollars), no = cents(m.no_ask_dollars);
      return yes === null || no === null ? Infinity : Math.abs(yes - no);
    };
    return markets.reduce((best, m) => distance(m) < distance(best) ? m : best, markets[0]);
  }
  function bidAsk(strike) {
    const ask = cents(strike.yes_ask_dollars), no = cents(strike.no_ask_dollars);
    if (ask === null) return null;
    const implied = no === null ? null : Math.round((100 - no) * 100) / 100;
    const bid = strike.yes_bid_dollars == null ? implied : cents(strike.yes_bid_dollars);
    if (bid === null) return null;
    return bid <= ask ? {bid, ask} : null;
  }
  function drawRanges(card, strikes) {
    const figure = card.querySelector('.range-chart'), plot = figure.querySelector('svg');
    plot.replaceChildren();
    const descriptions = [];
    const line = (x1,y1,x2,y2,kind) => {
      const node = document.createElementNS('http://www.w3.org/2000/svg','line');
      for (const [key,value] of Object.entries({x1,y1,x2,y2,class:kind})) node.setAttribute(key,value);
      plot.append(node);
    };
    for (const y of [8,48,88]) line(8,y,222,y,'range-grid');
    strikes.forEach((strike,index) => {
      const pair = bidAsk(strike);
      if (!pair) return;
      const x = 17 + index*49, top = 88-pair.ask*.8, bottom = 88-pair.bid*.8;
      line(x,top,x,bottom,'range-stem');
      line(x-5,top,x+5,top,'range-ask');
      line(x-5,bottom,x+5,bottom,'range-bid');
      descriptions.push(strike.yes_sub_title + ' · bid ' + pair.bid + '¢ / ask ' + pair.ask + '¢');
    });
    figure.querySelector('small').textContent = descriptions.length ? 'Low → high strike · '+descriptions.length+'/'+strikes.length+' two-sided quotes' : 'No current two-sided quotes';
    figure.querySelector('output').textContent = descriptions.join(' • ');
    plot.setAttribute('aria-label',descriptions.join('. ') || 'Two-sided quotes unavailable');
  }
  if (typeof module === 'object' && module.exports) { module.exports = {cents, nearest, bidAsk, quantity}; return; }
  const byId = id => document.getElementById(id);
  const cards = new Map();
  let controller, timer, hourly = false;
  function buildCard(asset, name) {
    const card = document.createElement('article');
    card.className = 'home-market-card hour-card';
    card.innerHTML = '<div class="home-market-heading"><span class="home-coin"></span><h3></h3></div><p class="hour-status"></p><label>Settlement threshold<select></select></label><div class="home-asks"><div><span>▲ YES ask</span><strong>—</strong></div><div><span>▼ NO ask</span><strong>—</strong></div></div><figure class="strike-chart"><figcaption>YES asks across nearby strikes</figcaption><div></div></figure><figure class="range-chart"><figcaption>YES bid–ask range</figcaption><div><span>100¢<br>50¢<br>0¢</span><svg viewBox="0 0 232 96" role="img"></svg></div><small></small><details><summary>Read prices</summary><output></output></details></figure><details class="hour-liquidity"><summary>Liquidity at this strike</summary><div class="hour-sizes"><span></span></div><div class="hour-quantities"></div><small>Displayed size can change before execution.</small></details><details class="tape-detail"><summary>Test my probability</summary><label>My chance of YES (%)<input type="number" min="0" max="100" step="any" placeholder="0–100"></label><p class="hour-result" aria-live="polite"></p></details>';
    const coin = card.querySelector('.home-coin');
    coin.classList.add(asset.toLowerCase());
    coin.textContent = {BTC:'₿', ETH:'Ξ', SOL:'◎'}[asset];
    card.querySelector('h3').textContent = name;
    const select = card.querySelector('select'), input = card.querySelector('input');
    const view = {card, asset, select, input, markets:[], stamp:0};
    select.onchange = () => { input.value = ''; draw(view); };
    input.oninput = () => draw(view);
    cards.set(asset, view);
    byId('hourly-cards').append(card);
  }
  function draw(view) {
    const {card, select, input, markets, stamp} = view;
    const m = markets.find(item => item.ticker === select.value);
    const fresh = m && Date.now() < Date.parse(m.close_time) && Date.now() - stamp < 45000;
    const yes = fresh ? cents(m.yes_ask_dollars) : null, no = fresh ? cents(m.no_ask_dollars) : null;
    card.querySelectorAll('.home-asks strong').forEach((el, i) => {
      const v = [yes,no][i];
      el.textContent = v === null ? '—' : v + '¢';
      el.parentElement.style.setProperty('--quote', (v || 0) + '%');
    });
    const chart = card.querySelector('.strike-chart > div');
    const chartKey = [stamp, select.value, Boolean(fresh)].join('/');
    if (chart.dataset.key !== chartKey) {
      chart.dataset.key = chartKey;
      chart.replaceChildren();
      const from = Math.max(0, Math.min(markets.length - 5, select.selectedIndex - 2));
      const nearby = fresh ? markets.slice(from, from + 5) : [];
      drawRanges(card, nearby);
      const quote = fresh && bidAsk(m);
      const bids = quote ? quantity(m.yes_bid_size_fp) : null, asks = quote ? quantity(m.yes_ask_size_fp) : null;
      const track = card.querySelector('.hour-sizes');
      track.hidden = bids === null || asks === null || bids + asks === 0;
      if (!track.hidden) track.style.setProperty('--bids', (100*bids/(bids+asks))+'%');
      const quantities = card.querySelector('.hour-quantities');
      quantities.replaceChildren();
      for (const [label,value] of [['At best YES bid',bids],['At best YES ask',asks],
          ['Contract volume',fresh ? quantity(m.volume_fp) : null],['Open interest',fresh ? quantity(m.open_interest_fp) : null]]) {
        const line = document.createElement('div'), name = document.createElement('span'), total = document.createElement('b');
        name.textContent = label;
        total.textContent = value===null?'—':value.toLocaleString(undefined,{maximumFractionDigits:2})+' contracts';
        line.append(name,total);quantities.append(line);
      }
      for (const strike of nearby) {
        const price = cents(strike.yes_ask_dollars);
        const row = document.createElement('button');
        row.type = 'button';
        row.setAttribute('aria-pressed', String(m.ticker === strike.ticker));
        row.style.setProperty('--quote', (price || 0) + '%');
        row.textContent = strike.yes_sub_title + ' · ' + (price === null ? '—' : price + '¢');
        row.onclick = () => { select.value = strike.ticker; select.onchange(); };
        chart.append(row);
      }
    }
    const result = card.querySelector('.hour-result'), p = input.valueAsNumber;
    if (!input.value || !input.validity.valid) result.textContent = 'Enter your estimate to compare both offers.';
    else if (yes === null || no === null) result.textContent = 'Waiting for two current offers.';
    else result.textContent = 'Expected margin per contract before fees: YES ' + (p-yes).toFixed(1) + '¢ · NO ' + (100-p-no).toFixed(1) + '¢.';
    if (m) card.querySelector('.hour-status').textContent = (fresh ? 'Snapshot' : 'Expired snapshot') + ' ' + new Date(stamp).toLocaleTimeString() + ' · closes ' + new Date(m.close_time).toLocaleTimeString([], {hour:'numeric',minute:'2-digit'});
  }
  async function load() {
    controller?.abort();
    const flight = new AbortController();
    controller = flight;
    await Promise.all([...cards.values()].map(async view => {
      const {card,select,input} = view;
      try {
        const response = await fetch('/api/hourly?asset='+view.asset,{signal:flight.signal,cache:'no-store'});
        if (!response.ok) throw Error();
        const data = await response.json();
        if (flight.signal.aborted) return;
        const previous = select.value;
        view.markets = data.markets;
        view.stamp = Date.parse(data.fetched_at);
        select.replaceChildren(...data.markets.map(m => new Option(m.yes_sub_title,m.ticker)));
        select.value = data.markets.some(m => m.ticker === previous) ? previous : nearest(data.markets)?.ticker || '';
        if (select.value !== previous) input.value = '';
        card.querySelector('.hour-status').textContent = 'No open thresholds for the next hourly close.';
      } catch {
        if (flight.signal.aborted) return;
        view.markets = []; select.replaceChildren(); input.value = '';
        card.querySelector('.hour-status').textContent = 'Hourly quotes unavailable · retrying';
      }
      select.disabled = !view.markets.length;
      draw(view);
    }));
    if (!flight.signal.aborted) timer = setTimeout(load,25000);
  }
  function choose(on) {
    hourly = on; clearTimeout(timer); controller?.abort();
    byId('hourly-board').hidden = !on;
    byId('quarter-cards').hidden = on;
    byId('tape-short').setAttribute('aria-pressed',String(!on));
    byId('tape-hour').setAttribute('aria-pressed',String(on));
    byId('home-workspace').classList.toggle('hourly-selected',on);
    if (on) { cards.forEach(draw); load(); }
  }
  for (const [asset,name] of [['BTC','Bitcoin'],['ETH','Ethereum'],['SOL','Solana']]) buildCard(asset,name);
  byId('tape-short').onclick = () => choose(false);
  byId('tape-hour').onclick = () => choose(true);
  window.HourlyTape = {leave:() => choose(false), tick:() => {if (hourly) cards.forEach(draw);}};
})();
// Hover previews use the same accessible disclosure as taps and keyboard activation.
if (typeof document !== 'undefined' && matchMedia('(hover: hover)').matches) {
  const guide = document.querySelector('.tape-help');
  let hoverTimer;
  guide.addEventListener('pointerenter', () => { hoverTimer = setTimeout(() => { guide.open = true; }, 250); });
  guide.addEventListener('pointerleave', () => { clearTimeout(hoverTimer); if (!guide.contains(document.activeElement)) guide.open = false; });
  guide.addEventListener('keydown', event => { if (event.key === 'Escape') { guide.open = false; guide.querySelector('summary').focus(); } });
}
