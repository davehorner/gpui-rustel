// Deliberately independent of GPUI, Nightshade, WASM and audio.
export async function runGpuCheck(record) {
  const devices=[];let disposing=false;
  const failures=[];
  async function device(label){
    record(`Requesting ${label} adapter`);
    if(!navigator.gpu)throw Error('WebGPU is unavailable');
    const adapter=await navigator.gpu.requestAdapter();
    if(!adapter)throw Error('No GPU adapter');
    record({label,features:[...adapter.features],info:adapter.info?{vendor:adapter.info.vendor,architecture:adapter.info.architecture}:null});
    if(!adapter.features.has('indirect-first-instance'))throw Error('indirect-first-instance unavailable');
    const result=await adapter.requestDevice({requiredFeatures:['indirect-first-instance']});
    devices.push(result);
    result.addEventListener('uncapturederror',event=>{failures.push(event.error.message);record({label,uncapturedError:event.error.message});});
    result.lost.then(info=>{if(!disposing)record({label,deviceLost:{reason:info.reason,message:info.message}});});
    return result;
  }
  async function bounded(device,operation){
    let timer;
    try{return await Promise.race([operation(),device.lost.then(info=>{throw Error(`Device lost: ${info.reason}: ${info.message}`);}),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('GPU operation timed out after 15 seconds')),15000);})]);}
    finally{clearTimeout(timer);}
  }
  async function readback(device,format,width,height,draw=null){
    const label=`${format} ${width}x${height}${draw?' '+draw.mode+' draw firstInstance='+draw.firstInstance:''}`;
    record(`Starting ${label}`);
    let texture,buffer,args;
    device.pushErrorScope('validation');
    try{
      texture=device.createTexture({size:[width,height],format,usage:GPUTextureUsage.RENDER_ATTACHMENT|GPUTextureUsage.COPY_SRC});
      const stride=Math.ceil(width*4/256)*256;
      buffer=device.createBuffer({size:stride*height,usage:GPUBufferUsage.COPY_DST|GPUBufferUsage.MAP_READ});
      const encoder=device.createCommandEncoder();
      const pass=encoder.beginRenderPass({colorAttachments:[{view:texture.createView(),loadOp:'clear',storeOp:'store',clearValue:{r:draw?0:1,g:0,b:0,a:1}}]});
      if(draw){
        const shader=device.createShaderModule({code:`struct Out {@builtin(position) p:vec4f,@location(0) @interpolate(flat) i:u32}; @vertex fn vs(@builtin(vertex_index) v:u32,@builtin(instance_index) i:u32)->Out {var p=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));return Out(vec4f(p[v],0,1),i);} @fragment fn fs(in:Out)->@location(0) vec4f {if(in.i==${draw.firstInstance}u){return vec4f(1,0,0,1);}return vec4f(0,0,1,1);}`});
        const pipeline=device.createRenderPipeline({layout:'auto',vertex:{module:shader,entryPoint:'vs'},fragment:{module:shader,entryPoint:'fs',targets:[{format}]}});
        args=device.createBuffer({size:16,usage:GPUBufferUsage.INDIRECT|GPUBufferUsage.COPY_DST});device.queue.writeBuffer(args,0,new Uint32Array([3,1,0,draw.firstInstance]));
        pass.setPipeline(pipeline);if(draw.mode==='direct')pass.draw(3,1,0,draw.firstInstance);else pass.drawIndirect(args,0);
      }
      pass.end();encoder.copyTextureToBuffer({texture},{buffer,bytesPerRow:stride,rowsPerImage:height},{width,height,depthOrArrayLayers:1});
      device.queue.submit([encoder.finish()]);
      await bounded(device,()=>buffer.mapAsync(GPUMapMode.READ));
      const bytes=new Uint8Array(buffer.getMappedRange());const expected=format==='bgra8unorm'?[0,0,255,255]:[255,0,0,255];
      for(let y=0;y<height;y++)for(let x=0;x<width;x++)for(let c=0;c<4;c++)if(bytes[y*stride+x*4+c]!==expected[c])throw Error(`${label}: unexpected pixel at ${x},${y}; raw bytes=${Array.from(bytes.slice(y*stride+x*4,y*stride+x*4+4))}; expected=${expected}`);
      buffer.unmap();record(`Passed ${label}`);
    }finally{buffer?.destroy();texture?.destroy();args?.destroy();const error=await bounded(device,()=>device.popErrorScope());if(error)throw Error(error.message);}
  }
  try{
    const first=await device('first device');
    await readback(first,'rgba8unorm',1,1);await readback(first,'bgra8unorm',1,1);
    await readback(first,'rgba8unorm',640,480);await readback(first,'bgra8unorm',640,480);
    for(const format of ['rgba8unorm','bgra8unorm'])for(const mode of ['direct','indirect'])for(const firstInstance of [0,1]){
      try{await readback(first,format,64,64,{mode,firstInstance});}
      catch(error){failures.push(String(error));record({failedCase:{format,mode,firstInstance},error:String(error)});}
    }
    const second=await device('second simultaneous device');
    await readback(second,'bgra8unorm',640,480);await readback(first,'bgra8unorm',640,480);
    if(failures.length)throw Error(failures.join('; '));record('All GPU checks passed');
  }finally{disposing=true;for(const device of devices)device.destroy();}
}
