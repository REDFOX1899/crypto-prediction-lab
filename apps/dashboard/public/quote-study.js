(function (root) {
  'use strict';

  const bandNames = ['0–20', '20–40', '40–60', '60–80', '80–100'];
  const number = new Intl.NumberFormat(undefined, {maximumFractionDigits: 1});
  const utcDate = new Intl.DateTimeFormat('en-US', {month: 'short', day: 'numeric', year: 'numeric', timeZone: 'UTC'});

  function bandStats(bin) {
    if (!bin || !Number.isInteger(bin.n) || bin.n <= 0 || !Number.isInteger(bin.yes)
        || bin.yes < 0 || bin.yes > bin.n || !Number.isInteger(bin.sum_ask_bps)) return null;
    return {share: 100 * bin.yes / bin.n, averageCents: bin.sum_ask_bps / (100 * bin.n)};
  }

  if (typeof module === 'object' && module.exports) {
    module.exports = {bandStats};
    return;
  }

  const el = id => document.getElementById(id);
  let asset = 'BTC', minute = 5, band = 2;
  let board = null, savedAt = 0, pending = null, message = 'Loading the archived contracts…';
  const bandButtons = bandNames.map((label, index) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'study-band';
    const bar = document.createElement('span');
    bar.className = 'study-fill';
    const marker = document.createElement('span');
    marker.className = 'study-breakeven';
    const value = document.createElement('span');
    value.className = 'study-value';
    const name = document.createElement('span');
    name.className = 'study-band-name';
    name.textContent = label;
    button.append(bar, marker, value, name);
    button.addEventListener('click', () => { band = index; render(); });
    el('study-bands').append(button);
    return {button, value};
  });

  function render() {
    const cohort = board?.series.find(s => s.asset === asset)?.cutoffs.find(c => c.minute === minute);
    const bins = cohort?.bins || [];
    bandButtons.forEach(({button, value}, index) => {
      const bin = bins[index], stats = bandStats(bin);
      const pct = stats ? number.format(stats.share) + '%' : '—';
      value.textContent = pct;
      button.style.setProperty('--share', (stats?.share || 0) + '%');
      button.style.setProperty('--break-even', (stats?.averageCents || 0) + '%');
      button.dataset.available = String(Boolean(stats));
      button.setAttribute('aria-pressed', String(band === index));
      button.setAttribute('aria-label', bandNames[index] + ' cents, ' +
        (stats ? pct + ' settled YES, ' + bin.yes + ' of ' + bin.n + ' contracts, break-even before fees ' + number.format(stats.averageCents) + ' percent' : 'no eligible quotes'));
      button.disabled = !board;
    });
    const selected = bins[band], stats = bandStats(selected);
    el('study-selection').textContent = asset + '  /  Minute ' + minute + '  /  ' + bandNames[band] + '¢';
    el('study-stat').textContent = stats ? number.format(stats.share) + '%' : '—';
    const gap = el('study-gap');
    gap.hidden = !stats;
    if (stats) {
      const points = Math.round((stats.share - stats.averageCents) * 10) / 10;
      gap.textContent = (points > 0 ? '+' : '') + number.format(points) + ' pts vs. break-even';
    }
    el('study-reading').textContent = stats
      ? selected.yes + ' of ' + selected.n + ' contracts settled YES. Average YES ask ' + number.format(stats.averageCents) + '¢.'
      : board ? 'No eligible quotes in this price band.' : message;
    if (board) {
      el('study-period').textContent = utcDate.formatRange(new Date(board.start_day + 'T00:00:00Z'), new Date(board.end_day + 'T00:00:00Z')) + ' UTC';
      const eligible = bins.reduce((sum, b) => sum + b.n, 0);
      const coverage = [eligible + ' of ' + cohort.expected + ' windows included'];
      if (cohort.settled > eligible) coverage.push((cohort.settled - eligible) + ' without an eligible quote');
      if (cohort.expected > cohort.settled) coverage.push((cohort.expected - cohort.settled) + ' without a result');
      el('study-coverage').textContent = coverage.join(' · ');
    } else {
      el('study-period').textContent = 'Archived windows · UTC';
      el('study-coverage').textContent = pending ? 'Checking coverage…' : 'Archive unavailable';
    }
    el('study-retry').hidden = Boolean(board || pending);
    for (const button of el('study-assets').querySelectorAll('button')) {
      button.setAttribute('aria-pressed', String(asset === button.dataset.asset));
    }
  }

  function leave() {
    pending?.abort();
    pending = null;
  }

  async function enter() {
    leave();
    if (board && Date.now() - savedAt < 300000) { render(); return; }
    const controller = new AbortController();
    pending = controller;
    board = null;
    message = 'Loading the archived contracts…';
    render();
    const timeout = setTimeout(() => controller.abort(), 10000);
    try {
      const response = await fetch('/api/research/quote-study', {signal: controller.signal});
      if (!response.ok) throw Error();
      const result = await response.json();
      if (pending !== controller) return;
      if (result.schema_version === 1 && result.series?.length === 3) {
        board = result;
        savedAt = Date.now();
      } else {
        message = 'No quote study is connected to this dashboard.';
      }
    } catch {
      if (pending === controller) message = 'The archive could not be reached. Reload to try again.';
    } finally {
      clearTimeout(timeout);
      if (pending === controller) { pending = null; render(); }
    }
  }

  el('study-assets').addEventListener('click', event => {
    const symbol = event.target.closest('button')?.dataset.asset;
    if (symbol) { asset = symbol; render(); }
  });
  el('study-minute').addEventListener('change', event => { minute = Number(event.target.value); render(); });
  el('study-retry').addEventListener('click', enter);
  root.QuoteStudy = {enter, leave};
})(typeof window !== 'undefined' ? window : globalThis);
