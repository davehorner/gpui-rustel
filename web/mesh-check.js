import captured from './mesh-pipeline-data.js?v=1';
const constantFragment=`struct Out {@location(0) color:vec4f,@location(1) entity:f32,@location(2) normal:vec4f,@location(3) velocity:vec2f}; @fragment fn fs_probe()->Out {return Out(vec4f(1,0,0,1),0,vec4f(0,0,1,0),vec2f(0));}`;
const output=captured.source.match(/struct VertexOutput\s*\{[\s\S]*?\}/)[0];
const constantVertex=output+` @vertex fn vs_probe(@builtin(vertex_index) i:u32)->VertexOutput {var p=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));var out:VertexOutput;out.position=vec4f(p[i],0.5,1);out.normal=vec3f(0,0,1);out.view_normal=vec3f(0,0,1);out.instance_tint=vec4f(1);out.vertex_color=vec4f(1);out.world_tangent=vec4f(1,0,0,1);return out;}`;
function oneSampler(source){
  const pattern=/@group\(2\)\s*@binding\(([2-7])\)\s*var (\w+): sampler;/g;
  const declarations=[...source.matchAll(pattern)],first=declarations.find(item=>item[1]==='2')[2];
  for(const item of declarations)if(item[1]!=='2'){source=source.replace(item[0],'');source=source.replaceAll(item[2],first);}
  return source;
}
export async function runMeshCheck(record){
  record({rendererRevision:captured.revision,profile:captured.profile,targets:captured.descriptor.fragment.targets});
  const cases=[
    {name:'Simple shaders with exact four mesh render targets',vertex:false,fragment:false},
    {name:'Simple shaders with component-matched masks and no replace blend',vertex:false,fragment:false,masks:true},
    {name:'Actual mesh vertex with constant fragment',vertex:true,fragment:false},
    {name:'Constant vertex with actual compact fragment',vertex:false,fragment:true},
    {name:'Exact compact mesh vertex and fragment',vertex:true,fragment:true},
    {name:'Exact compact mesh with one material sampler',vertex:true,fragment:true,sampler:true}
  ];
  let failed=0;
  for(const test of cases){
    record({test:test.name,stage:'Requesting device'});let device,cleaning=false;const resources=[];
    let timer;
    try{
      if(!navigator.gpu)throw Error('WebGPU is unavailable');const adapter=await navigator.gpu.requestAdapter();if(!adapter)throw Error('No GPU adapter');
      record({test:test.name,adapter:{vendor:adapter.info?.vendor,architecture:adapter.info?.architecture},limits:{maxSamplersPerShaderStage:adapter.limits.maxSamplersPerShaderStage,maxSampledTexturesPerShaderStage:adapter.limits.maxSampledTexturesPerShaderStage,maxStorageBuffersPerShaderStage:adapter.limits.maxStorageBuffersPerShaderStage}});
      device=await adapter.requestDevice({requiredFeatures:['indirect-first-instance'],requiredLimits:{maxStorageBuffersPerShaderStage:16}});
      device.lost.then(info=>{if(!cleaning)record({test:test.name,deviceLost:{reason:info.reason,message:info.message}});});
      device.addEventListener('uncapturederror',event=>record({test:test.name,uncapturedError:event.error.message}));
      device.pushErrorScope('out-of-memory');device.pushErrorScope('internal');device.pushErrorScope('validation');
      const groups=captured.descriptor.layout.map(desc=>device.createBindGroupLayout(desc));
      const layout=device.createPipelineLayout({bindGroupLayouts:groups});
      const bindGroups=captured.descriptor.layout.map((desc,index)=>device.createBindGroup({layout:groups[index],entries:desc.entries.map(entry=>{
        let resource;
        if(entry.buffer){const buffer=device.createBuffer({size:65536,usage:(entry.buffer.type==='uniform'?GPUBufferUsage.UNIFORM:GPUBufferUsage.STORAGE)|GPUBufferUsage.COPY_DST});resources.push(buffer);resource={buffer};}
        else if(entry.sampler){resource=device.createSampler(entry.sampler.type==='comparison'?{compare:'less-equal'}:entry.sampler.type==='filtering'?{minFilter:'linear',magFilter:'linear',mipmapFilter:'linear'}:{});}
        else if(entry.texture){const t=entry.texture,dimension=t.viewDimension||'2d',layers=dimension.startsWith('cube')?6:1;
          const format=t.sampleType==='depth'?'depth32float':t.sampleType==='uint'?'r32uint':t.sampleType==='sint'?'r32sint':t.sampleType==='unfilterable-float'?'r32float':'rgba8unorm';
          const texture=device.createTexture({size:[1,1,layers],dimension:dimension==='3d'?'3d':dimension==='1d'?'1d':'2d',format,usage:GPUTextureUsage.TEXTURE_BINDING});resources.push(texture);resource=texture.createView({dimension});
        }else throw Error('Unhandled binding '+JSON.stringify(entry));
        return {binding:entry.binding,resource};
      })}));
      const source=test.sampler?oneSampler(captured.source):captured.source;
      const vertex=device.createShaderModule({code:test.vertex?source:constantVertex}),fragment=device.createShaderModule({code:test.fragment?source:constantFragment});
      const targets=structuredClone(captured.descriptor.fragment.targets);
      if(test.masks){for(const target of targets)delete target.blend;targets[1].writeMask=1;targets[3].writeMask=3;}
      record({test:test.name,stage:'Creating pipeline'});
      const pipeline=device.createRenderPipeline({...captured.descriptor,label:test.name,layout,vertex:{...captured.descriptor.vertex,module:vertex,entryPoint:test.vertex?'vs_main':'vs_probe'},fragment:{...captured.descriptor.fragment,module:fragment,entryPoint:test.fragment?'fs_main':'fs_probe',targets}});
      const colors=targets.map(target=>{const texture=device.createTexture({size:[64,64],format:target.format,usage:GPUTextureUsage.RENDER_ATTACHMENT});resources.push(texture);return {view:texture.createView(),loadOp:'clear',storeOp:'store',clearValue:{r:0,g:0,b:0,a:1}};});
      const depth=device.createTexture({size:[64,64],format:captured.descriptor.depthStencil.format,usage:GPUTextureUsage.RENDER_ATTACHMENT});resources.push(depth);
      const encoder=device.createCommandEncoder(),pass=encoder.beginRenderPass({colorAttachments:colors,depthStencilAttachment:{view:depth.createView(),depthClearValue:0,depthLoadOp:'clear',depthStoreOp:'store'}});
      pass.setPipeline(pipeline);bindGroups.forEach((group,index)=>pass.setBindGroup(index,group));
      captured.descriptor.vertex.buffers.forEach((desc,index)=>{const buffer=device.createBuffer({size:Math.max(256,desc.arrayStride*3),usage:GPUBufferUsage.VERTEX});resources.push(buffer);pass.setVertexBuffer(index,buffer);});
      const indices=device.createBuffer({size:12,usage:GPUBufferUsage.INDEX|GPUBufferUsage.COPY_DST});resources.push(indices);device.queue.writeBuffer(indices,0,new Uint32Array([0,1,2]));pass.setIndexBuffer(indices,'uint32');pass.drawIndexed(3);pass.end();
      record({test:test.name,stage:'Submitting draw and waiting for GPU'});device.queue.submit([encoder.finish()]);
      await Promise.race([device.queue.onSubmittedWorkDone(),device.lost.then(info=>{throw Error('Device lost: '+info.message);}),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('GPU timed out after 15 seconds')),15000);})]);
      const errors=[];for(let i=0;i<3;i++){const error=await device.popErrorScope();if(error)errors.push({type:error.constructor.name,message:error.message});}
      if(errors.length){failed++;record({test:test.name,status:'Failed',errors});}else record({test:test.name,status:'Passed'});
    }catch(error){failed++;record({test:test.name,status:'Failed',error:String(error?.stack||error)});}
    finally{clearTimeout(timer);cleaning=true;for(const resource of resources)resource.destroy();device?.destroy();}
  }
  return {passed:cases.length-failed,failed,total:cases.length};
}
