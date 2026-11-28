// Shared published score catalogue; validate the bytes before editing/evaluation.
export const DEFAULT_DEMO='Bachata bruma de canela.strudel';
export async function demoCatalog(){
  const response=await fetch('./javascript-demos.json');
  if(!response.ok)throw Error('Demo catalogue could not load');
  return response.json();
}
// Resolve catalogue identities only; a demo query must never become a fetch URL.
export function resolveDemo(entries,name){
  const key=String(name??'').trim().replace(/\.strudel$/i,'').toLowerCase();
  return entries.find(entry=>entry.name.replace(/\.strudel$/i,'').toLowerCase()===key);
}
export async function demoSource(entry){
  const response=await fetch(entry.url);
  if(!response.ok)throw Error(`Could not load ${entry.name}`);
  const bytes=await response.arrayBuffer();
  if(bytes.byteLength>256*1024)throw Error('Demo exceeds 256 KiB');
  const digest=await crypto.subtle.digest('SHA-256',bytes);
  const hash=Array.from(new Uint8Array(digest),v=>v.toString(16).padStart(2,'0')).join('');
  if(hash!==entry.sha256)throw Error(`Demo source changed: ${entry.name}`);
  return new TextDecoder().decode(bytes);
}
export function demoWorkspace(state,entry,source,reuse=false){
  const next=JSON.parse(state);
  if(reuse){
    const existing=next.documents.findIndex(document=>document.source===source);
    if(existing>=0){next.current=existing;return JSON.stringify(next);}
  }
  if(next.documents.length>=32)throw Error('Studio has 32 scenes; close one before loading another demo');
  const stem=entry.name.replace(/\.strudel$/i,'');let name=entry.name,index=2;
  while(next.documents.some(document=>document.name===name))name=`${stem} (${index++}).strudel`;
  next.documents.push({name,source});next.current=next.documents.length-1;
  const serialized=JSON.stringify(next);
  if(new TextEncoder().encode(serialized).length>2*1024*1024)throw Error('Studio workspace exceeds 2 MiB; close a scene before loading this demo');
  return serialized;
}
