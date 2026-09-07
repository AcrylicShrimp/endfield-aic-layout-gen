import fs from 'node:fs';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { args,read,write,hash,sourcePaths,catalogKeys,Probe } from '../layout-rl/io.mjs';
import { checkRoot } from '../layout-rl/coverage.mjs';
import { SequentialEnvironment,tokenFields,actionFields } from './environment.mjs';

export function loadPlan(filename){
  const planPath=path.resolve(filename),plan=read(planPath),resolve=p=>path.resolve(path.dirname(planPath),p);
  if(plan.schema_version!==1)throw new Error(`Unsupported sequential plan: ${planPath}`);
  const datasetPath=resolve(plan.dataset),dataset=read(datasetPath);
  if(dataset.schema_version!==1||!dataset.factory_validated)throw new Error(`Invalid dataset: ${datasetPath}`);
  for(const p of dataset.provenance)if(hash(p.path)!==p.sha256)throw new Error(`Stale root provenance: ${p.path}`);
  const target=dataset.roots.find(r=>r.id===plan.target.root),witnessPath=resolve(plan.target.path);
  if(!target||target.split==='test'||hash(witnessPath)!==plan.target.sha256)throw new Error('Invalid target root or witness provenance');
  const roots=dataset.roots.filter(r=>r.split!=='test');
  if(roots.some(r=>r.id===plan.target.id))throw new Error('Duplicate target identity');
  roots.push({...target,id:plan.target.id,split:'train',generation:'target-training',witness:read(witnessPath)});
  const inputs=new Set([planPath,datasetPath,witnessPath,dataset.options.validator,...dataset.provenance.map(p=>p.path),
    ...catalogKeys.map(k=>dataset.options[k]),...sourcePaths(fileURLToPath(import.meta.url))]);
  return {plan,roots,options:dataset.options,catalogs:Object.fromEntries(catalogKeys.map(k=>[k,read(dataset.options[k])])),
    provenance:[...inputs].map(p=>({path:p,sha256:hash(p)}))};
}

export async function serve(options){
  const loaded=loadPlan(options.roots),output=path.resolve(options['output-directory']);fs.mkdirSync(output);
  write(path.join(output,'provenance.json'),loaded.provenance);
  const probe=new Probe(loaded.options,loaded.plan.environment.validator_ms),env=new SequentialEnvironment(loaded.roots,loaded.catalogs,probe,loaded.plan.environment);
  const input=createInterface({input:process.stdin}),emit=v=>process.stdout.write(JSON.stringify(v)+'\n');let episode=0;
  try{
    for await(const line of input){
      let request;
      try{
        request=JSON.parse(line);if(request.schema_version!==1||!Number.isSafeInteger(request.id))throw new Error('Invalid request envelope');
        let result;
        if(request.op==='hello')result={schema_version:1,roots:loaded.roots.map(({id,split,generation,family})=>({id,split,generation,family})),token_fields:tokenFields,action_fields:actionFields};
        else if(request.op==='reset'){result=await env.reset(request.root,request.budget);episode++;}
        else if(request.op==='step')result=await env.step(request.action);
        else if(request.op==='summary')result=env.summary({trace:request.trace===true,witnesses:request.witnesses===true});
        else if(request.op==='save'){
          const p=path.join(output,`episode-${String(episode).padStart(7,'0')}.json`);write(p,env.summary({trace:true,witnesses:true}));result={path:p,sha256:hash(p)};
        }else if(request.op==='audit'){
          const validation=await probe.check(env.best),coverage=checkRoot(env.root,env.root.graph,env.best,validation);
          if(!coverage.factory_validated)throw new Error('Retained incumbent failed fresh audit');result={validation,coverage};
        }else throw new Error(`Unknown operation: ${request.op}`);
        emit({schema_version:1,id:request.id,result});
      }catch(e){emit({schema_version:1,id:request?.id??null,error:{code:e.code??'sequential-server-failed',severity:'error',stage:'environment',message:e.message,entity:e.entity??null}});}
    }
    for(const p of loaded.provenance)if(hash(p.path)!==p.sha256)throw new Error(`Source changed during execution: ${p.path}`);
  }finally{probe.close();input.close();}
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  try{await serve(args(['roots','output-directory']));}
  catch(e){console.error(JSON.stringify({code:'sequential-startup-failed',severity:'error',stage:'startup',message:e.message,entity:null}));process.exitCode=1;}
}
