const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {bandStats} = require('../public/quote-study.js');

test('quote comparison uses the mean ask in cents and the official YES count', () => {
  assert.deepEqual(bandStats({n:4, yes:1, sum_ask_bps:10000}), {share:25, averageCents:25});
  assert.deepEqual(bandStats({n:4, yes:0, sum_ask_bps:400}), {share:0, averageCents:1});
  assert.equal(bandStats({n:0, yes:0, sum_ask_bps:0}), null);
});

test('leaving cancels the study and a late reply cannot replace the next visit', async () => {
  const nodes = new Map(), requests = [];
  function node() {
    return {style:{setProperty(){}}, dataset:{}, append(){}, addEventListener(){}, setAttribute(){}, querySelectorAll:()=>[]};
  }
  const document = {
    createElement:node,
    getElementById(id) { if (!nodes.has(id)) nodes.set(id,node()); return nodes.get(id); }
  };
  const window = {};
  vm.runInNewContext(fs.readFileSync(require.resolve('../public/quote-study.js'),'utf8'), {
    window, document, AbortController, setTimeout, clearTimeout,
    fetch:(url,options)=>new Promise(resolve=>requests.push({url,options,resolve}))
  });
  const first = window.QuoteStudy.enter();
  window.QuoteStudy.leave();
  const second = window.QuoteStudy.enter();
  assert.equal(requests[0].options.signal.aborted,true);
  requests[1].resolve({ok:true,json:async()=>({status:'unavailable'})});
  await second;
  requests[0].resolve({ok:false});
  await first;
  assert.equal(nodes.get('study-reading').textContent,'No quote study is connected to this dashboard.');
  assert.equal(nodes.get('study-retry').hidden,false);
});
