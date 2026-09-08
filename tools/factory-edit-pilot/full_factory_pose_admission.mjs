// Charge every offered compound-pose probe against the original complete factory.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,Probe,sourcePaths} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {transform} from '../../docs/benchmarks/layout-rl/geometry.mjs';
import {describe,execute} from './full_factory_executor.mjs';
import {measure} from './full_factory_audit.mjs';
async function main(o){
  const began=performance.now(),inputs=read(o.inputs),c=inputs.rows.find(c=>c.id===o.case),w=transform(read(o.initial));
  assert(c,`Unknown original case: ${o.case}`);
  const paths=read(inputs.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,p])=>[k,read(p)])),
    root={id:c.id,dialect:c.dialect,request:read(c.original),witness:w},prepared=prepareRoot(root,catalogs),probe=new Probe({...paths,validator:path.resolve(o.validator)});
  fs.mkdirSync(o.output);const rows=[];let status='complete',retained=0;
  try{
    assert(checkRoot(root,prepared,w,await probe.check(w)).factory_validated);
    const d=describe(w,catalogs),actions=d.actions.filter(a=>a.kind==='pose'&&a.poses.length>1&&a.poses.some(q=>q.rotation!==w.placements[q.member].rotation));
    write(path.join(o.output,'protocol.json'),{schema_version:1,executor_schema:d.schema_version,
      scope:'At most 512 offered compound poses from one complete root, 60 seconds total, 5000 expansions each; not learned selections or a full domain search',
      initial:path.resolve(o.initial),initial_sha256:hash(o.initial),original:c.original,original_sha256:hash(c.original),
      native_sha256:hash(o.validator),catalog_sha256:Object.fromEntries(Object.entries(paths).map(([k,p])=>[k,hash(p)])),
      source_sha256:Object.fromEntries([...sourcePaths(fileURLToPath(import.meta.url))].map(p=>[p,hash(p)])),
      vocabulary:d.poses,offered:actions.length,selected_limit:512,actions:actions.slice(0,512)});
    for(const action of actions.slice(0,512)){
      if(performance.now()-began>=60000){status='time_limit';break;}
      const result=execute(w,catalogs,action,5000),row={selected:rows.length+1,action,status:result.status,expanded:result.route_expansions,diagnostics:result.diagnostics??[]};
      assert(result.route_expansions<=5000);assert(!row.diagnostics.some(d=>d.code==='factory.executor-error'));
      if(result.status==='candidate'){
        row.acceptance=checkRoot(root,prepared,result.witness,await probe.check(result.witness));
        if(row.acceptance.factory_validated){
          row.metrics=measure(result.witness,catalogs);
          if(retained<2){row.witness=`candidate-${row.selected}.json`;write(path.join(o.output,row.witness),transform(result.witness));retained++;}
        }
      }
      rows.push(row);
    }
    write(path.join(o.output,'admission.json'),{status,initial:measure(w,catalogs),rows,offered:actions.length,domain_capped:actions.length>512,seconds:(performance.now()-began)/1000});
    console.log(JSON.stringify({status,offered:actions.length,selected:rows.length,valid:rows.filter(r=>r.acceptance?.factory_validated).length,seconds:(performance.now()-began)/1000}));
  }finally{probe.close();}
}
main(args(['inputs','initial','case','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
