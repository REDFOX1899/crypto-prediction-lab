const test=require('node:test'),assert=require('node:assert/strict');
const {slots,statistics}=require('../public/outcome-board.js');
test('fixed window marks absent settlements instead of hiding gaps',()=>{
 const now=1800000,contracts=[{close_time:new Date(900000).toISOString(),result:'yes'},{close_time:new Date(1800000).toISOString(),result:'no'}];
 const cells=slots(contracts,4,now);assert.equal(cells.length,4);assert.equal(cells[0].contract,null);assert.deepEqual(statistics(cells),{up:1,down:1,longest:1,missing:2});
});
test('unknown windows break runs and no forecasts are derived',()=>{
 const cells=[{contract:{result:'yes'}},{contract:{result:'yes'}},{contract:null},{contract:{result:'yes'}},{contract:{result:'no'}}];
 assert.deepEqual(statistics(cells),{up:3,down:1,longest:2,missing:1});
});
