import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args,read,write,hash,sourcePaths,catalogKeys,Probe } from '../layout-rl/io.mjs';
import { probeLevel,validateLevel,validateBudget } from './capability-model.mjs';

export async function capability(options) {
  const planPath=path.resolve(options.plan),plan=read(planPath),resolve=p=>path.resolve(path.dirname(planPath),p);
  if(plan.schema_version!==1||!Array.isArray(plan.roots)||!plan.roots.length||!Array.isArray(plan.levels)||!plan.levels.length
    ||plan.roots.length>32||plan.levels.length>64)throw new Error(`Invalid capability plan: ${planPath}`);
  validateBudget(plan.budget);plan.levels.forEach(validateLevel);
  if(new Set(plan.levels.map(l=>l.id)).size!==plan.levels.length||new Set(plan.roots.map(r=>r.id)).size!==plan.roots.length)throw new Error('Duplicate capability root/level identity');
  const datasetPath=resolve(plan.dataset),dataset=read(datasetPath);
  for(const p of dataset.provenance)if(hash(p.path)!==p.sha256)throw new Error(`Changed root provenance: ${p.path}`);
  const catalogs=Object.fromEntries(catalogKeys.map(k=>[k,read(dataset.options[k])]));
  const inputs=new Set([planPath,datasetPath,...dataset.provenance.map(p=>p.path),...catalogKeys.map(k=>dataset.options[k]),dataset.options.validator,...sourcePaths(fileURLToPath(import.meta.url))]);
  const roots=plan.roots.map(selected=>{
    const root=dataset.roots.find(r=>r.id===selected.root);
    if(typeof selected.id!=='string'||!/^[a-z0-9-]+$/.test(selected.id)||!root||root.split==='test')throw new Error(`Unknown, invalid or sealed-test root: ${selected.root}`);
    let witness=root.witness;
    if(selected.witness){const p=resolve(selected.witness.path);if(hash(p)!==selected.witness.sha256)throw new Error(`Changed replacement witness: ${p}`);inputs.add(p);witness=read(p);}
    return {...root,id:selected.id,witness};
  });
  const provenance=[...inputs].map(p=>({path:path.resolve(p),sha256:hash(p)}));
  const output=path.resolve(options['output-directory']);fs.mkdirSync(output);
  const probe=new Probe(dataset.options),rows=[];
  try{
    for(const root of roots)for(const level of plan.levels){
      const result=await probeLevel(root,catalogs,probe,level,plan.budget),directory=path.join(output,`${root.id}--${level.id}`);fs.mkdirSync(directory);
      const {best_witness,trace,...summary}=result;
      write(path.join(directory,'witness.json'),best_witness);write(path.join(directory,'trace.json'),trace);
      summary.artifacts=['witness.json','trace.json'].map(n=>({path:path.join(directory,n),sha256:hash(path.join(directory,n))}));
      write(path.join(directory,'report.json'),summary);rows.push(summary);
      console.log(JSON.stringify({root:root.id,level:level.id,visited:result.visited,domain:result.domain_size,counts:result.counts,best:result.best,elapsed_ms:Math.round(result.elapsed_ms)}));
    }
  }finally{probe.close();}
  for(const p of provenance)if(hash(p.path)!==p.sha256)throw new Error(`Input changed during capability run: ${p.path}`);
  const report={schema_version:1,status:'measured',scope:'finite-one-step-development-capability',test_evaluations:0,plan,rows,provenance};
  write(path.join(output,'report.json'),report);
}

if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  try{await capability(args(['plan','output-directory']));}
  catch(e){console.error(JSON.stringify({code:'ready-capability-failed',severity:'error',stage:'capability-probe',message:e.message,entity:e.path??null}));process.exitCode=1;}
}
