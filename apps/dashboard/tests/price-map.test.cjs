const test=require('node:test'), assert=require('node:assert/strict');
const {measure}=require('../public/price-map.js');
const {visible}=require('../public/paper-review.js');
const book={yes_dollars:[['0.48','3'],['0.50','40']],no_dollars:[['0.46','10']]};
const read=(p,c=2,b=book)=>measure(b,1000,100000,2000,c,p);
test('YES/NO thresholds, equality, signed depth and complementary margins',()=>{
  assert.deepEqual([read(60).yesAbove,read(60).noBelow,read(60).pressure],[56,48,60]);
  assert.deepEqual([read(60).side,read(42).side,read(56).side,read(48).side,read(null).side],['YES','NO','Pass','Pass',null]);
  assert.equal(read(60).yesNet,4);assert.equal(read(42).noNet,6);
  assert.equal(read(100,90).side,'Pass');
});
test('expired, future, one-sided and malformed books leave gaps',()=>{
  for(const args of [[book,0,100000,31000],[book,2000,100000,1000],[book,1000,1500,2000],
    [{...book,yes_dollars:[]},1000,100000,2000],[{...book,no_dollars:[[.6,5]]},1000,100000,2000],
    [{...book,no_dollars:[['',null]]},1000,100000,2000]])assert.ok(measure(...args,2,55).reason);
});
test('decision and later-book views hide settlement, daily performance and future fills',()=>{
  const r={open_ts:0,committed_at:new Date(9000).toISOString(),candidate_at:new Date(50000).toISOString(),
    result_available_at:new Date(980000).toISOString(),official_result:'yes',scorecard:{p_yes:0.62},
    strategies:{s:{side:'YES',cap:.54,status:'filled',execution_at:new Date(50000).toISOString(),settled_at:new Date(980000).toISOString(),filled_qty:1,fills:[],net_pnl:.42}}};
  const asof=new Date(990000).toISOString(),first=visible(r,'s','decision',asof),book=visible(r,'s','book',asof);
  assert.equal(first.intent,'YES');assert.equal(first.execution,undefined);assert.equal(book.execution.qty,1);
  r.official_result='no';r.strategies.s.net_pnl=-.58;
  assert.deepEqual(visible(r,'s','decision',asof),first);assert.deepEqual(visible(r,'s','book',asof),book);
  assert.equal(visible(r,'s','review',asof).net,-.58);
});
test('previous-window scores preserve missing intervals and their denominators',()=>{
 const {transitions}=require('../public/data-lab.js');
 const slots=['yes','yes',null,'no','yes','no'].map((r,i)=>({at:i*900000,contracts:{BTC:r?{result:r}:null}}));
 assert.deepEqual(transitions(slots,'BTC'),{yes:{wins:1,n:2},no:{wins:1,n:1}});
 assert.deepEqual(transitions(slots.filter(r=>r.contracts.BTC),'BTC'),transitions(slots,'BTC'));
});
