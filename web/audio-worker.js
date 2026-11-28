const version=encodeURIComponent(new URL(import.meta.url).searchParams.get('v')||'dev');
const {default:init,BrowserEngine,JavaScriptEngine}=await import(`./audio/rustel_browser_audio.js?v=${version}`);
const {SampleLibrary}=await import(`./sample-library.js?v=${version}`);
await init({module_or_path:new URL(`./audio/rustel_browser_audio_bg.wasm?v=${version}`,import.meta.url)});
let engine=null,generation=0,pipe=null,loadSerial=0,assets=null,decodeSerial=0;
const decodes=new Map();
const decode=(bytes,sampleRate)=>new Promise((resolve,reject)=>{const request=++decodeSerial;const timer=setTimeout(()=>{decodes.delete(request);reject(Error("Sample decode timed out"));},35000);decodes.set(request,{resolve,reject,timer});self.postMessage({kind:'decode',request,bytes,sampleRate},[bytes]);});
const library=new SampleLibrary(decode,message=>self.postMessage({kind:'progress',message}));await library.init();
const jobs=new WeakMap();
function retire(candidate){if(!candidate)return;Promise.allSettled([...(jobs.get(candidate)?.values()||[])]).then(()=>candidate.free());}
async function ensure(candidate,store,controls,sampleRate,seen=new Set()){
  for(const value of controls){const key=JSON.stringify([value.s||value.sound,value.bank,value.n]);if(seen.has(key))continue;seen.add(key);await store.ensure(candidate,value,sampleRate);}
}
function logs(candidate,generation){for(const message of JSON.parse(candidate.logs_json?.()||'[]').slice(-100))self.postMessage({kind:'log',generation,message});}
function render(data){
  if(data.generation!==generation||!engine)return;
  try{
    const start=engine.frame(),pcm=engine.render(4096),events=JSON.parse(engine.events_json());logs(engine,generation);
    pipe.postMessage({kind:'chunk',generation,start,pcm},[pcm.buffer]);
    self.postMessage({kind:'events',generation,events,visuals:engine.visual_audio_json?JSON.parse(engine.visual_audio_json()):[]});
    if(assets){
      const candidate=engine,store=assets;let inflight=jobs.get(candidate);if(!inflight){inflight=new Map();jobs.set(candidate,inflight);}
      for(const control of JSON.parse(engine.missing_json())){
        const key=JSON.stringify([control.s||control.sound,control.bank,control.n]);if(inflight.has(key))continue;const job=store.ensure(candidate,control,store.sampleRate).catch(error=>{if(engine===candidate)self.postMessage({kind:'asset-error',generation:data.generation,message:String(error)});}).finally(()=>inflight.delete(key));inflight.set(key,job);
      }
    }
  }catch(error){self.postMessage({kind:'error',generation,message:String(error)});}
}
self.onmessage=async({data})=>{
  try{
    if(data.kind==='decoded'){const pending=decodes.get(data.request);if(pending){decodes.delete(data.request);clearTimeout(pending.timer);if(data.error)pending.reject(Error(data.error));else pending.resolve({pcm:data.pcm,sampleRate:data.sampleRate,channels:data.channels});}return;}
    if(data.kind==='studio-control'&&data.generation===generation&&engine){if(data.control.type==='slider')engine.set_slider(data.control.id,data.control.value);else if(data.control.type==='orbit-gain')engine.set_orbit_gain(data.control.orbit,data.control.gain);return;}
    if(data.kind==='channel'){pipe?.close();pipe=data.port;pipe.onmessage=({data})=>render(data);pipe.start();}
    if(data.kind==='cancel-load'){loadSerial++;return;}
    if(data.kind==='load'){
      const serial=++loadSerial;let next=null;
      try{
        let metadata=null,store=null;
        if(data.score.format==='javascript'){
          next=new JavaScriptEngine(data.score.source,data.sampleRate,JSON.stringify(data.score.overrides||{}));next.set_visual_capture?.(data.score.studio===true);metadata=JSON.parse(next.metadata_json());
          store=await library.forScore(metadata.samples,data.score.url);store.sampleRate=data.sampleRate;
          const seen=new Set();
          for(let begin=0;begin<64;begin+=4){if(serial!==loadSerial)throw Error('Score load cancelled');await ensure(next,store,JSON.parse(next.probe_json(begin,begin+4)),data.sampleRate,seen);}
          for(const sound of (metadata.preload||[]).flatMap(names=>names.split(/\s+/).filter(Boolean))){const [s,n]=sound.split(':');await store.ensure(next,{s,n:Number(n)||0},data.sampleRate);}
          metadata.sample_banks=[...(store.registrations?.get(next)?.keys()||[])];
        }else next=new BrowserEngine(JSON.stringify(data.score),data.sampleRate);
        if(serial!==loadSerial)throw Error('Score load cancelled');
        retire(engine);engine=next;next=null;assets=store;generation=data.generation;self.postMessage({kind:'loaded',generation,metadata});logs(engine,generation);
      }finally{next?.free();}
    }
  }catch(error){self.postMessage({kind:'error',generation:data.generation,message:String(error)});}
};
self.postMessage({kind:'ready'});
