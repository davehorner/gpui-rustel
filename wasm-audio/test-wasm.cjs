// Exercises the actual compiled WASM engine through the AudioWorklet PCM consumer.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..');
const modulePath=path.resolve(process.argv[2]||path.join(root,'target/rustel-browser-audio-test/rustel_browser_audio.js'));
const {BrowserEngine}=require(modulePath);
const profiles=JSON.parse(fs.readFileSync(path.join(__dirname,'../web/audio-demos.json'),'utf8'));
const worklet=fs.readFileSync(path.join(__dirname,'../web/audio-worklet.js'),'utf8');
const reports=[];
for(const [demo,score] of Object.entries(profiles)){
  const engine=new BrowserEngine(JSON.stringify(score),48000);
  let Processor;
  const context=vm.createContext({AudioWorkletProcessor:class {constructor(){this.port={postMessage(){}};}},registerProcessor(_name,ctor){Processor=ctor;},Float32Array});
  vm.runInContext(worklet,context);const player=new Processor();
  let rendered=0;
  const pipe={start(){},postMessage(data){
    const pcm=engine.render(4096);rendered+=4096;
    this.onmessage({data:{kind:'chunk',generation:data.generation,pcm}});
  }};
  player.port.onmessage({data:{kind:'channel',port:pipe}});
  player.port.onmessage({data:{kind:'reset',generation:1}});
  player.port.onmessage({data:{kind:'play',playing:true}});
  let peak=0;
  for(let n=0;n<750;n++){
    const output=[new Float32Array(128),new Float32Array(128)];player.process([], [output]);
    for(const channel of output)for(const sample of channel){assert(Number.isFinite(sample));peak=Math.max(peak,Math.abs(sample));}
    assert(player.queue.length<=4);
  }
  assert(peak>0.001);assert(player.frame>=95872);
  player.port.onmessage({data:{kind:'play',playing:false}});const frozen=player.frame;
  player.process([] ,[[new Float32Array(128),new Float32Array(128)]]);assert.equal(player.frame,frozen);
  const queued=player.queue.length;
  player.port.onmessage({data:{kind:'chunk',generation:0,pcm:new Float32Array(8192)}});assert.equal(player.queue.length,queued);
  reports.push({demo,renderedFrames:rendered,playedFrames:player.frame,peak,underrunBlocks:player.underruns,pauseVerified:true,staleGenerationRejected:true});engine.free();
}
console.log(JSON.stringify(reports,null,2));
