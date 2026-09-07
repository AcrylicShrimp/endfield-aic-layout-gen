import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args,read,write,hash } from '../layout-rl/io.mjs';
import { compare } from '../layout-rl/score.mjs';

export function summarize(report){
  if(report.schema_version!==1||report.status!=='complete'||report.test_evaluations!==0)throw new Error('Incomplete or wrong-scope sequential report');
  const {training,evaluation}=report,groups=training.groups;
  if(training.optimizer_steps!==groups.length||training.sampled_transitions!==groups.reduce((s,g)=>s+g.sampled_transitions,0))throw new Error('Training accounting mismatch');
  const keys=new Map();
  for(const row of evaluation){
    const s=row.summary;
    if(!row.audit.coverage.factory_validated||!s.factory_validated||compare(s.best_score.tuple,s.initial_score.tuple)>0||s.work.proposals>row.horizon)throw new Error('Evaluation invariant failed');
    const key=JSON.stringify([row.root,row.horizon,row.seed]),pair=keys.get(key)??{};
    if(!['initial','trained'].includes(row.model)||pair[row.model])throw new Error('Duplicate/unknown evaluation control');
    pair[row.model]=row;keys.set(key,pair);
  }
  const pairs=[...keys.values()].map(p=>{
    if(!p.initial||!p.trained)throw new Error('Missing matched model control');
    if(JSON.stringify(p.initial.summary.initial_score.tuple)!==JSON.stringify(p.trained.summary.initial_score.tuple))throw new Error('Mismatched initial root scores');
    const compact=r=>({best:r.summary.best_score.tuple,proposals:r.summary.work.proposals,termination:r.summary.termination,
      futile_fraction:r.summary.waste.futile_fraction,empty_fraction:r.summary.density.empty_fraction,
      elapsed_ms:r.summary.elapsed_ms,preprocess_ms:r.summary.work.preprocess_ms,feature_ms:r.summary.work.feature_ms,route_ms:r.summary.work.route_ms,
      artifact:r.artifact});
    return {root:p.initial.root,split:p.initial.split,horizon:p.initial.horizon,seed:p.initial.seed,initial_score:p.initial.summary.initial_score.tuple,
      initial:compact(p.initial),trained:compact(p.trained),trained_comparison:compare(p.trained.summary.best_score.tuple,p.initial.summary.best_score.tuple)};
  });
  const cohorts=report.config.evaluation_horizons.map(horizon=>{
    const rows=pairs.filter(p=>p.horizon===horizon&&p.split==='validation');
    return {horizon,pairs:rows.length,wins:rows.filter(p=>p.trained_comparison<0).length,ties:rows.filter(p=>p.trained_comparison===0).length,
      losses:rows.filter(p=>p.trained_comparison>0).length,...Object.fromEntries(['initial','trained'].map(model=>[model,{
        mean_area_ratio:rows.reduce((s,p)=>s+p[model].best[0]/p.initial_score[0],0)/rows.length,
        mean_futile_fraction:rows.reduce((s,p)=>s+p[model].futile_fraction,0)/rows.length,
        elapsed_terminations:rows.filter(p=>p[model].termination==='elapsed-budget').length}]))};
  });
  return {schema_version:1,status:'measured',scope:report.scope,test_evaluations:0,config:report.config,
    training:{optimizer_steps:training.optimizer_steps,changed_weight_elements:training.changed_weight_elements,
      informative_groups:training.informative_groups,sampled_transitions:training.sampled_transitions,
      target_groups:groups.filter(g=>g.root==='heavy-strong').map(g=>({group:g.group,informative:g.terminal_advantages.some(a=>a!==0),
        best:g.summaries.reduce((a,s)=>compare(a,s.best_score.tuple)<0?a:s.best_score.tuple,g.summaries[0].best_score.tuple)}))},
    cohorts,pairs,elapsed_seconds:report.elapsed_seconds,readiness:'not-established'};
}

export function archive(options){
  const reportPath=path.resolve(options.report),report=read(reportPath),summary=summarize(report),operators={};
  for(const p of report.provenance)if(hash(p.path)!==p.sha256)throw new Error(`Changed source: ${p.path}`);
  const artifacts=[...report.training.groups.flatMap(g=>g.artifacts),...report.evaluation.map(r=>r.artifact)];
  for(const p of artifacts)if(hash(p.path)!==p.sha256)throw new Error(`Changed trajectory: ${p.path}`);
  for(const g of report.training.groups)for(const a of g.artifacts)for(const t of read(a.path).trace){
    const kind=`${t.action.kind}${t.action.compact_after?'+compact':''}`,r=operators[kind]??={proposals:0,accepted:0,improved:0};
    r.proposals++;r.accepted+=Number(t.accepted);r.improved+=Number(t.new_incumbent===true);
  }
  const output=path.resolve(options['output-directory']);fs.mkdirSync(output);
  write(path.join(output,'results.json'),{...summary,operators,source:{path:reportPath,sha256:hash(reportPath)}});
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  try{archive(args(['report','output-directory']));}
  catch(e){console.error(JSON.stringify({code:'sequential-results-failed',severity:'error',stage:'results',message:e.message,entity:null}));process.exitCode=1;}
}
