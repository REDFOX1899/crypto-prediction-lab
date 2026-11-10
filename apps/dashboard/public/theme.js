(function(root){'use strict';
const palettes={light:{bg:'#f5f6f8',surface:'#ffffff',text:'#15212f',muted:'#59687a',border:'#d9e0e8',up:'#08744c',down:'#b0394b',accent:'#245bce',target:'#59687a',line:'#245bce',area:'rgba(36,91,206,.14)'},dark:{bg:'#080b10',surface:'#10161e',text:'#edf4fa',muted:'#a0adbe',border:'#2b3645',up:'#75e1b3',down:'#ff9aa7',accent:'#a5bdff',target:'#a0adbe',line:'#99b6ff',area:'rgba(153,182,255,.14)'}};
function resolve(saved,system){return saved==='light'||saved==='dark'?saved:system?'dark':'light'}
if(typeof module!=='undefined'){module.exports={resolve,palettes};return}
let saved=null;try{saved=root.localStorage.getItem('cpl-theme')}catch{}
const media=root.matchMedia?.('(prefers-color-scheme: dark)');let preference=saved==='light'||saved==='dark'?saved:null;
function apply(theme,persist=false){const value=resolve(theme,media?.matches);document.documentElement.dataset.theme=value;
 if(persist){preference=value;try{root.localStorage.setItem('cpl-theme',value)}catch{}}
 const button=document.getElementById('theme-toggle');if(button){button.setAttribute('aria-pressed',String(value==='dark'));button.setAttribute('aria-label',value==='dark'?'Switch to light mode':'Switch to dark mode');document.getElementById('theme-label').textContent=value==='dark'?'Light':'Dark'}
 root.dispatchEvent(new CustomEvent('dashboard-theme',{detail:value}));
}
root.DashboardTheme={palette:()=>palettes[document.documentElement.dataset.theme]||palettes.light,apply};
apply(resolve(saved,media?.matches));
function init(){apply(document.documentElement.dataset.theme);document.getElementById('theme-toggle')?.addEventListener('click',()=>apply(document.documentElement.dataset.theme==='dark'?'light':'dark',true))}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',init,{once:true});else init();
media?.addEventListener?.('change',()=>{if(!preference)apply(resolve(null,media.matches))});
})(typeof window!=='undefined'?window:globalThis);
