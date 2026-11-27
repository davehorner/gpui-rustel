// Real upstream Ratatui Studio, rendered locally from its WASM cell buffer.
export async function startStudio(audioVersion,studioVersion){
  document.title='Rustel Studio';
  const boot=document.querySelector('#boot');
  const fail=error=>{boot.hidden=false;boot.textContent=`Rustel Studio: ${error?.stack||error}`;};
  try {
    boot.textContent='Loading Rustel Studio…';
    const wasm=await import(`./studio/${studioVersion}/rustel_browser_studio.js`);await wasm.default();
    const saved=localStorage.getItem('rustel-studio-file:browser/live.strudel')||localStorage.getItem('rustel-studio-draft-v1');
    const source=saved||await fetch('./scores/Studio.strudel').then(r=>r.text());
    const studio=new wasm.Studio(source,localStorage.getItem('rustel-studio-workspace-v1')||undefined);window.rustelStudio=studio;
    const style=document.createElement('style');style.textContent=`#studio{position:fixed;inset:0;display:flex;flex-direction:column;background:#101216}#studio header{display:flex;gap:8px;align-items:center;padding:7px;background:#18202d;font:13px system-ui;flex-wrap:wrap}#studio button{color:#e8efff;background:#29384c;border:1px solid #52647d;border-radius:5px;padding:7px 12px}#studio #play{background:#1465db;border-color:#4389ef}#studio canvas{flex:1;min-height:0;width:100%;touch-action:none}#studio-status{margin-left:auto;color:#b3c5dd}#studio-input{position:absolute;left:0;bottom:0;width:1px;height:1px;opacity:.01;border:0;padding:0;font-size:16px}`;document.head.append(style);
    const root=document.createElement('main');root.id='studio';root.innerHTML='<header><strong>Rustel Studio</strong><button id="play">Play</button><button id="stop">Stop</button><button id="keyboard">Keyboard</button><button id="open">Open</button><button id="save">Download</button><button id="fullscreen">Fullscreen</button><span id="studio-status">Preparing audio…</span></header><canvas aria-label="Rustel Studio editor" tabindex="0"></canvas><textarea id="studio-input" aria-label="Studio text input" autocapitalize="off" autocomplete="off" spellcheck="false"></textarea><input type="file" id="studio-file" accept=".strudel,.js,.txt" hidden>';
    document.body.append(root);const canvas=root.querySelector('canvas'),ctx=canvas.getContext('2d'),input=root.querySelector('textarea'),status=root.querySelector('#studio-status');
    const {LocalAudioHost}=await import(`./audio-host.js?v=${audioVersion}`);
    let audio=new LocalAudioHost({local_message(topic,value){if(topic==='music/status')status.textContent=String(value);}});audio.tryAutoplay=()=>{};window.rustelStudioAudio=audio;
    let startupAudioError=null;audio.demos={Studio:{format:'javascript',studio:true,source,url:location.href,overrides:{}}};audio.demo='Studio';
    audio.demos.Studio.source=studio.source();const running=audio.load(audio.demos.Studio,'Studio').catch(error=>{startupAudioError=error;status.textContent=String(error);});let pending=Promise.resolve(),dirty=true,lastSource=source,lastState='',lastFrame='',width=80,height=30,cw=8.4,ch=18;
    const report=error=>{status.textContent=String(error);console.error(error);};
    function resize(){const rect=canvas.getBoundingClientRect(),dpr=devicePixelRatio||1;canvas.width=Math.round(rect.width*dpr);canvas.height=Math.round(rect.height*dpr);ctx.setTransform(dpr,0,0,dpr,0,0);ctx.font='14px ui-monospace,Consolas,monospace';ctx.textBaseline='top';cw=ctx.measureText('M').width;ch=18;width=Math.max(20,Math.min(300,Math.floor(rect.width/cw)));height=Math.max(10,Math.min(120,Math.floor(rect.height/ch)));dirty=true;lastFrame='';}
    new ResizeObserver(resize).observe(canvas);const fit=()=>{root.style.height=`${window.visualViewport?.height||innerHeight}px`;};window.visualViewport?.addEventListener('resize',fit);fit();resize();
    function paint(){
      const json=studio.frame(width,height);if(json===lastFrame)return;
      lastFrame=json;const frame=JSON.parse(json);
      ctx.globalAlpha=1;ctx.clearRect(0,0,canvas.width,canvas.height);
      // Paint all backgrounds first: a wide Unicode glyph spans its next cell.
      for(let i=0;i<frame.cells.length;i++){
        const [,fg,bg,flags]=frame.cells[i];
        ctx.fillStyle=flags&64?(fg||'#abb2bf'):(bg||'#101216');
        ctx.fillRect(i%frame.width*cw,Math.floor(i/frame.width)*ch,Math.ceil(cw),ch);
      }
      for(let i=0;i<frame.cells.length;i++){
        const [text,fg,bg,flags]=frame.cells[i],x=i%frame.width*cw,y=Math.floor(i/frame.width)*ch;
        if(flags&128)continue;
        ctx.fillStyle=flags&64?(bg||'#101216'):(fg||'#abb2bf');
        ctx.globalAlpha=flags&2?.65:1;
        ctx.font=`${flags&4?'italic ':''}${flags&1?'bold ':''}14px ui-monospace,Consolas,monospace`;
        if(text.trim())ctx.fillText(text,x,y+1);
        if(flags&8)ctx.fillRect(x,y+ch-2,cw,1);
        if(flags&256)ctx.fillRect(x,y+ch*.55,cw,1);
      }
      ctx.globalAlpha=1;
      if(frame.cursor){ctx.fillStyle='#e7eef9';ctx.fillRect(frame.cursor[0]*cw,frame.cursor[1]*ch,2,ch);}
    }
    const mods=e=>(e.shiftKey?1:0)|(e.ctrlKey?2:0)|(e.altKey?4:0)|(e.metaKey?8:0);
    const event=fn=>{try{fn();dirty=true;dispatch();}catch(e){report(e);}};
    function dispatch(){for(const command of JSON.parse(studio.commands())){if(command.type==='evaluate'){audio.context ||= new AudioContext({latencyHint:'interactive'});audio.context.resume().catch(report);}pending=pending.then(async()=>{
      await running;
      if(command.type==='evaluate'){
        try{await audio.load({format:'javascript',studio:true,source:command.source,url:location.href,overrides:{}},'Studio');await audio.enable();await audio.playback(true);studio.evaluated(command.request,command.revision,command.source,audio.generation,undefined);status.textContent='Playing';}
        catch(error){studio.evaluated(command.request,command.revision,command.source,audio.generation,String(error));report(error);}
      }else if(command.type==='stop'){await audio.playback(false);status.textContent='Stopped';}
      else if(command.type==='orbit-gain'||command.type==='slider'){audio.worker.postMessage({kind:'studio-control',generation:audio.generation,control:command});}
      else if(command.type==='unsupported'){report(command.message);}
      dirty=true;
    }).catch(report);}}
    root.querySelector('#play').onclick=()=>{event(()=>studio.play());};
    root.querySelector('#stop').onclick=()=>event(()=>studio.stop());
    root.querySelector('#keyboard').onclick=()=>input.focus();
    root.querySelector('#fullscreen').onclick=()=>document.fullscreenElement?document.exitFullscreen():root.requestFullscreen();
    root.querySelector('#save').onclick=()=>{const url=URL.createObjectURL(new Blob([studio.source()],{type:'text/plain'})),a=document.createElement('a');a.href=url;a.download='studio.strudel';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);};
    root.querySelector('#open').onclick=()=>root.querySelector('#studio-file').click();
    root.querySelector('#studio-file').onchange=async e=>{const file=e.target.files[0];if(!file)return;if(file.size>256*1024){report('Score exceeds 256 KiB');return;}const text=await file.text();event(()=>{studio.key('a',2);studio.paste(text);});};
    function keyboard(e){if(e.isComposing||e.key==='Unidentified'||e.key==='Process')return;if((e.ctrlKey||e.metaKey)&&['c','x','v'].includes(e.key.toLowerCase()))return;if(e.key.length===1&&!e.ctrlKey&&!e.metaKey&&!e.altKey)return;e.preventDefault();event(()=>studio.key(e.key,mods(e)));}
    input.addEventListener('keydown',keyboard);canvas.addEventListener('keydown',e=>{if(e.key.length===1&&!e.ctrlKey&&!e.metaKey&&!e.altKey){e.preventDefault();event(()=>studio.paste(e.key));}else keyboard(e);});
    input.addEventListener('beforeinput',e=>{if(e.isComposing)return;const key={deleteContentBackward:'Backspace',deleteContentForward:'Delete',insertLineBreak:'Enter',insertParagraph:'Enter'}[e.inputType];if(key){e.preventDefault();event(()=>studio.key(key,0));}else if(e.inputType==='historyUndo'||e.inputType==='historyRedo'){e.preventDefault();event(()=>studio.key(e.inputType==='historyUndo'?'z':'y',2));}});
    input.addEventListener('input',e=>{if(e.isComposing)return;const value=input.value;input.value='';if(value)event(()=>studio.paste(value));});
    input.addEventListener('compositionend',()=>{const value=input.value;input.value='';if(value)event(()=>studio.paste(value));});
    input.addEventListener('copy',e=>{e.preventDefault();event(()=>studio.key('c',2));e.clipboardData.setData('text/plain',studio.clipboard());});
    input.addEventListener('cut',e=>{e.preventDefault();event(()=>studio.key('x',2));e.clipboardData.setData('text/plain',studio.clipboard());});
    input.addEventListener('paste',e=>{e.preventDefault();event(()=>studio.paste(e.clipboardData.getData('text/plain')));});
    let dragging=false;const point=e=>{const r=canvas.getBoundingClientRect();return [Math.max(0,Math.min(width-1,Math.floor((e.clientX-r.left)/cw))),Math.max(0,Math.min(height-1,Math.floor((e.clientY-r.top)/ch)))];};
    canvas.onpointerdown=e=>{e.preventDefault();canvas.focus();if(e.pointerType==='mouse')input.focus({preventScroll:true});canvas.setPointerCapture(e.pointerId);dragging=true;event(()=>studio.mouse('down',...point(e),mods(e)));};
    canvas.onpointermove=e=>{if(dragging)event(()=>studio.mouse('drag',...point(e),mods(e)));};
    canvas.onpointerup=e=>{dragging=false;event(()=>studio.mouse('up',...point(e),mods(e)));};canvas.onpointercancel=canvas.onpointerup;
    canvas.onwheel=e=>{e.preventDefault();event(()=>studio.mouse(e.deltaY<0?'scroll-up':'scroll-down',...point(e),mods(e)));};
    boot.hidden=true;paint();
    await running.catch(report);
    if(!audio.error&&!startupAudioError)status.textContent='Ready · Play evaluates the score';
    let audioSequence=0,eventGeneration=0,lastEvent=0,lastIdlePaint=0;const timer=setInterval(()=>{try{if(audio.generation!==eventGeneration){eventGeneration=audio.generation;lastEvent=0;}const events=audio.events.filter(e=>e.id>=lastEvent);if(events.length){lastEvent=Math.max(...events.map(e=>e.id))+1;studio.events(JSON.stringify(events),audio.generation,audio.frame/audio.sampleRate,audio.metadata?.cps||.5);}if(audio.analyser&&audio.playing){const samples=new Float32Array(4096);audio.analyser.fftSize=4096;audio.analyser.getFloatTimeDomainData(samples);studio.audio(JSON.stringify({master:Array.from(samples),visuals:audio.visuals||[]}),audio.generation,++audioSequence,audio.frame/audio.sampleRate,audio.sampleRate);}studio.transport(audio.playing,audio.generation,audio.frame/audio.sampleRate*(audio.metadata?.cps||.5),audio.metadata?.cps||.5);dispatch();const state=studio.state();if(state!==lastState){lastState=state;localStorage.setItem('rustel-studio-workspace-v1',state);}if(audio.gain)audio.gain.gain.value=audio.volume*studio.gain();const next=studio.source();if(next!==lastSource){lastSource=next;localStorage.setItem('rustel-studio-file:browser/live.strudel',next);dirty=true;}if(dirty||audio.playing||Date.now()-lastIdlePaint>250){paint();dirty=false;lastIdlePaint=Date.now();}}catch(e){report(e);}},100);
    window.addEventListener('pagehide',()=>{clearInterval(timer);audio.close();studio.free();},{once:true});
  }catch(error){fail(error);}
}
