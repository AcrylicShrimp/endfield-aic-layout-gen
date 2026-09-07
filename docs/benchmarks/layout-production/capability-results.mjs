import path from 'node:path';
import { args,read,write,hash } from '../layout-rl/io.mjs';

const options=args(['pilot','strong','output']);
const inputs=Object.fromEntries(['pilot','strong'].map(k=>[k,path.resolve(options[k])]));
const experiments={};
for(const [name,p]of Object.entries(inputs)){
  const report=read(p);
  if(report.status!=='measured'||report.test_evaluations!==0)throw new Error(`Unexpected capability experiment: ${p}`);
  for(const source of report.provenance)if(hash(source.path)!==source.sha256)throw new Error(`Changed experiment source: ${source.path}`);
  for(const row of report.rows)for(const a of row.artifacts)if(hash(a.path)!==a.sha256)throw new Error(`Changed capability artifact: ${a.path}`);
  experiments[name]={report:{path:p,sha256:hash(p)},provenance:report.provenance,rows:report.rows.map(r=>({
    root:r.root,level:r.level,initial:r.initial,best:r.best,empty_fraction:r.best_density.empty_fraction,
    domain_size:r.domain_size,visited:r.visited,enumeration_complete:r.enumeration_complete,
    counts:r.counts,diagnostics:r.diagnostics,unique_valid_states:r.unique_valid_states,work:r.work,budget:r.budget,
    elapsed_ms:r.elapsed_ms,termination:r.termination,factory_validated:r.factory_validated,artifacts:r.artifacts
  }))};
}
write(path.resolve(options.output),{schema_version:1,status:'capability-measured-not-ready',scope:'one-step-development-domains',
  test_evaluations:0,no_global_impossibility_claim:true,experiments});
console.log(JSON.stringify({status:'archived',experiments:Object.keys(experiments),output:path.resolve(options.output)}));
