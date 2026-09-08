// A bounded cooperative-port diagnostic for one explicit complete-factory move.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,Probe,sourcePaths} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {extractChains,reembed} from '../../docs/benchmarks/constructive-local-reembedding/model.mjs';
import {releaseBridgeDraft} from '../../docs/benchmarks/constructive-local-reembedding/components.mjs';
import {cooperativePortCandidates} from '../../docs/benchmarks/constructive-local-reembedding/cooperative.mjs';
import {measure} from './full_factory_audit.mjs';
const distance=c=>Math.abs(c.cells[0].x-c.cells.at(-1).x)+Math.abs(c.cells[0].y-c.cells.at(-1).y);
async function main(o){
  const began=performance.now(),inputs=read(o.inputs),c=inputs.rows.find(c=>c.id===o.case),w=read(o.initial),spec=read(o.spec);
  const paths=read(inputs.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,p])=>[k,read(p)]));
  assert(Number.isSafeInteger(spec.dx)&&Number.isSafeInteger(spec.dy)&&(spec.dx||spec.dy));
  assert(spec.members.length&&spec.members.length<=16&&new Set(spec.members).size===spec.members.length&&spec.members.every(i=>Number.isSafeInteger(i)&&i>=1&&i<=w.placements.length));
  const moves=[{instances:spec.members.map(i=>w.placements[i-1].instance),dx:spec.dx,dy:spec.dy}];
  const generated=cooperativePortCandidates(w,catalogs,moves,spec.terminals,{limit:512,nodeBudget:65536});
  const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:w},prepared=prepareRoot(root,catalogs),probe=new Probe({...paths,validator:path.resolve(o.validator)});
  fs.mkdirSync(o.output);const rows=[];let status='domain_exhausted';
  try{
    assert(checkRoot(root,prepared,w,await probe.check(w)).factory_validated);
    write(path.join(o.output,'protocol.json'),{schema_version:1,scope:'One explicit movement and bridge-release set; at most eight terminals jointly reassigned from catalog domains; 512 route trials, 60 seconds, 5000 expansions per trial; not ordinary policy actions',
      initial:path.resolve(o.initial),initial_sha256:hash(o.initial),original:c.original,original_sha256:hash(c.original),spec,
      generated:{...generated,candidates:generated.candidates.length},source_sha256:Object.fromEntries([...sourcePaths(fileURLToPath(import.meta.url))].map(p=>[p,hash(p)]))});
    const draft=releaseBridgeDraft(w,catalogs,spec.releaseBridges),chains=extractChains(draft.witness,draft.released).chains;
    outer:for(const assignment of generated.candidates)for(const order of ['near','far']){
      if(rows.length>=512){status='trial_limit';break outer;}if(performance.now()-began>=60000){status='time_limit';break outer;}
      let expanded=0;const routePriority=[...chains].sort((a,b)=>Number(a.transport!=='pipe')-Number(b.transport!=='pipe')||(order==='near'?1:-1)*(distance(a)-distance(b))||a.id-b.id).map(c=>c.id);
      const options={moves,releaseBridges:spec.releaseBridges,portAssignments:assignment.portAssignments,order,endpointPolicy:'terminal-flexible',junctionPolicy:'internal',maxNewCrossings:2,
        routePriority,workBudget:5000,totalWorkBudget:5000,onRouteResult:r=>{expanded+=r.expanded;}};
      let result;
      try{result=reembed({witness:w,labels:{}},catalogs,options);}catch(e){result={status:'executor_rejected',diagnostics:[{code:e.code,message:e.message,entity:e.entity}]};}
      const row={selected:rows.length+1,request:{...options,onRouteResult:undefined},status:result.status,expanded,diagnostics:result.diagnostics??[]};
      if(result.status==='candidate'){
        row.acceptance=checkRoot(root,prepared,result.detail.witness,await probe.check(result.detail.witness));
        if(row.acceptance.factory_validated){row.metrics=measure(result.detail.witness,catalogs);write(path.join(o.output,'candidate.json'),result.detail.witness);status='validated_movement';}
      }
      rows.push(row);if(status==='validated_movement')break outer;
    }
    write(path.join(o.output,'probe.json'),{status,initial:measure(w,catalogs),rows,seconds:(performance.now()-began)/1000});
    console.log(JSON.stringify({status,selected:rows.length,generated:generated.candidates.length,seconds:(performance.now()-began)/1000,final:rows.at(-1)}));
  }finally{probe.close();}
}
main(args(['inputs','initial','case','spec','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
