import {spawn} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {args,write,hash} from '../layout-rl/io.mjs';

const options=args(['chrome','theme-css','directory']),directory=path.resolve(options.directory);
const chrome=spawn(options.chrome,['--no-sandbox','--disable-gpu','--remote-debugging-port=0'],{stdio:['ignore','ignore','pipe']});
const guard=setTimeout(()=>chrome.kill(),45000);let ws;
try{
  const url=await new Promise((resolve,reject)=>{let s='';chrome.stderr.on('data',c=>{s+=c;const m=s.match(/DevTools listening on (ws:\/\/\S+)/);if(m)resolve(m[1]);});chrome.on('error',reject);chrome.on('exit',()=>reject(Error('Chrome exited')));});
  ws=new WebSocket(url);await new Promise((resolve,reject)=>{ws.addEventListener('open',resolve);ws.addEventListener('error',reject);});
  let seq=0;const pending=new Map();
  ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}});
  ws.addEventListener('close',()=>{for(const p of pending.values())p.reject(Error('Browser connection closed before the check completed'));pending.clear();});
  const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const id=++seq;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params,...(sessionId?{sessionId}:{})}));});
  const {targetId}=await call('Target.createTarget',{url:'about:blank'}),{sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});
  const c=(method,params)=>call(method,params,sessionId);
  const evaluate=async expression=>{const r=await c('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
  await c('Page.enable');const {frameTree}=await c('Page.getFrameTree');
  const fragmentPath=path.join(directory,'cumulative-layouts.html'),fragment=fs.readFileSync(fragmentPath,'utf8');
  assert.ok(!fragment.includes('__STREAM_VISUAL_DATA__'));assert.ok(!fragment.includes('<!doctype'));
  assert.ok(fragment.includes('const bounds=c.display_bounds,scale='));
  const html=`<!doctype html><html data-visualize-standalone><head><style>${fs.readFileSync(options['theme-css'],'utf8')}body{margin:0;padding:12px;box-sizing:border-box}</style></head><body>${fragment}</body></html>`,rows=[];
  for(const theme of ['light','dark'])for(const width of [360,736,1024]){
    await c('Emulation.setDeviceMetricsOverride',{width,height:1100,deviceScaleFactor:1,mobile:false});
    await c('Emulation.setEmulatedMedia',{features:[{name:'prefers-color-scheme',value:theme}]});
    await c('Page.setDocumentContent',{frameId:frameTree.frame.id,html});
    await evaluate('new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))');
    const count=await evaluate("JSON.parse(document.getElementById('budget-visual-data').textContent).cases.length"),scales=new Map();
    for(let index=0;index<count;index++){
      await evaluate(`{const p=document.getElementById('budget-case');p.value=${index};p.dispatchEvent(new Event('change'));}`);
      const value=await evaluate(`({scroll:document.documentElement.scrollWidth,facilities:document.querySelectorAll('[data-facility]').length,networks:document.querySelectorAll('[data-network]').length,metric:document.getElementById('budget-trained-score').textContent,budget:document.getElementById('budget-budget').textContent,labels:[...document.querySelectorAll('svg text')].map(e=>e.getBoundingClientRect().height),foreground:getComputedStyle(document.getElementById('budget-policy-comparison')).getPropertyValue('--foreground'),maps:[...document.querySelectorAll('.factory-map')].map(e=>({width:e.getBoundingClientRect().width,height:e.getBoundingClientRect().height,firstFacilityWidth:e.querySelector('[data-facility]').getBoundingClientRect().width}))})`);
      const expected=await evaluate(`(()=>{const c=JSON.parse(document.getElementById('budget-visual-data').textContent).cases[${index}];return {root:c.root,checkpoint:c.checkpoint,facilities:c.trained.facilities.length,area:c.trained.score.area,bounds:c.display_bounds,sizes:[c.untrained.facilities[0].width,c.trained.facilities[0].width]};})()`);
      assert.ok(value.scroll<=width);assert.equal(value.facilities,2*expected.facilities);assert.ok(value.networks>0);assert.ok(value.metric.includes(`area ${expected.area}`));assert.ok(value.budget.includes(`${expected.checkpoint} cumulative updates`));assert.ok(value.labels.every(h=>h>=11));assert.ok(value.foreground.trim());
      const pairScales=value.maps.map((m,i)=>(m.firstFacilityWidth+1)/expected.sizes[i]);
      assert.ok(Math.abs(pairScales[0]-pairScales[1])<.02);
      if(scales.has(expected.root))assert.ok(Math.abs(pairScales[0]-scales.get(expected.root))<.02);else scales.set(expected.root,pairScales[0]);
      await evaluate(`document.querySelector('[data-facility="F01"]').dispatchEvent(new Event('click'))`);
      assert.ok((await evaluate(`document.getElementById('budget-detail').textContent`)).startsWith('F01'));
      await evaluate(`{const p=document.getElementById('budget-facility');p.value='';p.dispatchEvent(new Event('change'));}`);
      if(width===1024){
        const metrics=await c('Page.getLayoutMetrics'),size=metrics.cssContentSize;
        const shot=await c('Page.captureScreenshot',{format:'png',captureBeyondViewport:true,clip:{x:0,y:0,width:size.width,height:size.height,scale:1}});
        fs.writeFileSync(path.join(directory,`${theme}-${width}-case-${index}.png`),Buffer.from(shot.data,'base64'));
      }
      rows.push({theme,width,index,root:expected.root,checkpoint:expected.checkpoint,scale:pairScales[0],...value});
    }
  }
  write(path.join(directory,'browser-check.json'),{status:'complete',rows,fragment:{path:fragmentPath,sha256:hash(fragmentPath)}});
  console.log(JSON.stringify({status:'complete',checks:rows.length}));
}catch(e){console.error(JSON.stringify({code:'budget-curve-browser-failed',severity:'error',stage:'cumulative-layout-browser-check',message:e.message,entity:directory}));process.exitCode=1;}
finally{clearTimeout(guard);ws?.close();chrome.kill();}
