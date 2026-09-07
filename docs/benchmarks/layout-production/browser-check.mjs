import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { args, write, hash } from '../layout-rl/io.mjs';

const options=args(['chrome','theme-css','directory']),directory=path.resolve(options.directory);
const chrome=spawn(options.chrome,['--no-sandbox','--disable-gpu','--remote-debugging-port=0'],{stdio:['ignore','ignore','pipe']});
const guard=setTimeout(()=>chrome.kill(),30000);let ws;
try {
  const url=await new Promise((resolve,reject)=>{let s='';chrome.stderr.on('data',c=>{s+=c;const m=s.match(/DevTools listening on (ws:\/\/\S+)/);if(m)resolve(m[1]);});chrome.on('error',reject);chrome.on('exit',()=>reject(Error('Chrome exited')));});
  ws=new WebSocket(url);await new Promise((resolve,reject)=>{ws.addEventListener('open',resolve);ws.addEventListener('error',reject);});
  let seq=0;const pending=new Map();
  ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}});
  const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const id=++seq;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params,...(sessionId?{sessionId}:{})}));});
  const {targetId}=await call('Target.createTarget',{url:'about:blank'}),{sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});
  const c=(method,params)=>call(method,params,sessionId);
  const evaluate=async expression=>{const r=await c('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
  await c('Page.enable');const {frameTree}=await c('Page.getFrameTree');
  const fragmentPath=path.join(directory,'heavy-xiranite-audit.html');
  const css=fs.readFileSync(options['theme-css'],'utf8'),fragment=fs.readFileSync(fragmentPath,'utf8');
  const html=`<!doctype html><html data-visualize-standalone><head><style>${css}body{margin:0;padding:12px;box-sizing:border-box}</style></head><body>${fragment}</body></html>`;
  const rows=[];
  for(const theme of ['light','dark'])for(const width of [360,736]){
    await c('Emulation.setDeviceMetricsOverride',{width,height:1100,deviceScaleFactor:1,mobile:false});
    await c('Emulation.setEmulatedMedia',{features:[{name:'prefers-color-scheme',value:theme}]});
    await c('Page.setDocumentContent',{frameId:frameTree.frame.id,html});
    await evaluate('new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))');
    const value=await evaluate(`({scroll:document.documentElement.scrollWidth,facilities:document.querySelectorAll('[data-facility]').length,networks:document.querySelectorAll('[data-network]').length,metric:document.getElementById('production-density').textContent,labels:[...document.querySelectorAll('svg text')].map(e=>e.getBoundingClientRect().height),foreground:getComputedStyle(document.getElementById('production-layout-audit')).getPropertyValue('--foreground')})`);
    assert.ok(value.scroll<=width);assert.equal(value.facilities,59);assert.ok(value.networks>300);assert.ok(value.metric.includes('29.69%'));assert.ok(value.labels.every(h=>h>=11));assert.ok(value.foreground.trim());
    await evaluate(`document.querySelector('[data-facility="F01"]').dispatchEvent(new Event('click'))`);
    assert.ok((await evaluate(`document.getElementById('production-detail').textContent`)).startsWith('F01'));
    await evaluate(`document.querySelector('[data-network]').dispatchEvent(new Event('click'))`);
    assert.ok((await evaluate(`document.getElementById('production-detail').textContent`)).includes('terminals'));
    // Reload the neutral state for direct geometry inspection.
    await c('Page.setDocumentContent',{frameId:frameTree.frame.id,html});
    await evaluate('new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))');
    const shot=await c('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});
    fs.writeFileSync(path.join(directory,`${theme}-${width}.png`),Buffer.from(shot.data,'base64'));
    rows.push({theme,width,...value});
  }
  write(path.join(directory,'browser-check.json'),{status:'complete',rows,fragment:{path:fragmentPath,sha256:hash(fragmentPath)}});
  console.log(JSON.stringify({status:'complete',checks:rows.length}));
}finally{clearTimeout(guard);ws?.close();chrome.kill();}
