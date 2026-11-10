(function(root){
'use strict';
function route(input){
 const url=new URL(input,'https://example.invalid');const view=url.searchParams.get('view');
 const asset=['BTC','ETH','SOL'].includes(url.searchParams.get('asset'))?url.searchParams.get('asset'):'BTC';
 return {mode:view==='data'?'data':view==='combos'?'combos':view==='market'?'market':view==='home'?'home':!view&&url.searchParams.has('asset')?'market':'home',asset};
}
if(typeof module==='object'&&module.exports)module.exports={route};else root.DashboardNavigation={route};
})(typeof window!=='undefined'?window:globalThis);
