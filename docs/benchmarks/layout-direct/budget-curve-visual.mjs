import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {args,write,hash,digest,catalogKeys,Probe,sourcePaths} from '../layout-rl/io.mjs';
import {drawing} from '../layout-rl/visualize.mjs';
import {prepareRoot,checkRoot} from '../layout-rl/coverage.mjs';
import {density,routeDiagnostics} from '../layout-production/metrics.mjs';
import {score,compare} from '../layout-rl/score.mjs';

const declared=[64,256,1024],modelSeeds=[1901,2901,3901];
const rowAt=(evaluation,root,seed)=>{
  const rows=evaluation.episodes.filter(e=>e.seed===seed&&e.summary.root===root);
  if(rows.length!==1)throw Error(`Missing or duplicate paired episode: ${root}/${seed}`);
  return rows[0];
};

export function selectCases(roots,runs,through){
  if(!declared.includes(through)||runs[0]?.seed!==modelSeeds[0])throw Error('Expected a declared checkpoint and first training seed');
  for(const run of runs)for(const checkpoint of [0,...declared.filter(n=>n<=through)]){
    const evaluation=run.evaluations[checkpoint];
    if(!evaluation||evaluation.episodes.length!==roots.length*2)throw Error('Incomplete checkpoint evaluation allocation');
    for(const root of roots)for(const seed of [101,211])if(rowAt(evaluation,root.id,seed).summary.work.proposals!==1024)throw Error('Checkpoint contains unequal evaluation effort');
  }
  const largest=[...roots.slice(0,-1)].sort((a,b)=>b.witness.placements.length-a.witness.placements.length||a.id.localeCompare(b.id))[0],target=roots.at(-1),selected=[];
  const pair=(run,root,checkpoint,seed,kind)=>{
    const initial=rowAt(run.evaluations[0],root.id,seed),trained=rowAt(run.evaluations[checkpoint],root.id,seed);
    if(initial.summary.work.proposals!==1024||trained.summary.work.proposals!==1024)throw Error('Paired visual effort is not 1024 proposals');
    return {root,model_seed:run.seed,checkpoint,seed,kind,initial,trained,
      delta:trained.summary.best_score.tuple.map((v,i)=>v-initial.summary.best_score.tuple[i])};
  };
  for(const checkpoint of declared.filter(n=>n<=through)){
    selected.push(pair(runs[0],target,checkpoint,101,'Fixed target'));
    selected.push(pair(runs[0],largest,checkpoint,101,'Fixed largest validation factory'));
  }
  if(through===1024){
    if(runs.map(r=>r.seed).sort((a,b)=>a-b).join()!==modelSeeds.join())throw Error('Final extrema require all three declared seeds');
    const candidates=runs.flatMap(run=>roots.flatMap(root=>[101,211].map(seed=>pair(run,root,1024,seed,'Least favorable final pair (post-hoc)'))));
    candidates.sort((a,b)=>compare(b.delta,a.delta)||a.model_seed-b.model_seed||a.root.id.localeCompare(b.root.id)||a.seed-b.seed);
    selected.push(candidates[0]);
  }
  return selected;
}

export function sharedBounds(cases){
  const bounds=new Map();
  for(const c of cases){
    const b=bounds.get(c.root)??{width:0,height:0};
    for(const side of [c.untrained,c.trained]){b.width=Math.max(b.width,side.score.width);b.height=Math.max(b.height,side.score.height);}
    bounds.set(c.root,b);
  }
  return cases.map(c=>({...c,display_bounds:bounds.get(c.root)}));
}

