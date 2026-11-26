// Browser fetch/decode host for Rustel's native sample-bank declarations.
export function parseFont(text){
  const start=text.search(/zones\s*:/);if(start<0)throw Error('Soundfont has no zones');
  const array=text.indexOf('[',start),end=text.lastIndexOf(']');
  const body=text.slice(array,end+1);let json='',quote=null;
  for(let i=0;i<body.length;i++){
    const ch=body[i];
    if(quote){if(ch==='\\'){json+=ch+(body[++i]||'');}else if(ch===quote){json+='"';quote=null;}else json+=ch;}
    else if(ch==="'"||ch==='"'){quote=ch;json+='"';}
    else if(ch==='/'&&body[i+1]==='/'){while(i<body.length&&body[i]!=='\n')i++;json+='\n';}
    else if(ch==='/'&&body[i+1]==='*'){i+=2;while(i<body.length&&!(body[i]==='*'&&body[i+1]==='/'))i++;i++;}
    else json+=ch;
  }
  json=json.replace(/([{,]\s*)([A-Za-z_$][\w$]*)\s*:/g,'$1"$2":').replace(/,\s*([}\]])/g,'$1');
  json=json.replace(/("(?:originalPitch|coarseTune|fineTune|sampleRate|loopStart|loopEnd|keyRangeLow|keyRangeHigh)"\s*:\s*)(-?\d+(?:\.\d+)?(?:\s*[+-]\s*\d+(?:\.\d+)?)+)/g,(_all,key,expression)=>key+(expression.replace(/\s/g,'').match(/[+-]?\d+(?:\.\d+)?/g)||[]).reduce((sum,n)=>sum+Number(n),0));
  return JSON.parse(json);
}
const MAX_BYTES=32*1024*1024;
export class SampleLibrary {
  constructor(decode,progress){this.decode=decode;this.progress=progress;this.custom=new Map();this.defaults=new Map();this.manifests=new Map();this.pcm=new Map();this.fonts=new Map();this.aliases=new Map();this.installed=new WeakMap();}
  async init(){[this.pins,this.gm]=await Promise.all([fetch('./banks/sample-banks.json').then(r=>r.json()),fetch('./banks/gm-fonts.json').then(r=>r.json())]);this.registerMap(this.defaults,this.pins.inline.banks,this.pins.inline.base);this.gm.fonts.gm_electric_piano_1=this.gm.fonts.gm_epiano1;this.gm.fonts.gm_electric_piano_2=this.gm.fonts.gm_epiano2;}
  url(value,base){
    if(value.startsWith('github:')){const [owner,repo='samples',branch='main',...dir]=value.slice(7).split('/');return `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/${dir.length?dir.join('/')+'/':''}strudel.json`;}
    const url=new URL(value,base||location.href);if(!['http:','https:'].includes(url.protocol))throw Error('Browser samples require HTTP(S) URLs');return url.href;
  }
  async bytes(url){
    const response=await fetch(url,{signal:AbortSignal.timeout(30000),credentials:'omit'});if(!response.ok)throw Error(`Sample fetch ${response.status}: ${url}`);
    if(Number(response.headers.get('content-length'))>MAX_BYTES)throw Error(`Sample exceeds 32 MiB: ${url}`);
    const bytes=await response.arrayBuffer();if(bytes.byteLength>MAX_BYTES)throw Error(`Sample exceeds 32 MiB: ${url}`);return bytes;
  }
  registerMap(map,value,base){
    const root=value._base?this.url(value._base,base):base;
    for(const [name,bank] of Object.entries(value))if(!name.startsWith('_'))map.set(name.toLowerCase(),{bank,base:root});
  }
  async manifest(value,base,target){
    if(typeof value==='string'){
      const url=this.url(value,base);let data=this.manifests.get(url);if(!data){data=JSON.parse(new TextDecoder().decode(await this.bytes(url)));this.manifests.set(url,data);}
      this.registerMap(target,data,url);
    }else if(value&&typeof value==='object')this.registerMap(target,value,base);
    else throw Error('Invalid samples() manifest');
  }
  async forScore(effects,sourceUrl){
    const local=new SampleLibrary(this.decode,this.progress);local.pins=this.pins;local.gm=this.gm;local.defaults=this.defaults;local.manifests=this.manifests;local.pcm=this.pcm;local.fonts=this.fonts;local.aliases=this.aliases;
    for(const [json,base] of effects)await local.manifest(JSON.parse(json),base?this.url(base,sourceUrl):sourceUrl,local.custom);
    return local;
  }
  async find(name){
    if(this.custom.has(name))return this.custom.get(name);
    if(this.defaults.has(name))return this.defaults.get(name);
    const aliased=()=>{const split=name.indexOf('_');if(split<0)return;const prefix=this.aliases.get(name.slice(0,split));return prefix&&this.defaults.get(prefix+name.slice(split));};
    if(aliased())return aliased();
    for(const source of this.pins.sources){
      if(source.name.includes('waveform'))continue;
      const key=source.url;if(!this.manifests.has(key)){
        const bytes=await this.bytes(key),digest=await crypto.subtle.digest('SHA-256',bytes);
        const hash=Array.from(new Uint8Array(digest),v=>v.toString(16).padStart(2,'0')).join('');
        if(source.sha256&&hash!==source.sha256)throw Error(`Pinned sample manifest changed: ${source.name}`);
        const data=JSON.parse(new TextDecoder().decode(bytes));this.manifests.set(key,data);
        if(source.base==null){for(const [canonical,names] of Object.entries(data))for(const alias of Array.isArray(names)?names:[names])this.aliases.set(String(alias).toLowerCase(),canonical.toLowerCase());}
        else this.registerMap(this.defaults,data,source.base);
      }
      if(this.defaults.has(name))return this.defaults.get(name);if(aliased())return aliased();
    }
    throw Error(`No browser sample bank resolves ${name}`);
  }
  note(value){if(typeof value==='number'||/^-?\d+(?:\.\d+)?$/.test(value))return Number(value);const match=/^([a-g])([#b]?)(-?\d+)$/i.exec(value);if(!match)throw Error(`Invalid note-mapped sample key ${value}`);return (Number(match[3])+1)*12+{c:0,d:2,e:4,f:5,g:7,a:9,b:11}[match[1].toLowerCase()]+(match[2]==='#'?1:match[2]==='b'?-1:0);}
  index(n,length){return ((Math.round(Number(n)||0)%length)+length)%length;}
  async decoded(url,sampleRate){
    const key=`${sampleRate}:${url}`;let value=this.pcm.get(key);if(value)return value;
    value=await this.decode(await this.bytes(url),sampleRate);this.pcm.set(key,value);this.trim();return value;
  }
  trim(){let bytes=0;for(const pcm of this.pcm.values())bytes+=pcm.pcm.byteLength;while(bytes>128*1024*1024&&this.pcm.size>1){const key=this.pcm.keys().next().value;bytes-=this.pcm.get(key).pcm.byteLength;this.pcm.delete(key);}}
  install(engine,key,decoded){let ids=this.installed.get(engine);if(!ids){ids=new Map();this.installed.set(engine,ids);}if(!ids.has(key))ids.set(key,engine.install_pcm(decoded.sampleRate,decoded.channels,decoded.pcm));return ids.get(key);}
  async ensure(engine,controls,sampleRate){
    const sound=String(controls.s||controls.sound||'sine'),original=controls.bank==null?sound:`${controls.bank}_${sound}`,name=original.toLowerCase();
    if(!this.custom.has(name)&&!this.gm.fonts[name]&&!engine.needs_sample(JSON.stringify(controls)))return;
    this.progress(`Loading ${original}…`);
    const n=Number(controls.n)||0;let entries=[];
    if(this.gm.fonts[name]&&!this.custom.has(name)){
      const names=this.gm.fonts[name],variant=this.index(n,names.length),url=`${this.gm.base}/${names[variant]}.js`;
      let zones=this.fonts.get(url);if(!zones){zones=parseFont(new TextDecoder().decode(await this.bytes(url)));this.fonts.set(url,zones);}
      for(let i=0;i<zones.length;i++){
        const zone=zones[i],zoneRate=zone.sampleRate||44100,key=`${url}:${i}:${sampleRate}`;let decoded=this.pcm.get(key);
        if(!decoded){
          const bytes=Uint8Array.from(atob(zone.file||zone.sample),c=>c.charCodeAt(0));
          if(zone.file){try{decoded=await this.decode(bytes.buffer,sampleRate);}catch(error){continue;}}
          else{const view=new DataView(bytes.buffer),pcm=new Float32Array(Math.floor(bytes.byteLength/2));for(let j=0;j<pcm.length;j++)pcm[j]=view.getInt16(j*2,true)/32768;decoded={pcm,sampleRate:zoneRate,channels:1};}
          this.pcm.set(key,decoded);this.trim();
        }
        const id=this.install(engine,key,decoded);
        entries.push({id,variant,variant_count:names.length,root:(zone.originalPitch-100*(zone.coarseTune||0)-(zone.fineTune||0))/100,duration:decoded.pcm.length/decoded.channels/decoded.sampleRate,font:true,low:zone.keyRangeLow,high:zone.keyRangeHigh,loop_start:zone.loopStart>1&&zone.loopStart<zone.loopEnd?zone.loopStart/zoneRate:null,loop_end:zone.loopStart>1&&zone.loopStart<zone.loopEnd?zone.loopEnd/zoneRate:null});
      }
    }else{
      const {bank,base}=await this.find(name);const groups=typeof bank==='string'?[[36,[bank]]]:Array.isArray(bank)?[[36,bank]]:Object.entries(bank).map(([note,urls])=>[this.note(note),Array.isArray(urls)?urls:[urls]]);
      if(groups.length>128)throw Error(`Sample bank too large: ${name}`);
      for(const [root,urls] of groups){if(!urls.length)continue;const variant=this.index(n,urls.length),url=this.url(urls[variant],base),decoded=await this.decoded(url,sampleRate);const id=this.install(engine,`${sampleRate}:${url}`,decoded);
        entries.push({id,variant,variant_count:urls.length,root,duration:decoded.pcm.length/decoded.channels/decoded.sampleRate});}
    }
    if(!entries.length)throw Error(`No decodable samples for ${name}`);
    let registration=this.registrations?.get(engine);if(!registration){this.registrations||=new WeakMap();registration=new Map();this.registrations.set(engine,registration);}
    const previous=registration.get(name)||[];const variants=new Set(entries.map(e=>e.variant));entries=[...previous.filter(e=>!variants.has(e.variant)),...entries];registration.set(name,entries);engine.register_bank(name,JSON.stringify(entries));
  }
}
