// Control-plane tests; actual QuickJS/DSP and worklet PCM are tested separately.
import assert from 'node:assert/strict';
import {readFileSync,readdirSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {LocalAudioHost} from './audio-host.js';
import {DEFAULT_DEMO} from './demo-catalog.js';
const files=new Map(readdirSync(new URL('../demos/',import.meta.url)).filter(n=>n.endsWith('.strudel')).map(name=>[name,readFileSync(new URL('../demos/'+name,import.meta.url))]));
const catalog=[...files].map(([name,bytes])=>({name,url:'./scores/'+name,sha256:createHash('sha256').update(bytes).digest('hex')}));
globalThis.window={addEventListener(){},removeEventListener(){}};
globalThis.location={href:'https://example.test/gpui-rustel/',search:''};
globalThis.fetch=async input=>{
 const url=String(input);if(url.endsWith('audio-demos.json'))return {ok:true,json:async()=>JSON.parse(readFileSync(new URL('./audio-demos.json',import.meta.url)))};
 if(url.endsWith('javascript-demos.json'))return {ok:true,json:async()=>catalog};
 const bytes=files.get(decodeURIComponent(url.split('/').at(-1)));assert(bytes,url);
 return {ok:true,arrayBuffer:async()=>bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength)};
};
let holdInitialization=false,failFirstLoad=false;
globalThis.Worker=class {
 constructor(){queueMicrotask(()=>this.onmessage({data:{kind:'ready'}}));}
 postMessage(data){if(data.kind==='load')this.loads=(this.loads||0)+1;if(data.kind==='load'&&failFirstLoad){failFirstLoad=false;queueMicrotask(()=>this.onmessage({data:{kind:'error',generation:data.generation,message:'score evaluation deadline'}}));return;}if(data.kind==='load'&&holdInitialization&&this.loads===2)return;if(data.kind==='load')queueMicrotask(()=>this.onmessage({data:{kind:'loaded',generation:data.generation,metadata:{cps:0.5,lanes:[]}}}));}
 terminate(){}
};
let permitted=false,nodes=0;
globalThis.AudioContext=class {
 constructor(){this.sampleRate=holdInitialization?44100:48000;this.waiters=[];this.audioWorklet={addModule:async()=>{}};}
 resume(){if(permitted){for(const resolve of this.waiters.splice(0))resolve();return Promise.resolve();}return new Promise(resolve=>this.waiters.push(resolve));}
 close(){return Promise.resolve();}
};
globalThis.AudioWorkletNode=class {
 constructor(){nodes++;let generation=0,closed=false;this.port={close(){closed=true;},postMessage:data=>{
  if(data.kind==='reset')generation=data.generation;
  if(data.kind==='play'&&data.request)setTimeout(()=>{if(!closed)this.port.onmessage({data:{kind:'transport-ack',request:data.request,generation,frame:0}});},0);
 }};}
 connect(){}disconnect(){}
};
globalThis.MessageChannel=class {constructor(){this.port1={};this.port2={};}};
globalThis.AnalyserNode=class {connect(){}disconnect(){}};
globalThis.GainNode=class {connect(){}disconnect(){}};
const settle=()=>new Promise(resolve=>setTimeout(resolve,10));
const make=()=>new LocalAudioHost({local_message(){}});
const command=(host,action)=>host.command({session:host.session,generation:host.generation,request:crypto.randomUUID(),transport:{action}});
// Browsers that block autoplay must still finish startup and accept an explicit Play.
let host=make();await host.start();await settle();assert.equal(host.demo,DEFAULT_DEMO);assert.equal(host.score.source,files.get(DEFAULT_DEMO).toString());assert.equal(host.playing,false);assert.equal(nodes,0);
permitted=true;await command(host,'play');assert.equal(host.playing,true);assert.equal(nodes,1,'autoplay and Play must share one audio graph');assert.equal(host.worker.loads,1,'matching sample rates must reuse the prepared banks instead of loading a second engine');
await command(host,'pause');assert.equal(host.playing,false);await command(host,'play');assert.equal(host.playing,true);assert.equal(nodes,1);host.close();
// Existing browser audio permission starts the default arrangement without a click.
host=make();await host.start();await settle();assert.equal(host.playing,true);assert.equal(nodes,2);host.close();
// Pause cancels a pending autoplay attempt even if audio permission arrives later.
permitted=false;host=make();await host.start();await command(host,'pause');permitted=true;await host.context.resume();await settle();assert.equal(host.playing,false);host.close();
// Stop during audio initialization must cancel cleanly and permit a later Play.
holdInitialization=true;permitted=true;host=make();await host.start();await settle();assert(host.loading&&host.initializingAudio);await command(host,'stop');assert.equal(host.playing,false);assert.equal(host.frame,0);await command(host,'play');assert.equal(host.playing,true);host.close();
// An initial score refusal leaves the demo catalog available and Play retries.
holdInitialization=false;failFirstLoad=true;permitted=true;const messages=[];
host=new LocalAudioHost({local_message(topic,value){messages.push([topic,value]);}});
await assert.rejects(host.start(),/score evaluation deadline/);host.failure('score evaluation deadline');host.publish();
const snapshot=JSON.parse(messages.findLast(([topic])=>topic==='music/snapshot')[1]);
assert(snapshot.demos.includes(DEFAULT_DEMO));assert.equal(snapshot.transport.paused,true);
await command(host,'play');assert.equal(host.playing,true);assert.equal(host.demo,DEFAULT_DEMO);assert.equal(host.worker.loads,2);host.close();
console.log('PASS: default Bachata, allowed autoplay, blocked autoplay + manual Play, single audio graph, pause cancellation, stop during initialization, restart and recovery from initial score refusal');