async function run(options){
  const through=Number(options.checkpoint),directories=options.runs.split(',').map(p=>path.resolve(p)),provenance=[];
  const take=p=>{const bytes=fs.readFileSync(p);provenance.push({path:path.resolve(p),sha256:hash(p)});return JSON.parse(bytes);};
  const checked=ref=>{if(hash(ref.path)!==ref.sha256)throw Error(`Changed evidence: ${ref.path}`);return take(ref.path);};
  const checkSources=sources=>{for(const ref of sources)if(hash(ref.path)!==ref.sha256)throw Error(`Changed frozen source: ${ref.path}`);};
  const runs=[],references=[];
  for(const directory of directories){
    const group=take(path.join(directory,'group-0001.json')),config=take(path.join(directory,'configuration.json'));
    if(config.checkpoints.join()!==declared.join()||config.seeds.join()!==modelSeeds.join())throw Error('Changed budget allocation');
    const reference=take(path.join(directory,'evaluation-reference.json'));references.push(reference);
    checkSources(take(path.join(directory,'provenance.json')));
    const evaluations={0:take(path.join(directory,'evaluation-0000-initial.json'))};
    for(const n of declared.filter(n=>n<=through)){
      evaluations[n]=take(path.join(directory,`evaluation-${String(n).padStart(4,'0')}-trained.json`));
      const step=take(path.join(directory,`group-${String(n).padStart(4,'0')}.json`));
      if(evaluations[n].training_state_sha256!==step.after_sha256||step.group!==n)throw Error('Visual checkpoint is not the cumulative trained snapshot');
    }
    if(through===1024){const report=take(path.join(directory,'report.json'));if(report.status!=='complete'||report.groups.length!==1024)throw Error('Final inspection requires completed seed runs');}
    runs.push({seed:group.seed,evaluations});
  }
  if(references.some(r=>r.path!==references[0].path||r.sha256!==references[0].sha256))throw Error('Evaluation populations differ across seeds');
  const frozen=checked(references[0]);checkSources(frozen.sources);
  if(frozen.scope!=='cumulative-budget-evaluation-roots'||frozen.roots.length!==13)throw Error('Expected thirteen frozen full-factory roots');
  const planPath=path.resolve(options.plan),plan=take(planPath),resolve=p=>path.resolve(path.dirname(planPath),p);
  const paths=Object.fromEntries(catalogKeys.map(k=>[k,resolve(plan.catalogs[k])])),catalogs=Object.fromEntries(catalogKeys.map(k=>[k,take(paths[k])]));
  for(const p of [...Object.values(paths),resolve(plan.validator)])if(!frozen.sources.some(s=>s.path===p&&s.sha256===hash(p)))throw Error(`Unpinned visual catalog/validator: ${p}`);
  const selection=selectCases(frozen.roots,runs,through),cases=[],audits=[];
  const probe=new Probe({...paths,validator:resolve(plan.validator)},plan.validator_ms);
  try{
    for(const selected of selection){
      const {root,checkpoint,model_seed,seed,kind,delta}=selected,graph=prepareRoot(root,catalogs);
      const aliases=new Map(root.witness.placements.map((p,i)=>[p.instance,`F${String(i+1).padStart(2,'0')}`])),sides=[];
      for(const [arm,e]of [['initial',selected.initial],['trained',selected.trained]]){
        const episode=checked(e.artifact),w=episode.best_witness;
        if(digest(w)!==episode.best_witness_sha256||digest(w)!==e.summary.best_witness_sha256||compare(score(w,catalogs).tuple,e.summary.best_score.tuple))throw Error('Selected visual geometry/score differs from the evaluated witness');
        if(episode.work.proposals!==1024||episode.trace.length!==1024||episode.root!==root.id)throw Error('Selected visual episode is truncated or belongs to another root');
        const validation=await probe.check(w),coverage=checkRoot(root,graph,w,validation),metrics=density(w,catalogs);
        if(!coverage.factory_validated||!validation.covered_geometry_validated)throw Error('Selected visual lacks original full-factory validation');
        if(JSON.stringify(metrics)!==JSON.stringify(e.summary.density))throw Error('Visual density does not reproduce from geometry');
        audits.push({root:root.id,model_seed,checkpoint,seed,kind,arm,source:e.artifact,witness_sha256:digest(w),validation,coverage,density:metrics,routes:routeDiagnostics(w)});
        sides.push({...drawing(w,aliases,catalogs),density:metrics});
      }
      const result=compare(delta,[0,0,0]);
      cases.push({root:root.id,label:`${root.witness.placements.length} facilities · ${checkpoint} updates · ${kind}`,
        family:`${kind} · training seed ${model_seed} · ${result<0?'trained better':result>0?'trained worse':'quality tie'} · ${through<1024?'interim, not final budget result':'fixed final budget'} · area / tiles / turns delta ${delta.join(' / ')}`,
        checkpoint,model_seed,seed,horizon:1024,untrained:sides[0],trained:sides[1]});
    }
  }finally{probe.close();}
  const templatePath=fileURLToPath(new URL('./visual.html',import.meta.url)),template=fs.readFileSync(templatePath,'utf8');
  const oldBounds='const bounds={width:Math.max(c.untrained.score.width,c.trained.score.width),height:Math.max(c.untrained.score.height,c.trained.score.height)},scale=';
  if(!template.includes(oldBounds))throw Error('Visual template no longer has the expected common-scale contract');
  const fragment=template.replaceAll('streaming-policy-comparison','budget-policy-comparison').replaceAll('stream-','budget-')
    .replace('Validation factory','Factory and checkpoint').replaceAll('Untrained policy','Initial policy').replaceAll('Trained policy','Checkpoint policy')
    .replace(oldBounds,'const bounds=c.display_bounds,scale=')
    .replace('`${c.horizon} proposals each · rollout seed ${c.seed} · original factory obligations validated`',
      '`${c.checkpoint} cumulative updates · ${c.horizon} evaluation proposals each · sampling seed ${c.seed} · complete factories validated`')
    .replace('__STREAM_VISUAL_DATA__',JSON.stringify({cases:sharedBounds(cases)}).replaceAll('<','\\u003c'));
  if(Buffer.byteLength(fragment)>=1000000)throw Error('Cumulative-layout visual exceeds 1 MB');
  const output=path.resolve(options['output-directory']);fs.mkdirSync(output);
  fs.writeFileSync(path.join(output,'cumulative-layouts.html'),fragment,{flag:'wx'});
  provenance.push(...[templatePath,...sourcePaths(fileURLToPath(import.meta.url))].map(p=>({path:p,sha256:hash(p)})));
  write(path.join(output,'report.json'),{schema_version:1,status:'complete',scope:through===1024?'final-budget-visual-inspection':'interim-checkpoint-visual-inspection',through,
    selection:selection.map(({root,model_seed,checkpoint,seed,kind,delta})=>({root:root.id,model_seed,checkpoint,seed,kind,delta})),audits,provenance});
  console.log(JSON.stringify({status:'complete',through,cases:cases.length,validated_frames:audits.length,bytes:Buffer.byteLength(fragment),output}));
}

if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  try{await run(args(['plan','runs','checkpoint','output-directory']));}
  catch(e){console.error(JSON.stringify({code:'budget-curve-visual-failed',severity:'error',stage:'cumulative-layout-inspection',message:e.message,entity:null}));process.exitCode=1;}
}
