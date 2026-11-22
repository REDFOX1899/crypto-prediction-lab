(function(root){'use strict';
const palettes={light:{bg:'#fafafa',surface:'#ffffff',text:'#141414',muted:'#5e5e66',border:'#d9d9d9',up:'#1b7a3d',down:'#c62828',accent:'#6d28d9',target:'#5e5e66',line:'#0074d9',area:'rgba(0,137,244,.12)'},dark:{bg:'#0b0b0d',surface:'#131317',text:'#f5f5f7',muted:'#a0a0ab',border:'#2a2a33',up:'#4ade80',down:'#fb7185',accent:'#a78bfa',target:'#a0a0ab',line:'#38bdf8',area:'rgba(56,189,248,.13)'}};
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
