(function (root) {
  'use strict';
  const STEP = 900;
  const terms = {
    BTC: /\b(bitcoin|bitcoins|btc)\b/i,
    ETH: /\b(ethereum|ether|eth)\b/i,
    SOL: /\b(solana|sol)\b/i
  };
  function matches(story, asset) {
    return asset === 'All' || Boolean(terms[asset]?.test(story.title));
  }
  function rowsFor(slot, asset) {
    return (slot?.stories || []).filter(story => matches(story, asset));
  }
  function coverage(slot) {
    if (!slot) return 'No capture';
    const sources = [slot.coindesk, slot.fed, slot.tavily].filter(value => value !== 'scheduled_skip');
    return sources.every(value => value === 'ok') ? 'Checked'
      : sources.includes('ok') ? 'Partial' : 'Unavailable';
  }
  function href(raw) {
    try {
      const u = new URL(raw);
      if (u.protocol === 'https:' && !u.username && !u.password) return u.href;
    } catch {}
    return null;
  }
  const dateLimit = 253402300799; // Last second in year 9999.
  const timestamp = n => Number.isSafeInteger(n) && n > 0 && n <= dateLimit;
  const sourceStates = new Set(['ok','access_denied','quota_or_rate_limit','provider_unavailable',
    'key_unavailable','response_too_large','invalid_response']);
  function checkedCaptures(data) {
    if (data?.schema_version !== 1 || !Array.isArray(data.captures)
        || !data.captures.length || data.captures.length > 1344) throw Error('Invalid capture file');
    const bySlot = new Map();
    for (const s of data.captures) {
      if (!timestamp(s?.slot) || s.slot % STEP !== 0 || !timestamp(s.saved_at) || s.saved_at < s.slot
          || bySlot.has(s.slot) || !sourceStates.has(s.coindesk) || !sourceStates.has(s.fed)
          || !(sourceStates.has(s.tavily) || s.tavily === 'scheduled_skip')
          || !Array.isArray(s.stories) || s.stories.length > 48) throw Error('Invalid slot');
      for (const r of s.stories) {
        if (!r || typeof r.title !== 'string' || !r.title.trim() || r.title.length > 600
            || typeof r.url !== 'string' || r.url.length > 2048 || !href(r.url)
            || new URL(r.url).hostname !== r.publisher
            || !['CoinDesk RSS','Federal Reserve RSS','Tavily'].includes(r.via)
            || (r.published_at !== null && (!timestamp(r.published_at) || r.published_at > s.saved_at))) {
          throw Error('Invalid headline');
        }
      }
      bySlot.set(s.slot, s);
    }
    return bySlot;
  }
  function atCutoff(bySlot, at) {
    if (!timestamp(at)) throw Error('Invalid cutoff');
    const first = Math.floor(at/STEP)*STEP - 7*STEP;
    return {at, first_slot:first, slots:Array.from({length:8}, (_, i) => {
      const s = bySlot.get(first + i*STEP);
      return s && s.saved_at <= at ? s : null;
    })};
  }
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = {matches, rowsFor, coverage, href, checkedCaptures, atCutoff};
    return;
  }
  const el = id => document.getElementById(id);
  const make = (tag, text, cls) => {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (cls) node.className = cls;
    return node;
  };
  const utc = seconds => new Date(seconds * 1000).toISOString().slice(11, 16);
  const date = seconds => new Date(seconds * 1000).toISOString().slice(0, 19).replace('T', ' ') + ' UTC';
  let active = false, asOf = null, mode = 'latest', entry = null, asset = 'All', board = null, selected = null;
  let captures = null, fileVersion = 0;
  const file = el('news-file');
  const status = el('news-status'), bars = el('news-bars'), list = el('news-headlines');
  function controls() {
    el('news-cutoff').disabled = !asOf || !captures;
    el('news-now').disabled = !captures;
    el('news-cutoff').setAttribute('aria-pressed', String(mode === 'outcome'));
    el('news-now').setAttribute('aria-pressed', String(mode === 'latest'));
    for (const b of document.querySelectorAll('[data-news-asset]')) {
      b.setAttribute('aria-pressed', String(asset === b.dataset.newsAsset));
    }
  }
  function clear() {
    board = null; selected = null; bars.replaceChildren(); list.replaceChildren();
    el('news-source-state').textContent = '';
  }
  function draw() {
    controls();
    if (!board) return;
    bars.replaceChildren(); list.replaceChildren();
    const slots = board.slots, ready = slots.filter(s => coverage(s) === 'Checked').length;
    const partial = slots.filter(s => coverage(s) === 'Partial').length;
    const peak = Math.max(1, ...slots.map(s => rowsFor(s,asset).length));
    const end = date(board.at);
    status.textContent = ({entry:'Bet entry ',outcome:'Outcome cutoff ',latest:'Last saved check '}[mode]) + end +
      ' · ' + ready + ' of 8 slots fully checked' + (partial ? ' · ' + partial + ' partial' : '');
    slots.forEach((s, i) => {
      const label = coverage(s), count = rowsFor(s, asset).length, start = board.first_slot + i * STEP;
      const usable = label === 'Checked' || label === 'Partial';
      const b = make('button', undefined, 'news-bin'); b.type = 'button';
      b.dataset.state = label.toLowerCase().replace(' ', '-');
      b.setAttribute('aria-pressed', String(selected === i));
      b.setAttribute('aria-label', utc(start) + ' UTC, ' + label + (usable ? ', ' + count + ' new links' : ''));
      const stem = make('span', undefined, 'news-stem');
      if (usable) { const bar = make('i'); bar.style.height = (count / peak * 100) + '%'; stem.append(bar); }
      b.append(make('strong', usable ? String(count) : '—'), stem, make('small', utc(start)));
      b.addEventListener('click', () => {selected = selected === i ? null : i; draw(); bars.children[i].focus();});
      bars.append(b);
    });
    const chosen = selected === null ? slots : [slots[selected]];
    const hits = chosen.flatMap(s => rowsFor(s, asset).map(story => ({story, saved:s.saved_at})));
    hits.sort((a,b) => b.saved - a.saved || (b.story.published_at || 0) - (a.story.published_at || 0));
    const state = el('news-source-state');
    const detail = selected === null ? slots.filter(Boolean).at(-1) : slots[selected];
    if (detail) {
      const names = value => ({ok:'checked', scheduled_skip:'scheduled pause'}[value] || String(value).replaceAll('_',' '));
      state.textContent = 'Capture ' + date(detail.saved_at) + ' · Tavily ' + names(detail.tavily) +
        ' · CoinDesk ' + names(detail.coindesk) + ' · Federal Reserve ' + names(detail.fed);
    } else { state.textContent = 'No saved source check for this selection.'; }
    if (!hits.length) {
      list.append(make('p', chosen.some(s=>['Checked','Partial'].includes(coverage(s))) ?
        'No new matching links in the available captures.' : 'Headline coverage is unavailable for this selection.'));
    }
    const items = make('ul');
    for (const {story,saved} of hits) {
      const link = href(story.url); if (!link) continue;
      const li = make('li'), anchor = make('a',story.title);
      anchor.href = link; anchor.target = '_blank'; anchor.rel = 'noopener noreferrer';
      li.append(anchor, make('small', story.publisher + ' · ' + story.via +
        ' · First saved ' + date(saved) + ' · Published ' + (story.published_at ? date(story.published_at) : 'time unavailable')));
      items.append(li);
    }
    if (items.children.length) list.append(items);
  }
  function load() {
    clear(); controls();
    if (!active || !captures) return;
    const cutoff = mode === 'latest' ? Math.max(...Array.from(captures.values(), s => s.saved_at))
      : mode === 'entry' ? entry : asOf;
    if (!timestamp(cutoff)) {status.textContent = 'Choose a valid UTC time.'; return;}
    board = atCutoff(captures, cutoff); draw();
  }
  function reset() {
    fileVersion++; captures = null; file.value = ''; mode = 'latest'; asset = 'All'; entry = null;
    el('news-entry').value = ''; el('news-review').hidden = true;
    clear(); controls(); status.textContent = 'Choose a saved news file to begin.';
  }
  file.addEventListener('change', async () => {
    const picked = file.files[0], version = ++fileVersion;
    captures = null; mode = 'latest'; entry = null; asset = 'All'; el('news-entry').value = '';
    clear(); controls(); el('news-review').hidden = true;
    status.textContent = 'Reading the saved news file…';
    try {
      if (!picked || picked.size > 8*1024*1024) throw Error();
      const text = await picked.text();
      if (!active || version !== fileVersion) return;
      captures = checkedCaptures(JSON.parse(text));
      el('news-review').hidden = false; load();
    } catch {
      if (active && version === fileVersion) {reset(); status.textContent = 'Choose a valid version 1 news capture (up to 8 MB).';}
    }
  });
  el('news-now').addEventListener('click', () => {mode = 'latest'; load();});
  el('news-cutoff').addEventListener('click', () => {mode = 'outcome'; load();});
  for (const b of document.querySelectorAll('[data-news-asset]')) {
    b.addEventListener('click', () => {asset = b.dataset.newsAsset; draw(); controls();});
  }
  el('news-entry-form').addEventListener('submit', event => {
    event.preventDefault();
    entry = Date.parse(el('news-entry').value + 'Z') / 1000;
    mode = 'entry'; load();
  });
  root.MarketNews = {
    enter() {active = true; load();},
    leave() {active = false; asOf = null; reset();},
    cutoff(seconds) {
      if (asOf === seconds) return;
      asOf = seconds; controls(); if (mode === 'outcome' && active) load();
    }
  };
})(typeof window === 'undefined' ? globalThis : window);
