// Native-checked explicit local pose requests; no learned-action or search claim.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,Probe,sourcePaths} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {extractChains,reembed} from '../../docs/benchmarks/constructive-local-reembedding/model.mjs';
import {releaseBridgeDraft} from '../../docs/benchmarks/constructive-local-reembedding/components.mjs';
import {measure} from './full_factory_audit.mjs';
const distance=c=>Math.abs(c.cells[0].x-c.cells.at(-1).x)+Math.abs(c.cells[0].y-c.cells.at(-1).y);
async function main(o){
  const began=performance.now(),inputs=read(o.inputs),c=inputs.rows.find(c=>c.id===o.case),w=read(o.initial),requests=read(o.requests);
  assert(Array.isArray(requests)&&requests.length>0&&requests.length<=512);
  const paths=read(inputs.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,p])=>[k,read(p)]));
  const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:w},prepared=prepareRoot(root,catalogs),probe=new Probe({...paths,validator:path.resolve(o.validator)});
  fs.mkdirSync(o.output);const rows=[];let status='complete';
  try{
    assert(checkRoot(root,prepared,w,await probe.check(w)).factory_validated);
    write(path.join(o.output,'protocol.json'),{schema_version:1,scope:'Explicit finite local requests, 60 seconds total and 5000 shared expansions per request; diagnostic requests are not ordinary policy actions',
      initial:path.resolve(o.initial),initial_sha256:hash(o.initial),original:c.original,original_sha256:hash(c.original),requests:path.resolve(o.requests),requests_sha256:hash(o.requests),
      native_sha256:hash(o.validator),catalog_sha256:Object.fromEntries(Object.entries(paths).map(([k,p])=>[k,hash(p)])),
      source_sha256:Object.fromEntries([...sourcePaths(fileURLToPath(import.meta.url))].map(p=>[p,hash(p)]))});
    for(const request of requests){
      if(performance.now()-began>=60000){status='time_limit';break;}
      let expanded=0,result;const {sort='index',...edit}=request;
      try{
        assert(['index','near','far'].includes(sort));const draft=releaseBridgeDraft(w,catalogs,edit.releaseBridges??[]),chains=extractChains(draft.witness,draft.released).chains;
        const routePriority=chains.sort((a,b)=>Number(a.transport!=='pipe')-Number(b.transport!=='pipe')||
          (sort==='index'?0:(sort==='near'?1:-1)*(distance(a)-distance(b)))||a.id-b.id).map(c=>c.id);
        result=reembed({witness:w,labels:{}},catalogs,{...edit,endpointPolicy:'terminal-flexible',junctionPolicy:'internal',maxNewCrossings:2,routePriority,
          workBudget:5000,totalWorkBudget:5000,onRouteResult:r=>{expanded+=r.expanded;}});
      }catch(e){result={status:'executor_rejected',diagnostics:[{code:e.code??'factory.pose-probe-error',message:e.message,entity:e.entity}]};}
      const row={selected:rows.length+1,request,status:result.status,expanded,diagnostics:result.diagnostics??[]};
      if(result.status==='candidate'){
        row.acceptance=checkRoot(root,prepared,result.detail.witness,await probe.check(result.detail.witness));
        if(row.acceptance.factory_validated){row.metrics=measure(result.detail.witness,catalogs);write(path.join(o.output,`candidate-${row.selected}.json`),result.detail.witness);}
      }
      rows.push(row);
    }
    write(path.join(o.output,'probe.json'),{status,initial:measure(w,catalogs),rows,seconds:(performance.now()-began)/1000});
    console.log(JSON.stringify({status,selected:rows.length,valid:rows.filter(r=>r.acceptance?.factory_validated).length,seconds:(performance.now()-began)/1000}));
  }finally{probe.close();}
}
main(args(['inputs','initial','case','requests','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
