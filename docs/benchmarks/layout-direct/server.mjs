import fs from 'node:fs';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { args,hash,read,write,sourcePaths } from '../layout-rl/io.mjs';
import { checkRoot } from '../layout-rl/coverage.mjs';
import { FactoryStream } from './stream.mjs';
import { DirectEnvironment } from './environment.mjs';

export async function serve(options){
  const output=path.resolve(options['output-directory']);fs.mkdirSync(output);
  const planPath=path.resolve(options.plan),resolve=p=>path.resolve(path.dirname(planPath),p),stream=new FactoryStream(planPath,path.join(output,'generation'));
  const input=createInterface({input:process.stdin});let episode=0;
  const emit=v=>process.stdout.write(JSON.stringify(v)+'\n');
  try{
    const env=new DirectEnvironment(stream.catalogs,stream.probe,stream.plan.environment);
    const sources=[...sourcePaths(fileURLToPath(import.meta.url)),resolve(stream.plan.target.request),resolve(stream.plan.target.witness)].map(p=>({path:p,sha256:hash(p)}));
    if(hash(resolve(stream.plan.target.witness))!==stream.plan.target.sha256)throw new Error('Changed target witness');
    write(path.join(output,'implementation.json'),sources);
    for await(const line of input){
      let request;
      try{
        request=JSON.parse(line);if(request.schema_version!==1||!Number.isSafeInteger(request.id))throw new Error('Invalid direct request envelope');
        let result;
        if(request.op==='hello')result={schema_version:1,policy_type:'conditional-direct-parameters',dynamic_generation:true,target:'heavy-strong',max_members:stream.plan.environment.max_members};
        else if(request.op==='generate'){
          if(!['train','validation'].includes(request.split))throw new Error('Sealed test generation is not exposed by the training worker');
          result=await stream.attempt({seed:request.seed,ordinal:request.ordinal,split:request.split});
        }else if(request.op==='target')result={id:'heavy-strong',family:'heavy-xiranite-target',split:'target-training',generation:'explicit-target-replay',dialect:'closure-factory',
          request:read(resolve(stream.plan.target.request)),witness:read(resolve(stream.plan.target.witness))};
        else if(request.op==='reset'){result=await env.reset(request.root,request.budget);episode++;}
        else if(request.op==='step')result=await env.step(request.action);
        else if(request.op==='summary')result=env.summary({trace:request.trace===true,witnesses:request.witnesses===true});
        else if(request.op==='save'){
          const p=path.join(output,`episode-${String(episode).padStart(7,'0')}.json`);write(p,env.summary({trace:true,witnesses:true}));result={path:p,sha256:hash(p)};
        }else if(request.op==='audit'){
          const validation=await stream.probe.check(env.best),coverage=checkRoot(env.root,env.root.graph,env.best,validation);
          if(!coverage.factory_validated)throw new Error('Direct incumbent failed fresh independent audit');result={validation,coverage};
        }else throw new Error(`Unknown direct operation: ${request.op}`);
        emit({schema_version:1,id:request.id,result});
      }catch(e){emit({schema_version:1,id:request?.id??null,error:{code:e.code??'direct-server-failed',severity:'error',stage:'direct-environment',message:e.message,entity:e.entity??null}});}
    }
    write(path.join(output,'generation-report.json'),{schema_version:1,attempts:stream.attempts,provenance:stream.provenance});
    for(const p of sources)if(hash(p.path)!==p.sha256)throw new Error(`Direct source changed during worker execution: ${p.path}`);
  }finally{stream.close();input.close();}
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  try{await serve(args(['plan','output-directory']));}
  catch(e){console.error(JSON.stringify({code:'direct-startup-failed',severity:'error',stage:'startup',message:e.message,entity:null}));process.exitCode=1;}
}
