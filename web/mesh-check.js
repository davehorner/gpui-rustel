import captured from './mesh-pipeline-data.js?v=1';
const constantFragment=`struct Out {@location(0) color:vec4f,@location(1) entity:f32,@location(2) normal:vec4f,@location(3) velocity:vec2f}; @fragment fn fs_probe()->Out {return Out(vec4f(1,0,0,1),0,vec4f(0,0,1,0),vec2f(0));}`;
const output=captured.source.match(/struct VertexOutput\s*\{[\s\S]*?\}/)[0];
const constantVertex=output+` @vertex fn vs_probe(@builtin(vertex_index) i:u32)->VertexOutput {var p=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));var out:VertexOutput;out.position=vec4f(p[i],0.5,1);out.normal=vec3f(0,0,1);out.view_normal=vec3f(0,0,1);out.instance_tint=vec4f(1);out.vertex_color=vec4f(1);out.world_tangent=vec4f(1,0,0,1);return out;}`;
function functionParts(source){
  const start=source.indexOf('fn fs_main('),open=source.indexOf('{',start);let end=open+1,depth=1;
  while(depth){if(source[end]==='{')depth++;else if(source[end]==='}')depth--;end++;}
  return {start,open,end,body:source.slice(open+1,end-1)};
}
function withBody(source,body,parameters=null){const p=functionParts(source);const head=parameters===null?source.slice(p.start,p.open):`fn fs_main(${parameters}) -> FragmentOutput `;return source.slice(0,p.start)+head+'{'+body+'}'+source.slice(p.end);}
function result(color){return `return FragmentOutput(${color},0.0,vec4<f32>(0.0,0.0,1.0,0.0),vec2<f32>(0.0));`;}
function perspective(source){return source.replaceAll('@interpolate(linear)','@interpolate(perspective)');}
function withoutLightLoop(source){const p=functionParts(source),start=p.body.indexOf('loop {');if(start<0)throw Error('Light loop missing');let end=p.body.indexOf('{',start)+1,depth=1;while(depth){if(p.body[end]==='{')depth++;else if(p.body[end]==='}')depth--;end++;}return withBody(source,p.body.slice(0,start)+p.body.slice(end));}
function withoutIbl(source){let body=functionParts(source).body;for(const name of ['irradiance_map','prefiltered_env','brdf_lut']){const start=body.indexOf('textureSampleLevel('+name+',');if(start<0)throw Error('IBL sample missing: '+name);let end=body.indexOf('(',start)+1,depth=1;while(depth){if(body[end]==='(')depth++;else if(body[end]===')')depth--;end++;}body=body.slice(0,start)+'vec4<f32>(0.03,0.03,0.03,1.0)'+body.slice(end);}return withBody(source,body);}
function variantSource(test){
  let source=captured.source;
  const texture=source.match(/@group\(2\)\s*@binding\(0\)\s*var (\w+)/)[1],sampler=source.match(/@group\(2\)\s*@binding\(2\)\s*var (\w+)/)[1];
  const input=source.match(/fn fs_main\((\w+):/)[1];
  switch(test.variant){
    case 'constant-interface':source=withBody(source,result('vec4<f32>(1.0,0.0,0.0,1.0)'));break;
    case 'constant-no-input':source=withBody(source,result('vec4<f32>(1.0,0.0,0.0,1.0)'), '');break;
    case 'linear-only':source=withBody(source,result('vec4<f32>(uv,0.0,1.0)'),'@location(8) @interpolate(linear) uv: vec2<f32>');break;
    case 'perspective-only':source=perspective(withBody(source,result('vec4<f32>(uv,0.0,1.0)'),'@location(8) @interpolate(linear) uv: vec2<f32>'));break;
    case 'material-field':source=perspective(withBody(source,result(`materials[${input}.material_id].base_color`)));break;
    case 'material-copy':source=perspective(withBody(source,`let material = materials[${input}.material_id];`+result('material.base_color')));break;
    case 'texture-level':source=perspective(withBody(source,result(`textureSampleLevel(${texture},${sampler},${input}.tex_coords,0,0.0)`)));break;
    case 'texture-grad':source=perspective(withBody(source,result(`textureSampleGrad(${texture},${sampler},${input}.tex_coords,0,dpdx(${input}.tex_coords),dpdy(${input}.tex_coords))`)));break;
    case 'no-light-loop':source=perspective(withoutLightLoop(source));break;
    case 'no-ibl':source=perspective(withoutIbl(source));break;
    case 'perspective':source=perspective(source);break;
  }
  return source;
}
export async function runMeshCheck(record){
  record({rendererRevision:captured.revision,profile:captured.profile,targets:captured.descriptor.fragment.targets});
  const cases=[
    {name:'Simple shaders with exact four mesh render targets',vertex:false,fragment:false},
    {name:'Original module with constant fragment and original input interface',variant:'constant-interface'},
    {name:'Original module with constant fragment and no fragment inputs',variant:'constant-no-input'},
    {name:'Exact compact mesh vertex and fragment',variant:'exact'},
    {name:'Exact shaders with perspective instead of linear interpolation',variant:'perspective'},
    {name:'Fragment reads only one linear interpolation varying',variant:'linear-only'},
    {name:'Fragment reads only one perspective interpolation varying',variant:'perspective-only'},
    {name:'Fragment reads one material storage field directly',variant:'material-field'},
    {name:'Fragment copies the material struct then reads base color',variant:'material-copy'},
    {name:'Fragment samples only the material texture array with explicit LOD',variant:'texture-level'},
    {name:'Fragment samples only the material texture array with gradients',variant:'texture-grad'},
    {name:'Exact fragment without the direct light loop',variant:'no-light-loop'},
    {name:'Exact fragment without IBL texture samples',variant:'no-ibl'}
  ];
  for(const test of cases)if(test.variant){test.vertex=true;test.fragment=true;}
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
      const source=variantSource(test);
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
