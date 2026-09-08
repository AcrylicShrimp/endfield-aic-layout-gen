// Explicit diagnostic: combine one selected translation with obstructing bridge release.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,Probe,sourcePaths} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {extractChains,reembed} from '../../docs/benchmarks/constructive-local-reembedding/model.mjs';
import {releaseBridgeDraft} from '../../docs/benchmarks/constructive-local-reembedding/components.mjs';
import {bodyCells} from '../../docs/benchmarks/constructive-local-reembedding/movement.mjs';
import {measure} from './full_factory_audit.mjs';
const key=p=>`${p.x},${p.y}`;
async function main(o){
  const inputs=read(o.inputs),c=inputs.rows.find(c=>c.id===o.case),w=read(o.initial),members=o.members.split(',').map(Number),dx=Number(o.dx),dy=Number(o.dy),portAssignments=o.ports?read(o.ports):[];
  assert(members.length&&members.length<=16&&new Set(members).size===members.length&&members.every(i=>Number.isSafeInteger(i)&&i>=1&&i<=w.placements.length));
  assert(Number.isSafeInteger(dx)&&Number.isSafeInteger(dy)&&(dx||dy));
  const paths=read(inputs.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,p])=>[k,read(p)]));
  const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:w},prepared=prepareRoot(root,catalogs),probe=new Probe({...paths,validator:path.resolve(o.validator)});
  const selected=new Set(members.map(i=>w.placements[i-1].instance)),body=new Set(w.placements.filter(p=>selected.has(p.instance)).flatMap(p=>bodyCells({...p,x:p.x+dx,y:p.y+dy})).map(key));
  const chains=extractChains(w).chains,blocked=chains.filter(c=>c.cells.some(p=>body.has(key(p)))),obstructing=w.logistics_components.filter(c=>c.kind==='bridge'&&blocked.some(j=>j.transport===c.transport&&j.cells.some(p=>key(p)===key(c.position))));
  // One neighboring bridge can reserve the released center as its immutable arm.
  const bridges=w.logistics_components.filter(c=>c.kind==='bridge'&&obstructing.some(b=>b.transport===c.transport&&Math.abs(b.position.x-c.position.x)+Math.abs(b.position.y-c.position.y)<=1)).map(c=>c.id);
  assert(bridges.length<=8);const sets=[[],...bridges.map(id=>[id]),...(bridges.length>1?[bridges]:[])];
  fs.mkdirSync(o.output);const rows=[];
  try{
    assert(checkRoot(root,prepared,w,await probe.check(w)).factory_validated);
    write(path.join(o.output,'protocol.json'),{schema_version:2,scope:'One translation with none/single/all body-obstructing and one-hop neighboring bridge release; near/far pipe-first repair; diagnostic actions are not ordinary policy vocabulary',
      initial:path.resolve(o.initial),initial_sha256:hash(o.initial),original:c.original,original_sha256:hash(c.original),members,dx,dy,bridges,portAssignments,work_per_attempt:5000,max_new_crossings:2,
      source_sha256:Object.fromEntries([...sourcePaths(fileURLToPath(import.meta.url))].map(p=>[p,hash(p)]))});
    for(const releaseBridges of sets)for(const order of ['near','far']){
      let expanded=0;const draft=releaseBridgeDraft(w,catalogs,releaseBridges),cs=extractChains(draft.witness,draft.released).chains;
      const options={moves:[{instances:[...selected],dx,dy}],releaseBridges,portAssignments,order,endpointPolicy:'terminal-flexible',junctionPolicy:'internal',maxNewCrossings:2,
        routePriority:cs.sort((a,b)=>Number(a.transport!=='pipe')-Number(b.transport!=='pipe')||a.id-b.id).map(c=>c.id),
        workBudget:5000,totalWorkBudget:5000,onRouteResult:r=>{expanded+=r.expanded;}};
      let result;
      try{result=reembed({witness:w,labels:{}},catalogs,options);}catch(e){result={status:'executor_rejected',diagnostics:[{code:e.code,message:e.message,entity:e.entity}]};}
      const row={selected:rows.length+1,releaseBridges,portAssignments,order,status:result.status,expanded,diagnostics:result.diagnostics??[]};
      if(result.status==='candidate'){
        row.acceptance=checkRoot(root,prepared,result.detail.witness,await probe.check(result.detail.witness));
        if(row.acceptance.factory_validated){row.metrics=measure(result.detail.witness,catalogs);write(path.join(o.output,`candidate-${row.selected}.json`),result.detail.witness);}
      }
      rows.push(row);console.log(JSON.stringify(row));
    }
    write(path.join(o.output,'probe.json'),{status:'complete',initial:measure(w,catalogs),rows});
  }finally{probe.close();}
}
main(args(['inputs','initial','case','members','dx','dy','validator','output'],['ports'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
