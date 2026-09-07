import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args,read,write,hash } from '../layout-rl/io.mjs';

export function inspectFrontier(wiring,report){
  const ids=new Set(wiring.nodes.filter(n=>n.kind==='facility').map(n=>n.id)),placed=new Set(report.final_node?.member_instances??[]),
    completed=new Set(report.final_node?.internal_requirements??[]),edges=wiring.edges.filter(e=>ids.has(e.source)&&ids.has(e.target));
  const missing=[...ids].filter(id=>!placed.has(id));
  const frontier=edges.filter(e=>!completed.has(e.id)&&(placed.has(e.source)||placed.has(e.target)));
  const seed=report.request.target_instance,component=new Set([seed]),queue=[seed];
  for(let i=0;i<queue.length;i++)for(const e of edges){const other=e.source===queue[i]?e.target:e.target===queue[i]?e.source:null;
    if(other!==null&&!component.has(other)){component.add(other);queue.push(other);}}
  const exactlySeedComponent=component.size===placed.size&&[...component].every(id=>placed.has(id));
  const noFinalSearch=report.failed_statistics.astar_searches===0&&report.failed_placement_attempts===0&&report.attempted_groups_at_failure.length===0;
  return {required_facilities:ids.size,retained_facilities:placed.size,missing_facilities:missing.length,
    unresolved_incident_requirements:frontier.length,seed_component_facilities:component.size,exactly_seed_component:exactlySeedComponent,
    no_final_candidate_or_route_search:noFinalSearch,
    implementation_no_frontier:report.status==='growth-exhausted'&&missing.length>0&&frontier.length===0&&exactlySeedComponent&&noFinalSearch,
    scope:'Current facility-internal-edge frontier only; not a full-factory infeasibility proof.'};
}

const counts=(xs,f)=>xs.reduce((o,x)=>{const k=f(x);o[k]=(o[k]??0)+1;return o;},{});
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  try{
    const o=args(['directory','output']),directory=path.resolve(o.directory),generationPath=path.join(directory,'worker-0/generation-report.json'),generation=read(generationPath);
    if(fs.existsSync(path.join(directory,'report.json'))||generation.attempts.at(-1)?.status==='accepted')throw new Error('Expected an incomplete run ending during request generation');
    const groups=fs.readdirSync(directory).filter(f=>/^group-\d+\.json$/.test(f)).sort().map(f=>read(path.join(directory,f)));
    if(groups.some(g=>g.audits.some(a=>!a.coverage.factory_validated||!a.validation.covered_geometry_validated)))throw new Error('Completed group has a failed audit');
    const inputs=[generationPath],failures=generation.attempts.filter(a=>a.status==='construction-stopped').map(a=>{
      const wiringPath=path.join(a.directory,'wiring.json'),reportPath=path.join(a.directory,'construction.json');inputs.push(wiringPath,reportPath);
      return {ordinal:a.ordinal,requested_stratum:a.requested_stratum,actual_stratum:a.size_stratum,target:a.sample.request.target.item,
        load:a.sample.target_load,topology:a.internal_topology,...inspectFrontier(read(wiringPath),read(reportPath))};
    });
    const result={schema_version:1,status:`required-${generation.attempts.at(-1)?.requested_stratum}-stream-unavailable`,scope:'Stratified dynamic-load training, bounded measured run',
      attempts:generation.attempts.length,statuses:counts(generation.attempts,a=>a.status),
      strata:counts(generation.attempts,a=>a.size_stratum),requested_strata:counts(generation.attempts,a=>a.requested_stratum),
      completed_training_groups:groups.length,executed_training_transitions:groups.reduce((n,g)=>n+g.sampled_transitions,0),
      audited_training_outputs:groups.reduce((n,g)=>n+g.audits.length,0),complete_training_report_present:fs.existsSync(path.join(directory,'report.json')),
      failures,all_stopped_constructions_have_no_frontier:failures.every(f=>f.implementation_no_frontier),
      no_global_impossibility_claim:true,provenance:inputs.map(p=>({path:p,sha256:hash(p)}))};
    write(o.output,result);console.log(JSON.stringify({status:result.status,attempts:result.attempts,confirmed_no_frontier:failures.filter(f=>f.implementation_no_frontier).length,output:path.resolve(o.output)}));
  }catch(e){console.error(JSON.stringify({code:'breadth-analysis-failed',severity:'error',stage:'analysis',message:e.message,entity:null}));process.exitCode=1;}
}
