import assert from 'node:assert/strict';
import {startup,validateAdapter} from './startup.js';
const adapter={features:new Set(['indirect-first-instance']),limits:{maxBindGroups:4}};
assert.throws(()=>validateAdapter(null),/no GPU adapter/);
assert.throws(()=>validateAdapter({...adapter,features:new Set()}),/indirect-first-instance/);
validateAdapter(adapter);
class Element extends EventTarget {constructor(){super();this.children=[];this.style={cssText:''};this.hidden=false;}append(...items){this.children.push(...items);}replaceChildren(...items){this.children=items;}}
const data=new Map();globalThis.localStorage={getItem:key=>data.get(key),setItem:(key,value)=>data.set(key,value)};
globalThis.document={createElement:()=>new Element()};globalThis.devicePixelRatio=3;globalThis.setTimeout=()=>0;
Object.defineProperty(globalThis,'navigator',{value:{userAgent:'Android Chrome',gpu:{requestAdapter:async()=>adapter},clipboard:{writeText:async()=>{}}},configurable:true});
globalThis.location={search:'',reload(){throw Error('unexpected reload');}};globalThis.window=new EventTarget();
const boot=new Element(),diag=startup(boot,'test','one');await diag.check();diag.appReady();assert.equal(boot.hidden,false,'run() returning must not hide the startup screen');
const ready=new Event('nightshade-stage');ready.detail={stage:'ready',message:'frame received'};window.dispatchEvent(ready);assert.equal(boot.hidden,true);
const failed=new Event('nightshade-stage');failed.detail={stage:'error',message:'GPU device lost'};window.dispatchEvent(failed);assert.equal(boot.hidden,false);assert.match(boot.children[0].textContent,/GPU device lost/);
assert.match(boot.children[2].children[1].textContent,/Android Chrome/);
// A killed startup stays at a lightweight diagnostic screen on reload.
data.set('retry-startup',JSON.stringify({pending:true,phase:'Compiling WASM'}));globalThis.window=new EventTarget();
let adapterRequests=0;navigator.gpu.requestAdapter=async()=>{adapterRequests++;return adapter;};
const retryBoot=new Element(),retry=startup(retryBoot,'retry','one');let started=false;const pending=retry.check().then(()=>started=true);await new Promise(resolve=>setImmediate(resolve));assert.equal(started,false);assert.equal(adapterRequests,0,'recovery must not touch the GPU before Start app');
assert.match(retryBoot.children[0].textContent,/Previous startup stopped at: Compiling WASM/);
retryBoot.children[1].children[0].onclick();await pending;assert.equal(started,true);assert.equal(adapterRequests,1);
console.log('PASS: adapter checks, first-frame startup gating, visible GPU failures, crash-recovery diagnostics');
