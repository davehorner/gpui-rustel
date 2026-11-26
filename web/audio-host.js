// Browser-owned Rustel: real Rust/WASM scheduling and DSP, never a JS oscillator fallback.
export class LocalAudioHost {
  constructor(viewer){
    this.viewer=viewer;this.session=`browser-${crypto.randomUUID()}`;this.generation=0;this.loadSequence=0;this.progress=null;this.frame=0;this.playing=false;this.events=[];this.sequence=0;this.error=null;this.starting=false;this.sampleRate=48000;this.loading=null;this.volume=0.25;this.transportRequests=new Map();this.transportEpoch=0;
    this.worker=new Worker(new URL('./audio-worker.js',import.meta.url),{type:'module'});
    this.ready=new Promise((resolve,reject)=>{this.workerReady=resolve;this.workerFailure=reject;});
    this.worker.onmessage=({data})=>this.receive(data);
    this.worker.onerror=event=>this.failure(event.message);
    this.onCommand=event=>this.command(JSON.parse(event.detail)).catch(error=>this.response(JSON.parse(event.detail).request,false,String(error)));
    window.addEventListener('rustel-command',this.onCommand);
  }
  async start(){
    const response=await fetch('./audio-demos.json');if(!response.ok)throw Error('Browser demos could not load');
    this.demos=await response.json();
    const manifest=await fetch('./javascript-demos.json').then(r=>r.json());
    for(const entry of manifest){
      const url=new URL(entry.url,location.href).href,response=await fetch(url);if(!response.ok)throw Error(`Could not load ${entry.name}`);
      const bytes=await response.arrayBuffer(),digest=await crypto.subtle.digest('SHA-256',bytes);
      const hash=Array.from(new Uint8Array(digest),v=>v.toString(16).padStart(2,'0')).join('');if(hash!==entry.sha256)throw Error(`Demo source changed: ${entry.name}`);
      this.demos[entry.name]={format:'javascript',source:new TextDecoder().decode(bytes),url,overrides:{}};
    }
    this.demo=new URLSearchParams(location.search).has('mini')?Object.keys(this.demos)[0]:manifest[0].name;
    await this.load(structuredClone(this.demos[this.demo]),this.demo);
    this.timer=setInterval(()=>this.publish(),66);
  }
  message(topic,value){this.viewer.local_message(topic,typeof value==='string'?value:JSON.stringify(value));}
  response(request,accepted,message=''){if(request)this.message('music/response',{request,accepted,message});}
  failure(message){this.error=String(message);this.playing=false;this.node?.port.postMessage({kind:'play',playing:false});this.message('music/status',`Browser audio error: ${message}`);this.cancelLoad(message);this.workerFailure?.(Error(message));}
  async receive(data){
    if(data.kind==='ready')this.workerReady();
    if(data.kind==='progress'){this.progress=data.message;this.message('music/status',data.message);}
    if(data.kind==='asset-error')this.error=data.message;
    if(data.kind==='decode'){
      try{
        if(!this.decoder||this.decoder.sampleRate!==data.sampleRate)this.decoder=new OfflineAudioContext(2,1,data.sampleRate);
        const buffer=await this.decoder.decodeAudioData(data.bytes),channels=Math.min(2,buffer.numberOfChannels);
        if(buffer.length*channels*4>32*1024*1024)throw Error('Decoded sample exceeds 32 MiB');
        const pcm=new Float32Array(buffer.length*channels);for(let c=0;c<channels;c++){const values=buffer.getChannelData(c);for(let i=0;i<buffer.length;i++)pcm[i*channels+c]=values[i];}
        this.worker.postMessage({kind:'decoded',request:data.request,pcm,channels,sampleRate:buffer.sampleRate},[pcm.buffer]);
      }catch(error){this.worker.postMessage({kind:'decoded',request:data.request,error:String(error)});}return;
    }
    if(data.kind==='loaded'&&this.loading?.generation===data.generation){
      const load=this.loading;clearTimeout(load.timer);this.loading=null;this.progress=null;this.generation=data.generation;this.score=load.score;this.demo=load.demo;this.frame=0;this.events=[];this.stereo=null;this.error=null;this.metadata=data.metadata;
      this.node?.port.postMessage({kind:'reset',generation:this.generation});this.node?.port.postMessage({kind:'play',playing:this.playing});
      this.publish();load.resolve();
    }
    if(data.kind==='events'&&data.generation===this.generation){this.events.push(...data.events);if(this.events.length>2048)this.events.splice(0,this.events.length-2048);}
    if(data.kind==='error'&&data.generation!==this.generation&&data.generation!==this.loading?.generation)return;
    if(data.kind==='error'){if(this.loading?.generation===data.generation){const load=this.loading;clearTimeout(load.timer);this.loading=null;this.progress=null;load.reject(Error(data.message));}else this.failure(data.message);}
  }
  async load(score,demo){
    await this.ready;if(this.loading)throw Error('A score is already loading');
    return new Promise((resolve,reject)=>{
      const generation=++this.loadSequence;
      const timer=setTimeout(()=>this.cancelLoad('Score/sample loading timed out after 60 seconds'),60000);
      this.loading={generation,score,demo,resolve,reject,timer};this.progress='Preparing score and samples…';
      this.worker.postMessage({kind:'load',generation,score,sampleRate:this.sampleRate});
    });
  }
  cancelLoad(reason='Score load cancelled'){
    if(!this.loading)return;const load=this.loading;clearTimeout(load.timer);this.loading=null;this.progress=null;
    this.worker.postMessage({kind:'cancel-load'});load.reject(Error(reason));
  }
  async enable(){
    if(this.context){await this.context.resume();return;}
    // Create/resume during the user's Play gesture, before asynchronous module loading.
    this.context=new AudioContext({latencyHint:'interactive'});const resumed=this.context.resume();
    await this.context.audioWorklet.addModule('./audio-worklet.js');await resumed;
    this.sampleRate=this.context.sampleRate;
    this.node=new AudioWorkletNode(this.context,'rustel-pcm',{numberOfInputs:0,numberOfOutputs:1,outputChannelCount:[2]});
    this.node.onprocessorerror=()=>this.failure('Audio worklet stopped');
    const pipe=new MessageChannel();
    this.worker.postMessage({kind:'channel',port:pipe.port1},[pipe.port1]);
    this.node.port.postMessage({kind:'channel',port:pipe.port2},[pipe.port2]);
    this.node.port.onmessage=({data})=>{
      if(data.kind==='transport-ack'){const pending=this.transportRequests.get(data.request);if(pending){clearTimeout(pending.timer);this.transportRequests.delete(data.request);if(data.generation===this.generation)this.frame=data.frame;pending.resolve();}}
      if(data.kind==='clock'&&data.generation===this.generation){this.frame=data.frame;this.stereo=data;this.underruns=data.underruns;}
    };
    this.analyser=new AnalyserNode(this.context,{fftSize:1024,smoothingTimeConstant:0.6});
    this.gain=new GainNode(this.context,{gain:this.volume});this.node.connect(this.analyser);this.analyser.connect(this.gain);this.gain.connect(this.context.destination);
    await this.load(structuredClone(this.score),this.demo);
  }
  async playback(playing){
    this.playing=playing;if(!this.node)return;
    const request=crypto.randomUUID();return new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{this.transportRequests.delete(request);reject(Error('Audio transport acknowledgement timed out'));},2000);
      this.transportRequests.set(request,{resolve,reject,timer});this.node.port.postMessage({kind:'play',playing,request});
    });
  }
  async command(command){
    if(command.session!==this.session||command.generation!==this.generation)throw Error('Stale browser score generation');
    const action=command.transport?.action;
    if(this.loading||this.starting){if(action==='stop')this.cancelLoad();else if(action!=='pause')throw Error('Browser audio is loading');}
    if(command.transport){
      const action=command.transport.action;
      if(action==='play'){const epoch=++this.transportEpoch;this.starting=true;try{await this.enable();if(epoch!==this.transportEpoch)throw Error('Play cancelled');await this.playback(true);}finally{this.starting=false;}}
      else if(action==='pause'){this.transportEpoch++;await this.playback(false);}
      else if(action==='stop'){this.transportEpoch++;await this.playback(false);await this.load(structuredClone(this.score),this.demo);}
      else throw Error('Browser audio does not implement seek');
    }else if(command.demo){
      if(!Object.hasOwn(this.demos,command.demo))throw Error('Unknown browser demo');
      await this.load(structuredClone(this.demos[command.demo]),command.demo);
    }else{
      const score=structuredClone(this.score);
      if(command.tempo){if(score.format==='javascript'){score.overrides.tempo=command.tempo;score.overrides.cps=command.tempo.bpm*command.tempo.bottom/(240*command.tempo.top);}else Object.assign(score,{bpm:command.tempo.bpm,top:command.tempo.top,bottom:command.tempo.bottom});}
      else{
        const index=Number(command.lane?.replace(/^#/,''));
        const lanes=score.format==='javascript'?this.metadata.lanes:score.lanes;
        if(!Number.isInteger(index)||!lanes[index])throw Error('Unknown browser lane');
        if(score.format==='javascript'){if(command.solo!=null)score.overrides.solo=command.solo?index:null;else{score.overrides.transpose||={};score.overrides.transpose[index]=command.semitones;}}
        else if(command.solo!=null)score.solo=command.solo?index:null;
        else score.lanes[index].transpose=command.semitones;
      }
      await this.load(score,this.demo);
    }
    this.publish();this.response(command.request,true);
  }
  publish(){
    if(!this.score)return;
    const javascript=this.score.format==='javascript',meta=this.metadata;
    const cps=javascript?meta.cps:this.score.bpm*this.score.bottom/(240*this.score.top),cycle=this.frame/this.sampleRate*cps;
    this.events=this.events.filter(event=>event.cycle+event.duration>=cycle-2);
    const tempo=javascript?(this.score.overrides.tempo||{bpm:cps*240,top:4,bottom:4}):{bpm:this.score.bpm,top:this.score.top,bottom:this.score.bottom};
    const solo=javascript?this.score.overrides.solo:this.score.solo;
    this.message('music/snapshot',{
      version:1,session:this.session,generation:this.generation,cycle,cps,demo:this.demo,demos:Object.keys(this.demos),
      source:javascript?this.score.source:JSON.stringify(this.score,null,2),tempo,transport:{paused:!this.playing,view_cycle:cycle},solo_lane:solo==null?null:`#${solo}`,
      lanes:javascript?meta.lanes:this.score.lanes.map((lane,index)=>({id:`#${index}`,label:lane.label,line:0,transpose:lane.transpose||0})),events:this.events,
      capabilities:{backend:javascript?'browser-js-wasm':'browser-wasm',pause:true,seek:false,stereo:true,named_parts:false,control_maps:false,demo:true,restart:false}
    });
    this.message('music/status',this.error?`Browser audio error: ${this.error}`:this.progress|| (this.playing?`Browser audio · Rustel WASM · ${this.sampleRate} Hz`:'Browser audio · press Play to enable sound'));
    if(this.analyser&&this.stereo){
      const scope=new Float32Array(512),spectrum=new Float32Array(512);this.analyser.getFloatTimeDomainData(scope);this.analyser.getFloatFrequencyData(spectrum);
      this.message('music/audio',{version:1,session:this.session,generation:this.generation,sequence:++this.sequence,stream_id:1,epoch:this.generation,end_frame:this.frame,sample_rate:this.sampleRate,device_time:this.frame/this.sampleRate,scope:Array.from(scope),spectrum:Array.from(spectrum,v=>Number.isFinite(v)?v:-120),sides:Array.from(this.stereo.left,(left,i)=>[left,this.stereo.right[i]])});
    }
  }
  close(){clearInterval(this.timer);window.removeEventListener('rustel-command',this.onCommand);this.worker.terminate();this.node?.disconnect();this.context?.close();}
}
