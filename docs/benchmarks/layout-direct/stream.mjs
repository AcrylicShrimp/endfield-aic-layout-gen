import fs from 'node:fs';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { args,read,write,hash,digest,sourcePaths,catalogKeys,Probe } from '../layout-rl/io.mjs';
import { prepareRoot,checkRoot } from '../layout-rl/coverage.mjs';
import { sampleRequest,validateSampling } from './sampling.mjs';

const diagnostic=(code,message,entity=null,stage='stream')=>({code:`stream-${code}`,severity:'warning',stage,message,entity});
const run=(binary,argv,timeout)=>new Promise((resolve,reject)=>{
  const began=performance.now();
  execFile(binary,argv,{timeout,maxBuffer:64*1024*1024,killSignal:'SIGKILL'},(error,stdout,stderr)=>{
    if(error&&typeof error.code!=='number'&&!error.killed)return reject(error);
    resolve({code:error?.code??0,timed_out:error?.killed===true,stdout,stderr,elapsed_ms:performance.now()-began});
  });
});

export class FactoryStream {
  constructor(filename,output){
    const planPath=path.resolve(filename),plan=read(planPath),resolve=p=>path.resolve(path.dirname(planPath),p);
    if(plan.schema_version!==1)throw new Error('Unsupported factory stream plan');validateSampling(plan.sampling);
    const paths=Object.fromEntries(catalogKeys.map(k=>[k,resolve(plan.catalogs[k])]));
    this.plan=plan;this.paths=paths;this.catalogs=Object.fromEntries(catalogKeys.map(k=>[k,read(paths[k])]));this.cli=resolve(plan.cli);this.validator=resolve(plan.validator);
    for(const k of ['flow_elapsed_ms','validator_ms'])if(!Number.isSafeInteger(plan[k])||plan[k]<1||plan[k]>30000)throw new Error(`Invalid ${k}`);
    if(!Number.isSafeInteger(plan.construction?.elapsed_ms)||plan.construction.elapsed_ms<1||plan.construction.elapsed_ms>300000)throw new Error('Invalid construction deadline');
    const inputs=new Set([planPath,...Object.values(paths),this.cli,this.validator,...sourcePaths(fileURLToPath(import.meta.url))]);
    this.provenance=[...inputs].map(p=>({path:p,sha256:hash(p)}));this.output=path.resolve(output);fs.mkdirSync(this.output);write(path.join(this.output,'provenance.json'),this.provenance);
    this.probe=new Probe({...paths,validator:this.validator},plan.validator_ms);this.attempts=[];this.seen=new Map();
  }
  async attempt(position){
    const started=performance.now(),directory=path.join(this.output,`attempt-${String(this.attempts.length).padStart(7,'0')}`);fs.mkdirSync(directory);
    const row={...position,directory,status:'pending',factory_validated:false};let root=null;
    try{
      const sample=sampleRequest(this.catalogs,this.plan.sampling,position);row.sample=sample;
      const requestPath=path.join(directory,'source-plan.json');write(requestPath,sample.request);write(path.join(directory,'sampling.json'),sample);
      const flow=await run(this.cli,['recipes','contextual-instance-wiring','--file',this.paths.recipes,'--request',requestPath],this.plan.flow_elapsed_ms);
      write(path.join(directory,'flow-process.json'),flow);row.flow_ms=flow.elapsed_ms;
      if(flow.timed_out){row.status='flow-budget';row.diagnostic=diagnostic('flow-budget','Material-flow calculation exceeded its safety deadline');}
      else{
        const wiring=JSON.parse(flow.stdout);write(path.join(directory,'wiring.json'),wiring);
        if(flow.code!==0||!wiring.success){
          row.status='flow-rejected';row.diagnostics=wiring.diagnostics;
          const explanation=await run(this.cli,['recipes','contextual-throughput','--file',this.paths.recipes,'--request',requestPath],this.plan.flow_elapsed_ms);
          write(path.join(directory,'throughput-process.json'),explanation);row.flow_ms+=explanation.elapsed_ms;
          if(!explanation.timed_out){const throughput=JSON.parse(explanation.stdout);write(path.join(directory,'throughput.json'),throughput);row.diagnostics=throughput.diagnostics;}
        }
        else{
          const nodes=wiring.nodes.filter(n=>n.kind==='facility'),ids=new Set(nodes.map(n=>n.id));
          row.facilities=nodes.length;row.requirements=wiring.edges.length;
          row.transports=[...new Set(wiring.edges.map(e=>this.catalogs.items.items.find(i=>i.id===e.item).transport))].sort();
          const graphRequest={placements:nodes.map(n=>({instance:n.id,recipe:n.recipe,facility:n.facility})),
            internal_requirements:wiring.edges.filter(e=>ids.has(e.source)&&ids.has(e.target)),external_requirements:wiring.edges.filter(e=>!ids.has(e.source)||!ids.has(e.target))};
          row.production_sha256=digest({target:sample.request.target,graph:graphRequest});
          if(nodes.length>this.plan.sampling.max_facilities){row.status='instance-guard';row.diagnostic=diagnostic('instance-guard','Whole generated factory exceeds declared operational instance guard',nodes.length);}
          else if(this.seen.has(row.production_sha256)){row.status='duplicate-production';row.duplicate_of=this.seen.get(row.production_sha256);}
          else{
            this.seen.set(row.production_sha256,{...position,directory});
            const target=wiring.edges.find(e=>wiring.nodes.find(n=>n.id===e.target)?.kind==='target')?.source;
            if(!ids.has(target))throw new Error('Successful wiring lacks a facility supplying the actual target');
            const reportPath=path.join(directory,'construction.json'),c=this.plan.construction;
            const construction=await run(this.cli,['layouts','construct-production','--recipes',this.paths.recipes,'--source-plan',requestPath,
              '--facility-catalog',this.paths.facilities,'--item-catalog',this.paths.items,'--transport-catalog',this.paths.transports,
              '--logistics-component-catalog',this.paths.components,'--target-instance',target,'--max-steps',String(c.max_steps),
              '--max-placement-attempts',String(c.max_placement_attempts),'--max-route-searches',String(c.max_route_searches),
              '--analysis-workers',String(c.analysis_workers),'--downstream-closed-growth',
              '--report-output',reportPath,'--visualization-output',path.join(directory,'construction.html')],c.elapsed_ms);
            write(path.join(directory,'construction-process.json'),construction);row.construction_ms=construction.elapsed_ms;
            if(construction.timed_out){row.status='construction-budget';row.diagnostic=diagnostic('construction-budget','Initial factory construction exceeded its safety deadline');}
            else{
              const report=read(reportPath);row.construction_status=report.status;
              if(construction.code!==0||!report.complete||!report.success){row.status='construction-stopped';row.diagnostics=report.diagnostics;}
              else{
                const n=report.final_node,witness={schema_version:1,placements:n.placements,transport_networks:n.transport_networks,logistics_components:n.logistics_components,
                  utilizations:Object.fromEntries(nodes.map(n=>[n.id,n.work_seconds_per_second]))};
                root={id:`stream-${digest(position).slice(0,16)}`,split:position.split,generation:'dynamic-catalog',family:sample.request.target.item,dialect:'closure-factory',request:graphRequest,witness};
                root.graph=prepareRoot(root,this.catalogs);const start=performance.now(),validation=await this.probe.check(witness),coverage=checkRoot(root,root.graph,witness,validation);
                row.validation_ms=performance.now()-start;write(path.join(directory,'validation.json'),{validation,coverage});
                if(!coverage.factory_validated){row.status='validation-rejected';row.diagnostics=coverage.diagnostics;root=null;}
                else{row.status='accepted';row.factory_validated=true;write(path.join(directory,'root.json'),root);row.root={path:path.join(directory,'root.json'),sha256:hash(path.join(directory,'root.json'))};}
              }
            }
          }
        }
      }
    }catch(e){if(!e.code?.startsWith('stream-'))throw e;row.status='sampling-rejected';row.diagnostic={code:e.code,severity:e.severity,stage:e.stage,message:e.message,entity:e.entity};}
    row.elapsed_ms=performance.now()-started;this.attempts.push(row);write(path.join(directory,'attempt.json'),row);return {root,row};
  }
  close(){this.probe.close();for(const p of this.provenance)if(hash(p.path)!==p.sha256)throw new Error(`Factory stream source changed: ${p.path}`);}
}

if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  let stream;
  try{
    const o=args(['plan','seed','split','attempts','output-directory']),count=Number(o.attempts);
    if(!Number.isSafeInteger(count)||count<1||count>1000||o.split==='test')throw new Error('Expected 1..1000 development attempts; sealed test is not available in this CLI');
    stream=new FactoryStream(o.plan,o['output-directory']);
    for(let ordinal=0;ordinal<count;ordinal++){const {row}=await stream.attempt({seed:o.seed,ordinal,split:o.split});console.log(JSON.stringify({ordinal,status:row.status,facilities:row.facilities,target:row.sample?.request.target.item,elapsed_ms:row.elapsed_ms}));}
    write(path.join(stream.output,'report.json'),{schema_version:1,status:'measured',scope:'dynamic-complete-factory-generation',attempts:stream.attempts,provenance:stream.provenance});
  }catch(e){console.error(JSON.stringify(diagnostic('generation-failed',e.message,e.entity??null)));process.exitCode=1;}
  finally{stream?.close();}
}
