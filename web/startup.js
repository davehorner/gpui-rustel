// Startup UI stays outside WASM so a graphics failure cannot hide its diagnostics.
export function validateAdapter(adapter) {
  if (!adapter) throw Error('Chrome exposes WebGPU, but no GPU adapter is available.');
  if (!adapter.features.has('indirect-first-instance')) throw Error('This GPU does not offer indirect-first-instance, which the current Nightshade renderer requires.');
  if (adapter.limits.maxBindGroups < 4) throw Error('This GPU offers fewer than the four bind groups required by Nightshade.');
}
export function startup(boot,name,build) {
  const params=new URLSearchParams(location.search),key=`${name}-startup`;
  let previous;try{previous=JSON.parse(localStorage.getItem(key));}catch{}
  const state={name,build,phase:'Checking browser',ready:false,error:null,lines:[],previous};
  const status=document.createElement('div'),controls=document.createElement('div'),details=document.createElement('details'),summary=document.createElement('summary'),report=document.createElement('pre');
  summary.textContent='Startup diagnostics';details.append(summary,report);report.style.cssText='white-space:pre-wrap;overflow-wrap:anywhere;max-height:48vh;overflow:auto;font:12px monospace';
  controls.style.cssText='display:flex;gap:8px;flex-wrap:wrap';
  const start=document.createElement('button'),copy=document.createElement('button'),probe=document.createElement('button');
  start.textContent='Start app';start.hidden=true;copy.textContent='Copy diagnostics';probe.textContent='Test GPU';probe.disabled=true;
  for(const button of [start,copy,probe]){button.style.cssText='font:inherit;padding:8px 12px';controls.append(button);}
  boot.style.cssText+=';z-index:2147483647;overflow:auto';boot.replaceChildren(status,controls,details);
  function record(message){state.lines.push(typeof message==='string'?message:JSON.stringify(message));report.textContent=JSON.stringify(state,null,2);}
  function phase(message){state.phase=message;status.textContent=`${name}: ${message}`;record(message);try{localStorage.setItem(key,JSON.stringify({build,phase:message,pending:!waitingForUser,time:Date.now()}));}catch{}}
  function fail(error){state.error??=String(error?.stack||error);boot.hidden=false;status.textContent=`${name} could not start.\n${state.error}`;details.open=true;record(String(error));try{localStorage.setItem(key,JSON.stringify({build,phase:state.phase,pending:false,error:state.error}));}catch{}}
  let appReady=false,frameReady=false,waitingForUser=false;
  function finish(){if(!appReady||!frameReady||state.error)return;state.ready=true;phase('Ready');try{localStorage.setItem(key,JSON.stringify({build,phase:'Ready',pending:false}));}catch{}boot.hidden=true;}
  copy.onclick=async()=>{try{await navigator.clipboard.writeText(JSON.stringify(state,null,2));copy.textContent='Copied';}catch{details.open=true;copy.textContent='Select diagnostics below';}};
  window.addEventListener('error',event=>fail(event.error||event.message));
  window.addEventListener('unhandledrejection',event=>fail(event.reason));
  for(const eventName of ['nightshade-panic','rustel-panic'])window.addEventListener(eventName,event=>{state.error=null;fail(event.detail);});
  window.addEventListener('nightshade-stage',event=>{
    const {stage,message}=event.detail||{};
    if(stage==='error')fail(message);
    else if(stage==='ready'){frameReady=true;record(message);finish();}
    else phase(message);
  });
  window.addEventListener('pageshow',event=>{if(event.persisted)location.reload();});
  setTimeout(()=>{if(!waitingForUser&&!state.ready&&!state.error){status.textContent=`Still starting: ${state.phase}. Open startup diagnostics for details.`;details.open=true;}},30000);
  record({userAgent:navigator.userAgent,secureContext:globalThis.isSecureContext,devicePixelRatio:devicePixelRatio,deviceMemory:navigator.deviceMemory});
  return {
    phase,fail,
    appReady(){appReady=true;finish();},
    async check(){
      if(params.has('diagnostics')||previous?.pending){
        waitingForUser=true;
        phase(previous?.pending?`Previous startup stopped at: ${previous.phase}`:'Diagnostics only. No GPU or WASM has been started. Copy diagnostics before starting.');details.open=true;start.hidden=false;
        await new Promise(resolve=>{start.onclick=()=>{waitingForUser=false;start.hidden=true;resolve();};});
      }
      if(!navigator.gpu)throw Error('WebGPU is not exposed by this Chrome/device. The current GPUI/Nightshade apps require WebGPU.');
      phase('Requesting a WebGPU adapter');
      const adapter=await navigator.gpu.requestAdapter();
      if(adapter)record(JSON.stringify({features:[...adapter.features],info:adapter.info?Object.fromEntries(["vendor","architecture","device","description"].map(key=>[key,adapter.info[key]])):null,limits:Object.fromEntries(['maxBindGroups','maxTextureDimension2D','maxTextureArrayLayers','maxBufferSize','maxStorageBufferBindingSize','maxStorageBuffersPerShaderStage'].map(key=>[key,adapter.limits[key]]))}));
      validateAdapter(adapter);probe.disabled=false;
      probe.onclick=async()=>{
        probe.disabled=true;let device;
        try{
          phase('Testing a tiny WebGPU render');device=await adapter.requestDevice({requiredFeatures:['indirect-first-instance']});
          device.pushErrorScope('validation');
          const texture=device.createTexture({size:[1,1],format:'rgba8unorm',usage:GPUTextureUsage.RENDER_ATTACHMENT});
          const encoder=device.createCommandEncoder(),pass=encoder.beginRenderPass({colorAttachments:[{view:texture.createView(),loadOp:'clear',storeOp:'store',clearValue:{r:0,g:1,b:0,a:1}}]});
          pass.end();device.queue.submit([encoder.finish()]);await device.queue.onSubmittedWorkDone();
          const error=await device.popErrorScope();if(error)throw Error(error.message);texture.destroy();phase('Tiny WebGPU render passed');
        }catch(error){fail(error);}finally{device?.destroy();probe.disabled=false;}
      };

    }
  };
}
