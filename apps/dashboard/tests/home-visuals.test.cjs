const {test}=require('node:test'),assert=require('node:assert/strict');
const {reference,sparkline}=require('../public/home-visuals.js');
const {resolve,palettes}=require('../public/theme.js');
const start=Date.parse('2026-11-09T20:00:00Z'),close=start+900000;
const market={ticker:'BTC-TEST',status:'active',open_time:new Date(start).toISOString(),close_time:new Date(close).toISOString(),floor_strike:'100',strike_type:'greater_or_equal'};
function state(now,value='102',source=now){return reference(market,{fetched_at:new Date(now).toISOString()},{state:'live',value,source_time_ms:source},now,now)}
test('reference distance uses the numeric strike and fresh same-window observations',()=>{
 const r=state(start+2000);assert.equal(r.distance,2);assert.equal(r.percent,2);assert.equal(r.fresh,true);
 assert.equal(state(start+2000,'98').distance,-2);
 assert.equal(state(start+12000,'102',start).distance,null);
 assert.equal(state(start+2000,'102',start-1).value,null);
 assert.equal(state(start+2000,'102',start+5001).fresh,false);
 assert.equal(reference({...market,strike_type:'less'}, {fetched_at:new Date(start+2000).toISOString()},{state:'live',value:'102',source_time_ms:start+2000},start+2000,start+2000).distance,null);
});
test('final-minute transition and exclusive close are explicit without predicting outcomes',()=>{
 assert.equal(state(close-60001).phase,'building');assert.equal(state(close-60001).startsIn,1);
 assert.equal(state(close-60000).phase,'final');assert.equal(state(close-60000).seconds,60);
 assert.equal(state(close).phase,'closed');assert.equal(state(close).value,null);
 assert.equal(state(close).distance,null);assert.equal(state(close+1).startsIn,null);
});
test('sparklines preserve gaps, remove duplicates and never create observations',()=>{
 const now=start+120000,samples=[{t:now-10000,v:100},{t:now-9000,v:101},{t:now,v:103},{t:now,v:104},{t:now+1,v:999},{t:now-130000,v:1}];
 const s=sparkline(samples,102,now);assert.equal(s.count,3);assert.equal((s.path.match(/M/g)||[]).length,2);assert.ok(s.targetY>=14&&s.targetY<=82);assert.ok(s.last.x<=310);assert.equal(sparkline([],100,now).path,'');
 assert.ok(!sparkline([{t:now,v:100}],100,now).path.includes('NaN'));
});
function luminance(hex){const rgb=hex.slice(1).match(/../g).map(v=>parseInt(v,16)/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4);return rgb[0]*.2126+rgb[1]*.7152+rgb[2]*.0722}
function contrast(a,b){const x=luminance(a),y=luminance(b);return (Math.max(x,y)+.05)/(Math.min(x,y)+.05)}
test('both palettes keep normal data text and semantic colors above 4.5:1 on surfaces',()=>{
 for(const p of Object.values(palettes))for(const bg of [p.bg,p.surface])for(const fg of [p.text,p.muted,p.up,p.down,p.accent])assert.ok(contrast(fg,bg)>=4.5,`${fg} on ${bg}: ${contrast(fg,bg)}`);
 assert.equal(resolve('dark',false),'dark');assert.equal(resolve('invalid',true),'dark');assert.equal(resolve(null,false),'light');
});
test('theme remains usable when storage is blocked and manual selection survives system changes',()=>{
 const vm=require('node:vm'),fs=require('node:fs'),events={},attrs={},modeLabel={textContent:''},media={matches:true,addEventListener(name,fn){events.system=fn}};
 const button={setAttribute(k,v){attrs[k]=v},addEventListener(name,fn){events.click=fn}},document={documentElement:{dataset:{}},readyState:'complete',getElementById:id=>id==='theme-toggle'?button:modeLabel};
 const window={localStorage:{getItem(){throw Error('blocked')},setItem(){throw Error('blocked')}},matchMedia:()=>media,dispatchEvent(){}};
 vm.runInNewContext(fs.readFileSync(require.resolve('../public/theme.js'),'utf8'),{window,document,CustomEvent:class{},Error});
 assert.equal(document.documentElement.dataset.theme,'dark');assert.equal(attrs['aria-pressed'],'true');
 events.click();assert.equal(document.documentElement.dataset.theme,'light');assert.equal(attrs['aria-label'],'Switch to dark mode');events.system();assert.equal(document.documentElement.dataset.theme,'light');
});
