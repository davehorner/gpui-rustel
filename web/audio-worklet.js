class RustelPCM extends AudioWorkletProcessor {
  constructor(){
    super();this.pipe=null;this.queue=[];this.offset=0;this.generation=0;this.playing=false;this.started=false;this.frame=0;this.blocks=0;this.requested=0;this.underruns=0;
    this.left=new Float32Array(512);this.right=new Float32Array(512);this.at=0;
    this.port.onmessage=({data})=>{
      if(data.kind==='channel'){this.pipe=data.port;this.pipe.onmessage=({data})=>{if(data.kind==='chunk'&&data.generation===this.generation){this.requested=Math.max(0,this.requested-1);if(this.queue.length<4)this.queue.push(data.pcm);}};this.pipe.start();}
      if(data.kind==='reset'){this.started=false;this.queue=[];this.offset=0;this.frame=0;this.generation=data.generation;this.requested=0;this.left.fill(0);this.right.fill(0);}
      if(data.kind==='chunk'&&data.generation===this.generation){this.requested=Math.max(0,this.requested-1);if(this.queue.length<4)this.queue.push(data.pcm);}
      if(data.kind==='play'){this.playing=data.playing;if(data.request)this.port.postMessage({kind:'transport-ack',request:data.request,generation:this.generation,frame:this.frame});}
    };
  }
  process(_inputs,outputs){
    const channels=outputs[0];if(!channels?.length)return true;
    const left=channels[0],right=channels[1]||left;
    if(this.playing&&!this.started&&this.queue.length>=2)this.started=true;
    if(this.playing&&this.started){
      if(this.queue.length===0)this.underruns++;
      for(let i=0;i<left.length;i++){
        const pcm=this.queue[0];if(!pcm)break;
        left[i]=pcm[this.offset++];right[i]=pcm[this.offset++];
        this.left[this.at]=left[i];this.right[this.at]=right[i];this.at=(this.at+1)%512;this.frame++;
        if(this.offset>=pcm.length){this.queue.shift();this.offset=0;}
      }
    }
    while(this.pipe&&this.queue.length+this.requested<3){this.requested++;this.pipe.postMessage({kind:'need',generation:this.generation});}
    if(++this.blocks%16===0)this.port.postMessage({kind:'clock',generation:this.generation,frame:this.frame,playing:this.playing,underruns:this.underruns,left:this.left,right:this.right});
    return true;
  }
}
registerProcessor('rustel-pcm',RustelPCM);
