export async function prepareIsolation(){
 if(window.crossOriginIsolated)return true;
 if(!window.isSecureContext||!('serviceWorker' in navigator))throw Error('HTTPS and service-worker support are required for browser graphics');
 await navigator.serviceWorker.register(new URL('./coi-serviceworker.js',import.meta.url),{scope:'./'});await navigator.serviceWorker.ready;
 if(!navigator.serviceWorker.controller){await new Promise(resolve=>navigator.serviceWorker.addEventListener('controllerchange',resolve,{once:true}));}
 const key='gpui-isolation-reload:'+location.pathname;
 if(!sessionStorage.getItem(key)){sessionStorage.setItem(key,'1');location.reload();return false;}
 throw Error('Cross-origin isolation is unavailable. Reload this page after allowing its service worker.');
}
