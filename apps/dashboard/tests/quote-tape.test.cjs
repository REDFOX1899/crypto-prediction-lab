const test=require('node:test'), assert=require('node:assert/strict');
const {record,segments}=require('../public/quote-tape.js');
const start=Date.parse('2026-11-11T01:00:00Z');
const m={ticker:'A',open_time:new Date(start).toISOString(),close_time:new Date(start+900000).toISOString()};
const book=(ms)=>({sampleAt:new Date(start+ms).toISOString(),mode:'live',spread:.02,yes:.55});
test('repainting does not invent samples and one second retains only the last receipt',()=>{
  let tape=record(null,m,book(50),start+100);
  tape=record(tape,m,book(50),start+200);
  tape=record(tape,m,book(250),start+300);
  assert.equal(tape.points.length,1);assert.equal(tape.points[0].t,start+250);
  assert.ok(Math.abs(tape.points[0].mid-54)<1e-8);
});
test('missing books and collection gaps split traces; stale, future and crossed quotes stay out',()=>{
  let tape=record(null,m,book(500),start+600);
  tape=record(tape,m,{...book(2400),spread:null},start+2500);
  tape=record(tape,m,book(3500),start+3600);
  tape=record(tape,m,book(20000),start+20100);
  assert.equal(segments(tape.points).length,3);
  for(const [quote,now] of [[book(20001),40000],[book(50000),40500],[{...book(40500),spread:-.01},41000]])
    tape=record(tape,m,quote,start+now);
  assert.equal(tape.points.length,3);
  assert.equal(record(tape,{...m,ticker:'B'},book(41000),start+42000).points.length,1);
  assert.equal(record(tape,m,book(800000),start+900000).points.length,0);
});
