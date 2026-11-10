const {test}=require('node:test'),assert=require('node:assert/strict');
const {model}=require('../public/final-minute.js');
const s={asset:'BTC',server_time_ms:100000,close_time_ms:161000,phase:'before_final',source_state:'live',source_time_ms:99500,distance_usd:'2',distance_percent:'0.1',coverage:'-'.repeat(60)};
test('server clock drives the transition and exclusive close without relying on browser wall clock',()=>{
 assert.equal(model(s).phase,'before_final');assert.equal(model(s,1000).phase,'final_minute');
 assert.equal(model(s,61000).phase,'closed');assert.equal(model(s,61000).distance_usd,null);
 assert.equal(model(s,-1000).seconds,61);
});
test('stale or disconnected reference hides current distance while preserving observed coverage',()=>{
 const partial={...s,observed_average:'100',coverage:'101'+'-'.repeat(57)};
 assert.equal(model(partial,10000).distance_usd,null);
 assert.equal(model({...partial,source_state:'reconnecting'}).distance_usd,null);
 assert.equal(model(partial,20000).observed_average,'100');
 assert.equal(model(partial,20000).coverage,partial.coverage);
 assert.equal(model({...partial,phase:'metadata_stale'}).phase,'metadata_stale');
});
test('invalid coverage cannot turn into fabricated observations',()=>{
 assert.equal(model({...s,coverage:'111'}).coverage,'-'.repeat(60));assert.equal(model(null),null);
});
test('asset switches and leaving the workspace reject late feed messages',()=>{
 const vm=require('node:vm'),fs=require('node:fs');
 const nodes=new Map();function node(){return {textContent:'',dataset:{},children:[],setAttribute(){},replaceChildren(...children){this.children=children},get childElementCount(){return this.children.length}}}
 const document={getElementById(id){if(!nodes.has(id))nodes.set(id,node());return nodes.get(id)},createElement:node};
 const window={};vm.runInNewContext(fs.readFileSync(require.resolve('../public/final-minute.js'),'utf8'),{window,document,performance:{now:()=>0},setInterval(){}});
 const view=window.FinalMinuteView;view.select('BTC');view.update({...s,target:'100',latest_value:'102',observed_seconds:0,elapsed_seconds:0});
 assert.equal(nodes.get('final-target').textContent,'$100.00');
 view.select('ETH');view.update({...s,target:'999'});
 assert.equal(nodes.get('final-target').textContent,'—');
 view.update({...s,asset:'ETH',target:'200'});assert.equal(nodes.get('final-target').textContent,'$200.00');
 view.disconnect();assert.equal(nodes.get('final-distance').textContent,'—');
 view.pause();view.update({...s,asset:'ETH',target:'500'});assert.equal(nodes.get('final-target').textContent,'$200.00');
});
