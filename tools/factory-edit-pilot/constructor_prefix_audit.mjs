// Validate committed prefix geometry and internal original-wiring coverage, without full admission.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,Probe,catalogKeys} from '../../docs/benchmarks/layout-rl/io.mjs';

async function main(o){
  const protocol=read(path.join(o.prefix,'protocol.json')),report=read(path.join(o.prefix,'construction.json')),
    wiring=read(path.join(o.attempt,'wiring.json')),paths=read(o.catalogs),
    probe=new Probe({...paths,validator:path.resolve(o.validator)}), rows=[];
  for(const [p,digest] of Object.entries(protocol.source_sha256))assert.equal(hash(p),digest);
  assert.equal(report.required_facilities,wiring.nodes.filter(n=>n.kind==='facility').length);
  const nodes=[...report.steps.map(s=>s.node),report.final_node];
  try{
    for(const [index,n] of nodes.entries()){
      assert(n,`Missing committed node ${index}`);
      const members=new Set(n.member_instances),covered=wiring.nodes.filter(n=>members.has(n.id));
      assert.equal(covered.length,members.size);
      for(const p of n.placements){const original=covered.find(n=>n.id===p.instance);assert(original);assert.equal(p.recipe,original.recipe);assert.equal(p.facility,original.facility);}
      const internal=wiring.edges.filter(e=>members.has(e.source)&&members.has(e.target)).map(e=>e.id).sort();
      assert.deepEqual([...n.internal_requirements].sort(),internal);
      const witness={schema_version:1,placements:n.placements,transport_networks:n.transport_networks,
        logistics_components:n.logistics_components,utilizations:Object.fromEntries(covered.map(n=>[n.id,n.work_seconds_per_second]))};
      const result=await probe.check(witness);assert(result.covered_geometry_validated,JSON.stringify(result.diagnostics));
      rows.push({index,members:members.size,original_internal_requirements:internal.length,covered_geometry_validated:true});
    }
  }finally{probe.close();}
  write(o.output,{schema_version:1,status:'passed',factory_admitted:false,rows,
    scope:'Committed prefix geometry and all original internal edges between covered facilities; full original factory remains incomplete',
    sources:[path.join(o.prefix,'construction.json'),path.join(o.attempt,'wiring.json'),o.validator,o.catalogs,fileURLToPath(import.meta.url),...catalogKeys.map(k=>paths[k])].map(p=>({path:path.resolve(p),sha256:hash(p)}))});
  console.log(JSON.stringify({status:'passed',states:rows.length,final_covered:rows.at(-1).members,factory_admitted:false}));
}
main(args(['prefix','attempt','catalogs','validator','output'])).catch(e=>{console.error(e);process.exitCode=1;});
