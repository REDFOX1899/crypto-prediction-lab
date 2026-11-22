(function(root){
'use strict';

const FONT_SANS='ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
const FONT_MONO='ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace';

const SERIES_LIGHT={
 purple:'#7F4BF3',
 terracotta:'#CC785C',
 ink:'#1F1F1F',
 green:'#34A853',
 azure:'#0089F4',
 orange:'#FF7018',
 indigo:'#736CD3',
 cobalt:'#2243E6',
 forest:'#195428',
 crimson:'#E12D39',
 teal:'#1B818E',
 slate:'#676767',
};

const SERIES_DARK={
 purple:'#9D74F7',
 terracotta:'#E08E73',
 ink:'#E5E7EB',
 green:'#4ADE80',
 azure:'#38BDF8',
 orange:'#FB923C',
 indigo:'#948EF0',
 cobalt:'#60A5FA',
 forest:'#22C55E',
 crimson:'#F87171',
 teal:'#2DD4BF',
 slate:'#9CA3AF',
};

const FAMILY_ORDER=[
 ['Favorite-Longshot Bias','green'],
 ['Multi-Factor Ensemble','azure'],
 ['Cross-Asset Lead-Lag','cobalt'],
 ['L2 Order-Book Microstructure','purple'],
 ['Adverse-Selection Maker','terracotta'],
 ['Session & News Catalysts','orange'],
 ['Black-Scholes Digital','indigo'],
 ['Impulse Mean-Reversion','teal'],
 ['Momentum & Volume Surge','forest'],
 ['Controls & Latency Stress','ink'],
];

const FAMILY_ORDER_GEN2=[
 ['SOL Lead-Lag & Latency','green'],
 ['Multi-Asset Best-EV Router','azure'],
 ['Multi-Asset 3-Leg Portfolio','cobalt'],
 ['Regime & Session Gated','purple'],
 ['Maker-Taker Smart Hybrid','terracotta'],
 ['Gen-2 Sharp FLB Compression','orange'],
 ['L2 Persistence & Anti-Toxicity','indigo'],
 ['Sub-100ms Latency Shocks','teal'],
 ['ETH Lead-Lag & Latency','forest'],
 ['Risk Control & Falsification','ink'],
];

function hexLum(hex){
 if(typeof hex!=='string'||!/^#[0-9a-fA-F]{6}$/.test(hex))return 0.5;
 const [r,g,b]=hex.slice(1).match(/../g).map(h=>{
  const v=parseInt(h,16)/255;
  return v<=0.03928?v/12.92:Math.pow((v+0.055)/1.055,2.4);
 });
 return 0.2126*r+0.7152*g+0.0722*b;
}

function isDarkHex(hex){
 return hexLum(hex)<0.42;
}

function familyColors(dark){
 const s=dark?SERIES_DARK:SERIES_LIGHT;
 const out={};
 for(const [name,key] of FAMILY_ORDER)out[name]=s[key];
 for(const [name,key] of FAMILY_ORDER_GEN2)out[name]=s[key];
 return out;
}

function familyColor(family,dark){
 const map=familyColors(Boolean(dark));
 return map[family]||(dark?SERIES_DARK.azure:SERIES_LIGHT.azure);
}

function palette(){
 const dark=typeof document!=='undefined'&&document.documentElement?.dataset?.theme==='dark';
 const s=dark?SERIES_DARK:SERIES_LIGHT;
 return {
  dark,
  fontSans:FONT_SANS,
  fontMono:FONT_MONO,
  bg:dark?'#0B0B0D':'#FAFAFA',
  surface:dark?'#131317':'#FFFFFF',
  surface2:dark?'#1B1B21':'#F4F4F5',
  text:dark?'#F5F5F7':'#141414',
  muted:dark?'#A0A0AB':'#5E5E66',
  axisText:dark?'#A0A0AB':'#52525B',
  legendText:dark?'#E4E4E7':'#1F1F1F',
  grid:dark?'rgba(255,255,255,0.14)':'#CCCCCC',
  border:dark?'#2A2A33':'#D9D9D9',
  tooltipBg:dark?'#18181E':'#FFFFFF',
  tooltipBorder:dark?'#32323D':'#D9D9D9',
  goodZoneBg:dark?'rgba(74, 222, 128, 0.10)':'rgba(52, 168, 83, 0.11)',
  goodZoneBorder:dark?'rgba(74, 222, 128, 0.36)':'rgba(52, 168, 83, 0.38)',
  goodZoneText:dark?'#4ADE80':'#1B7A3D',
  watermark:dark?'rgba(160,160,171,0.52)':'rgba(94,94,102,0.62)',
  neutralBar:dark?'#2B2B35':'#E4E6EB',
  series:s,
  // Semantic aliases
  accent:s.azure,
  brand:s.purple,
  up:s.green,
  down:s.crimson,
  warn:s.orange,
  terracotta:s.terracotta,
  ink:s.ink,
  indigo:s.indigo,
  cobalt:s.cobalt,
  teal:s.teal,
  slate:s.slate,
  families:familyColors(dark),
 };
}

function computeParetoFrontier(agents){
 const active=(agents||[])
  .filter(a=>a&&Number.isFinite(Number(a.max_drawdown_pct))&&Number.isFinite(Number(a.end_usd))&&(a.trades??1)>0)
  .sort((a,b)=>{
   const d=Number(a.max_drawdown_pct)-Number(b.max_drawdown_pct);
   if(d!==0)return d;
   return Number(b.end_usd)-Number(a.end_usd);
  });
 const frontier=[];
 let bestY=-Infinity;
 for(const a of active){
  const y=Number(a.end_usd);
  if(y>bestY+1e-9){
   frontier.push(a);
   bestY=y;
  }
 }
 return frontier;
}

function tooltipConfig(tp,extra){
 const base={
  backgroundColor:tp.tooltipBg,
  titleColor:tp.text,
  bodyColor:tp.text,
  borderColor:tp.tooltipBorder,
  borderWidth:1,
  padding:{top:9,right:12,bottom:9,left:12},
  cornerRadius:8,
  displayColors:true,
  usePointStyle:true,
  boxWidth:8,
  boxHeight:8,
  titleFont:{family:tp.fontSans,size:12,weight:'600'},
  bodyFont:{family:tp.fontSans,size:11.5,weight:'400'},
  titleMarginBottom:6,
  bodySpacing:4,
 };
 return extra?Object.assign({},base,extra,{callbacks:Object.assign({},base.callbacks,extra.callbacks)}):base;
}

function legendConfig(tp,extra){
 const base={
  position:'top',
  align:'start',
  labels:{
   color:tp.legendText,
   usePointStyle:true,
   pointStyle:'circle',
   boxWidth:7,
   boxHeight:7,
   padding:14,
   font:{family:tp.fontSans,size:11.5,weight:'500'},
  },
 };
 if(!extra)return base;
 return Object.assign({},base,extra,{labels:Object.assign({},base.labels,extra.labels)});
}

function dottedYScale(tp,extra){
 const base={
  border:{display:false,dash:[2,4]},
  grid:{color:tp.grid,lineWidth:1,tickLength:0},
  ticks:{color:tp.axisText,padding:8,font:{family:tp.fontSans,size:11}},
 };
 if(!extra)return base;
 return Object.assign({},base,extra,{
  border:Object.assign({},base.border,extra.border),
  grid:Object.assign({},base.grid,extra.grid),
  ticks:Object.assign({},base.ticks,extra.ticks),
 });
}

function cleanXScale(tp,extra){
 const base={
  border:{display:false},
  grid:{display:false},
  ticks:{color:tp.axisText,padding:6,font:{family:tp.fontSans,size:11}},
 };
 if(!extra)return base;
 return Object.assign({},base,extra,{
  border:Object.assign({},base.border,extra.border),
  grid:Object.assign({},base.grid,extra.grid),
  ticks:Object.assign({},base.ticks,extra.ticks),
 });
}

function rawEditorialConfig(chart){
 return chart?.config?._config?.options?.plugins?.aaEditorial
  || chart?.config?.options?.plugins?.aaEditorial
  || null;
}

function formatBarVal(fmt,val,idx,di){
 if(typeof fmt==='function')return fmt(val,idx,di);
 if(fmt==='usd1')return `$${val.toFixed(1)}`;
 if(fmt==='brier4')return val.toFixed(4);
 if(fmt==='ms'){
  if(val<1&&val>0)return `${val.toFixed(2)}ms`;
  if(val<100)return `${val.toFixed(1)}ms`;
  if(val>=1000)return `${(val/1000).toFixed(2)}s`;
  return `${Math.round(val)}ms`;
 }
 return String(val);
}

const aaEditorialPlugin={
 id:'aaEditorial',
 beforeDatasetsDraw(chart){
  const cfg=rawEditorialConfig(chart);
  if(!cfg)return;
  const {ctx,chartArea,scales}=chart;
  if(!chartArea)return;
  const tp=palette();

  // 1. Optimal zone / "Most attractive quadrant" background shading
  if(cfg.zone){
   try{
    const z=cfg.zone;
    ctx.save();
    ctx.fillStyle=z.color||tp.goodZoneBg;
    let rx=chartArea.left,ry=chartArea.top,rw=0,rh=0;
    if(z.type==='xBand'&&scales.x){
     const x1=scales.x.getPixelForValue(z.fromIndex);
     const x2=scales.x.getPixelForValue(z.toIndex);
     const step=scales.x.ticks.length>1?Math.abs(scales.x.getPixelForValue(1)-scales.x.getPixelForValue(0))*0.48:18;
     rx=Math.max(chartArea.left,Math.min(x1,x2)-step);
     const right=Math.min(chartArea.right,Math.max(x1,x2)+step);
     rw=Math.max(0,right-rx);
     ry=chartArea.top;
     rh=chartArea.bottom-chartArea.top;
    } else if(z.type==='yBand'){
     const yScale=scales[z.yAxisID||'y'];
     if(yScale){
      const yTop=Math.max(chartArea.top,yScale.getPixelForValue(z.yMax));
      const yBot=Math.min(chartArea.bottom,yScale.getPixelForValue(z.yMin));
      rx=chartArea.left;rw=chartArea.right-chartArea.left;
      ry=Math.min(yTop,yBot);rh=Math.abs(yBot-yTop);
     }
    } else if(z.type==='quadrant'&&scales.x&&(scales[z.yAxisID||'y'])){
     const yScale=scales[z.yAxisID||'y'];
     const xEnd=Math.min(chartArea.right,Math.max(chartArea.left,scales.x.getPixelForValue(z.xMax)));
     const yEnd=Math.min(chartArea.bottom,Math.max(chartArea.top,yScale.getPixelForValue(z.yMin)));
     rx=chartArea.left;ry=chartArea.top;
     rw=Math.max(0,xEnd-chartArea.left);
     rh=Math.max(0,yEnd-chartArea.top);
    }
    if(rw>2&&rh>2){
     ctx.fillRect(rx,ry,rw,rh);
     if(z.borderColor!==false){
      ctx.strokeStyle=z.borderColor||tp.goodZoneBorder;
      ctx.lineWidth=1;
      ctx.setLineDash([3,3]);
      ctx.strokeRect(rx+0.5,ry+0.5,rw-1,rh-1);
      ctx.setLineDash([]);
     }
     if(z.label&&rw>70){
      ctx.font=`600 10.5px ${tp.fontSans}`;
      ctx.fillStyle=tp.goodZoneText;
      ctx.textAlign='left';
      ctx.textBaseline='top';
      ctx.fillText(z.label,rx+8,ry+7);
     }
    }
    ctx.restore();
   }catch{}
  }
 },

 afterDatasetsDraw(chart){
  const cfg=rawEditorialConfig(chart);
  if(!cfg)return;
  const {ctx,chartArea,scales}=chart;
  if(!chartArea)return;
  const tp=palette();

  // 2. Dashed horizontal reference line + inline tag
  if(cfg.refLine){
   try{
    const rl=cfg.refLine;
    const yScale=scales[rl.yAxisID||'y'];
    if(yScale){
     const y=yScale.getPixelForValue(rl.y);
     if(y>=chartArea.top-2&&y<=chartArea.bottom+2){
      ctx.save();
      ctx.strokeStyle=rl.color||tp.muted;
      ctx.lineWidth=1.25;
      ctx.setLineDash([4,4]);
      ctx.beginPath();
      ctx.moveTo(chartArea.left,y);
      ctx.lineTo(chartArea.right,y);
      ctx.stroke();
      ctx.setLineDash([]);
      if(rl.label){
       ctx.font=`600 10px ${tp.fontSans}`;
       const tw=ctx.measureText(rl.label).width;
       const bx=chartArea.right-tw-12;
       const by=rl.below?y+4:y-17;
       ctx.fillStyle=tp.surface;
       ctx.strokeStyle=tp.border;
       ctx.lineWidth=1;
       ctx.beginPath();
       if(typeof ctx.roundRect==='function')ctx.roundRect(bx,by,tw+10,15,4);
       else ctx.rect(bx,by,tw+10,15);
       ctx.fill();
       ctx.stroke();
       ctx.fillStyle=tp.text;
       ctx.textAlign='left';
       ctx.textBaseline='middle';
       ctx.fillText(rl.label,bx+5,by+7.5);
      }
      ctx.restore();
     }
    }
   }catch{}
  }

  // 3. Direct data value labels on bars (Artificial Analysis signature)
  if(cfg.barLabels&&cfg.barLabels.enabled!==false){
   try{
    const bl=cfg.barLabels;
    const indices=bl.datasetIndices||[0];
    const horizontal=Boolean(bl.horizontal);
    const minW=bl.minBarWidth??16;
    ctx.save();
    ctx.font=`600 ${bl.fontSize||10.5}px ${tp.fontSans}`;
    for(const di of indices){
     const ds=chart.data.datasets[di];
     const meta=chart.getDatasetMeta(di);
     if(!ds||!meta||meta.hidden)continue;
     meta.data.forEach((bar,idx)=>{
      const raw=ds.data[idx];
      if(raw==null||!Number.isFinite(Number(raw)))return;
      const text=formatBarVal(bl.format,Number(raw),idx,di);
      if(!text)return;
      if(horizontal){
       const x=bar.x+6;
       const y=bar.y;
       ctx.fillStyle=tp.text;
       ctx.textAlign='left';
       ctx.textBaseline='middle';
       ctx.fillText(text,Math.min(x,chartArea.right-4),y);
      } else {
       const w=bar.width||0;
       if(w<minW)return;
       const top=Math.max(chartArea.top,Math.min(bar.y,bar.base));
       const bot=Math.min(chartArea.bottom,Math.max(bar.y,bar.base));
       const h=bot-top;
       const bg=Array.isArray(ds.backgroundColor)?ds.backgroundColor[idx]:ds.backgroundColor;
       const placeInside=Boolean(bl.inside)&&h>=26&&Number(raw)>=0;
       ctx.textAlign='center';
       if(placeInside){
        ctx.fillStyle=isDarkHex(bg)?'#FFFFFF':'#141414';
        ctx.textBaseline='middle';
        ctx.fillText(text,bar.x,top+h*0.42);
       } else {
        const isNeg=Number(raw)<0;
        ctx.fillStyle=tp.text;
        ctx.textBaseline=isNeg?'top':'bottom';
        const ly=isNeg?Math.min(chartArea.bottom-2,bot+4):Math.max(chartArea.top+11,top-4);
        ctx.fillText(text,bar.x,ly);
       }
      }
     });
    }
    ctx.restore();
   }catch{}
  }

  // 4. Non-overlapping callout labels for Scatter / Pareto Frontier points
  if(cfg.pointLabels&&cfg.pointLabels.items?.length){
   try{
    const pl=cfg.pointLabels;
    const meta=chart.getDatasetMeta(pl.datasetIndex??0);
    if(meta&&!meta.hidden){
     ctx.save();
     ctx.font=`500 10.5px ${tp.fontSans}`;
     const placed=[];
     for(const item of pl.items){
      const pt=meta.data[item.index];
      if(!pt)continue;
      const label=item.text;
      const tw=ctx.measureText(label).width;
      const th=13;
      const candidates=[
       {x:pt.x+9,y:pt.y-6,align:'left'},
       {x:pt.x+9,y:pt.y+11,align:'left'},
       {x:pt.x-9,y:pt.y-6,align:'right'},
       {x:pt.x-9,y:pt.y+11,align:'right'},
       {x:pt.x,y:pt.y-14,align:'center'},
      ];
      let chosen=null;
      for(const c of candidates){
       const left=c.align==='left'?c.x:c.align==='right'?c.x-tw:c.x-tw/2;
       const right=left+tw;
       const top=c.y-th+2;
       const bot=c.y+2;
       if(left<chartArea.left+2||right>chartArea.right-2||top<chartArea.top+2||bot>chartArea.bottom-2)continue;
       const hit=placed.some(b=>!(right+3<b.left||left-3>b.right||bot+2<b.top||top-2>b.bot));
       if(!hit){chosen={...c,left,right,top,bot};break}
      }
      if(!chosen)continue;
      placed.push(chosen);
      ctx.fillStyle=tp.dark?'rgba(19,19,23,0.84)':'rgba(255,255,255,0.88)';
      ctx.fillRect(chosen.left-2,chosen.top-1,tw+4,th+2);
      ctx.fillStyle=tp.text;
      ctx.textAlign=chosen.align;
      ctx.textBaseline='alphabetic';
      ctx.fillText(label,chosen.x,chosen.y);
     }
     ctx.restore();
    }
   }catch{}
  }

  // 5. Subtle Artificial Analysis-style top-right watermark inside plot corner (never collides with top legend)
  if(cfg.watermark!==false){
   try{
    const text=typeof cfg.watermark==='string'?cfg.watermark:'◈ Yes No Tape';
    ctx.save();
    ctx.font=`600 10px ${tp.fontSans}`;
    ctx.fillStyle=tp.watermark;
    ctx.textAlign='right';
    ctx.textBaseline='top';
    ctx.fillText(text,chartArea.right-6,chartArea.top+6);
    ctx.restore();
   }catch{}
  }
 },
};

const api={
 FONT_SANS,
 FONT_MONO,
 SERIES_LIGHT,
 SERIES_DARK,
 FAMILY_ORDER,
 FAMILY_ORDER_GEN2,
 hexLum,
 isDarkHex,
 familyColors,
 familyColor,
 palette,
 computeParetoFrontier,
 tooltipConfig,
 legendConfig,
 dottedYScale,
 cleanXScale,
 plugin:aaEditorialPlugin,
};

if(typeof module==='object'&&module.exports){
 module.exports=api;
 return;
}
root.AAChartTheme=api;
})(typeof window==='undefined'?globalThis:window);
