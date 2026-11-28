const assert=require('node:assert/strict');
const fs=require('node:fs');const path=require('node:path');
const out=path.resolve(__dirname,'../target/rustel-browser-studio-test');
const version=fs.readFileSync(path.join(out,'version.txt'),'utf8').trim();
const {Studio,memory_bytes}=require(path.join(out,version,'rustel_browser_studio.js'));
const source='$: note("c3 e3 g3").s("sine")._pianoroll()\n';
const studio=new Studio(source);
const screen=()=>JSON.parse(studio.frame(100,35));
const text=()=>screen().cells.map(c=>c[0]).join('');
assert.match(text(),/File.*Edit.*Scene.*Transport/);
studio.key('a',2);studio.paste('$: note("d4").s("sine")\n');
assert.equal(studio.source(),'$: note("d4").s("sine")\n');
studio.key('z',2);assert.equal(studio.source(),source);
studio.play();const commands=JSON.parse(studio.commands());const evaluation=commands.find(c=>c.type==='evaluate');
assert.ok(evaluation,'real App emitted an evaluation');assert.equal(evaluation.source,source);
studio.evaluated(evaluation.request,evaluation.revision,source,1);
studio.transport(true,1,1,.5);assert.match(text(),/PLAYING/);
studio.events(JSON.stringify([{id:1,cycle:1,duration:1,pitch:60,gain:.2,sound:'sine',lane:'#0',ui_visuals:'1',context:[]}]),1,2,.5);
assert.ok(screen().cells.length===3500);
const state=studio.state();const restored=new Studio('',state);assert.equal(restored.source(),source);restored.free();
studio.stop();assert.ok(JSON.parse(studio.commands()).some(c=>c.type==='stop'));
studio.transport(false,1,1,.5);assert.match(text(),/STOPPED/);
studio.mouse('down',2,0,0);studio.mouse('up',2,0,0);assert.match(text(),/set/i);
studio.key('Escape',0);for(let i=0;i<20;i++)screen();const memory=memory_bytes();for(let i=0;i<300;i++)screen();assert(memory_bytes()-memory<=1024*1024,'Idle frames keep growing WASM memory');
const clipboard=new Studio(source);clipboard.key('a',2);clipboard.key('v',2);
assert.equal(clipboard.source(),source,'deferred clipboard read must not delete the selection or paste cached text');
assert(JSON.parse(clipboard.commands()).some(c=>c.type==='clipboard-read'));
const pasted='first line\n🎵 second line\n';clipboard.paste(pasted);assert.equal(clipboard.source(),pasted);
clipboard.key('a',2);clipboard.key('c',2);const copied=JSON.parse(clipboard.commands()).find(c=>c.type==='clipboard-write');
assert.equal(copied.text,pasted);assert.equal(clipboard.clipboard(),pasted);assert(!JSON.parse(clipboard.commands()).some(c=>c.type==='clipboard-read'));clipboard.free();
const bachataSource=fs.readFileSync(path.resolve(__dirname,'../demos/Bachata.strudel'),'utf8');
const bachata=new Studio(bachataSource);bachata.play();assert(JSON.parse(bachata.commands()).some(c=>c.type==='evaluate'),'Unmodified Bachata must pass Studio preflight');bachata.free();
for(const newline of ['\n','\r\n','\u2028','\u2029']){
 const continued=new Studio('note("c4 '+String.fromCharCode(92)+newline+' e4").s("sine")');continued.play();assert(JSON.parse(continued.commands()).some(c=>c.type==='evaluate'),'JS line continuation was rejected');continued.free();
 const joined=new Studio('note("c'+String.fromCharCode(92)+newline+'4").s("sine")');joined.play();assert(JSON.parse(joined.commands()).some(c=>c.type==='evaluate'),'Continued token must stay joined');joined.free();
}
const invalid=new Studio('note("c4 '+String.fromCharCode(92)+'\r\n z9").s("sine")');invalid.play();assert(!JSON.parse(invalid.commands()).some(c=>c.type==='evaluate'));
const diagnostic=JSON.parse(invalid.frame(150,15)).cells.slice(-300,-150).map(c=>c[0]).join('');assert.match(diagnostic,/2:2.*not a note: "z9"/);invalid.free();
studio.toggle_zen();assert.equal(screen().zen,true);studio.toggle_zen();assert.equal(screen().zen,false);assert.match(text(),/File.*Edit/);
studio.free();console.log('PASS: real Studio editor, undo, menus, evaluate/stop, visuals and workspace restore');

for(const name of fs.readdirSync(path.resolve(__dirname,'../demos')).filter(name=>name.endsWith('.strudel'))){
 const demo=new Studio(fs.readFileSync(path.resolve(__dirname,'../demos',name),'utf8'));demo.play();
 assert(JSON.parse(demo.commands()).some(command=>command.type==='evaluate'),`Studio rejected demo: ${name}`);demo.free();
}
console.log('PASS: all bundled demos pass Studio preflight');
