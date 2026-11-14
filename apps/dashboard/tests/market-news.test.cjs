const test = require('node:test');
const assert = require('node:assert/strict');
const {coverage, matches, rowsFor, href} = require('../public/market-news.js');

test('missing, unavailable, partial and successful empty checks stay distinct', () => {
  assert.equal(coverage(null), 'No capture');
  assert.equal(coverage({coindesk:'ok', fed:'ok', tavily:'scheduled_skip', stories:[]}), 'Checked');
  assert.equal(coverage({coindesk:'ok', fed:'ok', tavily:'quota_or_rate_limit'}), 'Partial');
  assert.equal(coverage({coindesk:'access_denied', fed:'provider_unavailable', tavily:'scheduled_skip'}), 'Unavailable');
});

test('asset filters use explicit headline words and preserve macro context in All', () => {
  const stories = [{title:'Bitcoin and Ethereum ETF flows'}, {title:'Solid demand'}, {title:'Fed release'}];
  assert.deepEqual(rowsFor({stories},'ETH'), [stories[0]]);
  assert.equal(matches(stories[1],'SOL'),false);
  assert.equal(rowsFor({stories},'All').length,3);
  assert.equal(rowsFor(null,'BTC').length,0);
});

test('headline links accept HTTPS and reject embedded credentials or scripts', () => {
  assert.equal(href('https://example.org/story'),'https://example.org/story');
  for (const raw of ['javascript:alert(1)','https://user@evil.example/','data:text/html,test']) {
    assert.equal(href(raw),null);
  }
});

const {checkedCaptures, atCutoff} = require('../public/market-news.js');
const capture = {slot:1800, saved_at:1850, tavily:'scheduled_skip', coindesk:'ok', fed:'ok', stories:[]};
const file = captures => ({schema_version:1, captures});

test('replay distinguishes an absent check from a quiet capture at its original availability time', () => {
  const bySlot = checkedCaptures(file([capture]));
  assert.ok(atCutoff(bySlot,1849).slots.every(s => s === null));
  assert.equal(atCutoff(bySlot,1850).slots.at(-1),capture);
  assert.equal(atCutoff(bySlot,1850).slots.length,8);
});

test('rejects conflicting slots, invalid status, misattributed links and dates beyond capture', () => {
  assert.throws(() => checkedCaptures(file([capture,capture])));
  assert.throws(() => checkedCaptures(file([{...capture, fed:'unknown'}])));
  const story = {url:'https://example.org/story',publisher:'example.org',title:'Bitcoin update',via:'Tavily',published_at:null};
  assert.doesNotThrow(() => checkedCaptures(file([{...capture,tavily:'ok',stories:[story]}])));
  for (const change of [{published_at:1851},{url:'data:text/html,news'},{publisher:'different.org'}]) {
    assert.throws(() => checkedCaptures(file([{...capture,stories:[{...story,...change}]}])));
  }
});
