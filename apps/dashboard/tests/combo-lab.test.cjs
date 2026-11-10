const test=require('node:test'),assert=require('node:assert/strict');
const {legs,alignedWindow,jointHistory,scenario}=require('../public/combo-lab.js');
const iso=t=>new Date(t).toISOString();
const market=(start=0,end=900000)=>({status:'active',open_time:iso(start),close_time:iso(end)});
const row=(asset,result,start=0,end=900000)=>({asset,result,open_time:iso(start),close_time:iso(end)});
test('two or three distinct current 15-minute legs must align exactly',()=>{
 const markets={BTC:market(),ETH:market(),SOL:market()};
 assert.equal(alignedWindow(markets,{BTC:'yes'},500).ready,false);
 assert.equal(alignedWindow(markets,{BTC:'yes',ETH:'no'},500).ready,true);
 assert.equal(alignedWindow(markets,{BTC:'yes',ETH:'no',SOL:'yes'},500).ready,true);
 markets.SOL=market(900000,1800000);
 assert.equal(alignedWindow(markets,{BTC:'yes',ETH:'no',SOL:'yes'},500).ready,false);
 assert.equal(alignedWindow(markets,{BTC:'yes',ETH:'no'},900000).ready,false);
 markets.ETH=market(0,1800000);
 assert.equal(alignedWindow(markets,{BTC:'yes',ETH:'no'},500).ready,false);
 assert.deepEqual(legs({BTC:'yes',ETH:'invalid',XRP:'yes'}),['BTC']);
});
test('joint results use observed intersections and exclude missing or wrong-asset records',()=>{
 const history={BTC:[row('BTC','yes'),row('BTC','no',900000,1800000)],ETH:[row('ETH','no')]};
 const cells=jointHistory(history,{BTC:'yes',ETH:'no'},1800000,2);
 assert.deepEqual(cells.map(c=>[c.complete,c.matched]),[[true,true],[false,null]]);
 history.ETH.push(row('ETH','yes',900000,1800000));
 assert.equal(jointHistory(history,{BTC:'yes',ETH:'no'},1800000,2)[1].matched,false);
 history.ETH=[row('BTC','no')];assert.equal(jointHistory(history,{BTC:'yes',ETH:'no'},900000,1)[0].complete,false);
});
test('equal close times alone do not join different contract durations',()=>{
 const history={BTC:[row('BTC','yes')],ETH:[row('ETH','yes',-900000,900000)]};
 assert.equal(jointHistory(history,{BTC:'yes',ETH:'yes'},900000,1)[0].complete,false);
 assert.deepEqual(jointHistory(history,{BTC:'yes'},900000),[]);
});
test('hypothetical payout requires a user price and uses whole contracts before fees',()=>{
 assert.equal(scenario('',10),null);assert.equal(scenario(0,10),null);assert.equal(scenario(100,10),null);
 assert.equal(scenario(25,Infinity),null);assert.equal(scenario(25,-1),null);
 assert.deepEqual(scenario(25,10),{contracts:40,cost:10,payout:40,profit:30,multiple:4});
 const rounded=scenario(30,1);assert.equal(rounded.contracts,3);assert.ok(rounded.cost<=1);assert.equal(rounded.payout,3);
});

test('public payout preview requires exact fresh legs and respects visible ask size',()=>{
 const {quotedPreview}=require('../public/combo-lab.js');const now=Date.parse('2026-11-08T21:05:00Z');
 const markets={BTC:{ticker:'BTC-A',status:'active',open_time:'2026-11-08T21:00:00Z',close_time:'2026-11-08T21:15:00Z'},SOL:{ticker:'SOL-A',status:'active',open_time:'2026-11-08T21:00:00Z',close_time:'2026-11-08T21:15:00Z'}};
 const picks={BTC:'yes',SOL:'no'},book={status:'available',ask:'0.25',ask_size:'6.5',checked_at:'2026-11-08T21:05:00Z',window_close:'2026-11-08T21:15:00Z',legs:[{market_ticker:'BTC-A',side:'yes'},{market_ticker:'SOL-A',side:'no'}]};
 assert.deepEqual(quotedPreview(book,markets,picks,'10',now),{contracts:6,cost:1.5,payout:6,limited:true});
 assert.equal(quotedPreview({...book,status:'empty',ask:null},markets,picks,'10',now),null);
 assert.equal(quotedPreview(book,markets,picks,'10',now+16000),null);
 assert.equal(quotedPreview(book,markets,{BTC:'no',SOL:'no'},'10',now),null);
 assert.equal(quotedPreview({...book,legs:[...book.legs,book.legs[0]]},markets,picks,'10',now),null);
 assert.equal(quotedPreview(book,markets,picks,'',now),null);
});
