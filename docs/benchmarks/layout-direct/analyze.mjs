import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args,read,write,hash } from '../layout-rl/io.mjs';

const counts=(xs,f)=>xs.reduce((out,x)=>{const k=f(x);out[k]=(out[k]??0)+1;return out;},{});
const mean=xs=>xs.length?xs.reduce((a,b)=>a+b,0)/xs.length:null;
const compare=(a,b)=>{for(let i=0;i<a.length;i++)if(a[i]!==b[i])return Math.sign(a[i]-b[i]);return 0;};
export function summarize(r){
  if(r.schema_version!==1||r.status!=='complete'||r.scope!=='dynamic-factory-direct-action-training')throw new Error('Expected a complete streaming run');
  const audits=[...r.evaluation.map(e=>e.audit),...r.training.groups.flatMap(g=>g.audits)];
  if(audits.some(a=>!a.coverage.factory_validated||!a.validation.covered_geometry_validated))throw new Error('Run contains a failed independent audit');
  const generation={};
  for(const split of ['train','validation']){
    const all=r.generation.filter(g=>g.split===split),eligible=all.filter(g=>g.learning_eligible);
    generation[split]={attempts:all.length,statuses:counts(all,g=>g.status),eligible:eligible.length,
      target_families:new Set(eligible.map(g=>g.sample.request.target.item)).size,
      selected_recipes:new Set(eligible.flatMap(g=>g.sample.selected_recipes)).size,
      facility_counts:counts(eligible,g=>g.facilities),transports:counts(eligible,g=>g.transports.join('+')),
      source_cycle_counts:counts(eligible,g=>g.sample.source_cycles),
      costs_ms:Object.fromEntries(['elapsed_ms','flow_ms','construction_ms','validation_ms'].map(k=>[k,all.reduce((sum,g)=>sum+(g[k]??0),0)]))};
  }
  const evaluation=[];
  for(const horizon of r.config.evaluation_horizons)for(const seed of r.config.evaluation_seeds){
    const rows=r.evaluation.filter(e=>e.horizon===horizon&&e.seed===seed&&e.split==='validation'),models={};
    for(const model of ['initial','uniform','trained']){
      const xs=rows.filter(e=>e.model===model);
      models[model]={roots:xs.length,mean_area_ratio:mean(xs.map(e=>e.summary.best_score.area/e.summary.initial_score.area)),
        futile_fraction:mean(xs.map(e=>e.summary.waste.futile_fraction)),mean_route_expansions:mean(xs.map(e=>e.summary.work.route_expansions)),
        mean_episode_seconds:mean(xs.map(e=>e.summary.elapsed_ms/1000)),mean_environment_preprocessing_seconds:mean(xs.map(e=>e.summary.work.preprocess_ms/1000))};
    }
    const comparisons={};
    for(const model of ['initial','uniform'])comparisons[`trained_vs_${model}`]=counts(rows.filter(e=>e.model==='trained'),e=>{
      const control=rows.find(c=>c.root===e.root&&c.model===model);if(!control)throw new Error('Missing matched control');
      return ['wins','ties','losses'][1+compare(e.summary.best_score.tuple,control.summary.best_score.tuple)];
    });
    evaluation.push({horizon,seed,models,comparisons});
  }
  return {schema_version:1,status:'measured-not-production-ready',scope:r.scope,elapsed_seconds:r.elapsed_seconds,
    training:Object.fromEntries(Object.entries(r.training).filter(([k])=>k!=='groups')),generation,evaluation,
    target:r.evaluation.filter(e=>e.root==='heavy-strong').map(e=>({model:e.model,horizon:e.horizon,seed:e.seed,
      best:e.summary.best_score.tuple,percent_empty:e.summary.density.percent_empty,waste:e.summary.waste,work:e.summary.work,elapsed_ms:e.summary.elapsed_ms})),
    constructor_failures:r.generation.filter(g=>g.status.startsWith('construction')).map(g=>({split:g.split,ordinal:g.ordinal,
      target:g.sample.request.target.item,facilities:g.facilities,status:g.status,diagnostics:g.diagnostics??[g.diagnostic]})),
    audited_episodes:audits.length,evaluation_episodes:r.evaluation.length,
    proposal_counts:counts(r.evaluation,e=>`${e.horizon}:${e.summary.work.proposals}`),test_evaluations:r.test_evaluations,
    limitations:['One model seed and one evaluation seed; development evidence only.',
      'Direct decoder masking/inference time is included in episode wall time but not separated per evaluation row in this run.',
      'Episode wall time includes co-batched worker/inference waits; not isolated per-factory latency.',
      'Same selected-action limits and per-action routing caps, not equal consumed routing work.',
      'Source cycle references are not proof that every cycle is active in the solved production graph.',
      'No eligible fresh training factory exceeded ten facilities; heavy-strong is explicit training replay.',
      'Semantic fingerprint includes source-path identities; it is not general graph-isomorphism deduplication.',
      'Current-extent and frozen external-anchor execution restrictions remain; no global impossibility claim.']};
}

if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  try{const o=args(['report','output']),result=summarize(read(o.report));
    write(o.output,{...result,source:{path:path.resolve(o.report),sha256:hash(o.report)}});
    console.log(JSON.stringify({status:result.status,audited_episodes:result.audited_episodes,output:path.resolve(o.output)}));
  }catch(e){console.error(JSON.stringify({code:'stream-analysis-failed',severity:'error',stage:'analysis',message:e.message,entity:null}));process.exitCode=1;}
}
