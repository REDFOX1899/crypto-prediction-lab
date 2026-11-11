// One coherent book per calculation. All displayed thresholds are on the YES probability axis.
(function () {
  'use strict';
  function levels(raw) {
    if (!Array.isArray(raw) || !raw.length) return null;
    const seen = new Set(), result = [];
    for (const pair of raw) {
      if (!Array.isArray(pair) || pair.length !== 2 ||
          pair.some(v => !['number','string'].includes(typeof v) || String(v).trim() === '')) return null;
      const [price, size] = pair.map(Number);
      if (!Number.isFinite(price) || price <= 0 || price >= 1 || !Number.isFinite(size) || size <= 0 || seen.has(price)) return null;
      seen.add(price); result.push({price, size});
    }
    return result.sort((a,b) => b.price - a.price);
  }
  const round = n => Math.round(n * 1e6) / 1e6;
  function measure(book, received, close, now, cost, estimate) {
    if (!Number.isFinite(close) || now >= close) return {reason:'The window is closed or its close time is unavailable.'};
    if (!Number.isFinite(received) || received > now || now-received > 30000) return {reason:'Waiting for a book received in the last 30 seconds.'};
    const yes = levels(book?.yes_dollars), no = levels(book?.no_dollars);
    if (!yes || !no) return {reason:'Both bid ladders need a valid price and size.'};
    const bid = round(100*yes[0].price), ask = round(100*(1-no[0].price));
    if (bid >= ask) return {reason:'The book is locked or crossed. Waiting for the next quote.'};
    if (!Number.isFinite(cost) || cost < 0 || cost > 100) return {reason:'Enter an assumed cost from 0 to 100 cents.'};
    const noBelow = round(bid-cost), yesAbove = round(ask+cost);
    const y = yes[0].size, n = no[0].size;
    const valid = Number.isFinite(estimate) && estimate >= 0 && estimate <= 100;
    const yesNet = valid ? round(estimate-yesAbove) : null, noNet = valid ? round(noBelow-estimate) : null;
    return {noBelow, yesAbove, yesAsk:ask, noAsk:round(100-bid), spread:round(ask-bid),
      pressure:round(100*(y-n)/(y+n)), yesSize:y, noSize:n, yesNet, noNet,
      side:!valid ? null : yesNet>0 ? 'YES' : noNet>0 ? 'NO' : 'Pass'};
  }
  if (typeof module !== 'undefined') module.exports = {measure};
  if (typeof document === 'undefined') return;
  const byId = id => document.getElementById(id);
  const fmt = n => Number(n.toFixed(2)).toString();
  const pct = v => Math.max(0, Math.min(100, v))+'%';
  let latest = {}, activeTicker;
  function draw() {
    const estimate = byId('map-estimate').valueAsNumber, cost = byId('map-cost').valueAsNumber;
    const s = measure(latest.book, Date.parse(latest.received), Date.parse(latest.close), Date.now(), cost, estimate);
    byId('map-evidence').hidden = !!s.reason;
    byId('map-status').textContent = s.reason || (activeTicker+' · '+latest.source+' received '+new Date(latest.received).toISOString().slice(11,19)+' UTC');
    if (s.reason) return;
    byId('map-no-band').style.width = pct(s.noBelow);
    byId('map-yes-band').style.width = pct(100-s.yesAbove);
    byId('map-marker').hidden = s.side === null;
    byId('map-marker').style.left = pct(estimate || 0);
    byId('map-no').textContent = s.noBelow <= 0 ? 'NO cannot clear costs' : 'NO below '+fmt(s.noBelow)+'%';
    byId('map-yes').textContent = s.yesAbove >= 100 ? 'YES cannot clear costs' : 'YES above '+fmt(s.yesAbove)+'%';
    byId('map-asks').textContent = 'YES costs '+fmt(s.yesAsk)+'¢ · NO costs '+fmt(s.noAsk)+'¢ · spread '+fmt(s.spread)+'¢';
    byId('map-result').textContent = s.side === null ? 'Enter your YES estimate to compare both sides.'
      : (s.side === 'Pass' ? 'Pass. Neither side clears your costs.' : s.side+' clears your assumed costs.')+
        ' Expected margin per contract at '+fmt(estimate)+'% YES: YES '+fmt(s.yesNet)+'¢, NO '+fmt(s.noNet)+'¢.';
    byId('map-pressure').textContent = (s.pressure>0?'+':'')+fmt(s.pressure);
    byId('map-pressure-bar').style.left = pct((100+s.pressure)/2);
    byId('map-depth').textContent = fmt(s.yesSize)+' YES bid contracts and '+fmt(s.noSize)+' NO at their respective best prices.';
  }
  window.PriceMap = {update(ticker, book, received, close, source) {
    if (activeTicker !== ticker) byId('map-estimate').value='';
    activeTicker=ticker; latest={book, received, close, source}; draw();
  }};
  for (const id of ['map-estimate','map-cost']) byId(id).addEventListener('input',draw);
})();
