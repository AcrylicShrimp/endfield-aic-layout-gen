// Independent full-original replay, history masks and return/checkpoint accounting.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,Probe} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {transform} from '../../docs/benchmarks/layout-rl/geometry.mjs';
import {describe,execute} from './full_factory_executor.mjs';
import {measure} from './full_factory_audit.mjs';
import {originalRequest} from './full_factory_inputs.mjs';

const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const ordered=v=>Array.isArray(v)?v.map(ordered):v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,ordered(v[k])])):v;
const digest=v=>createHash('sha256').update(JSON.stringify(ordered(v))).digest('hex');
const equal=(a,b)=>digest(a)===digest(b);
const cmp=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
const canonical=v=>Array.isArray(v)?v.map(canonical).sort((a,b)=>JSON.stringify(a)<JSON.stringify(b)?-1:JSON.stringify(a)>JSON.stringify(b)?1:0):
  v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])])):v;
const physical=w=>digest(canonical(w));
const context={stage:'input',entity:null};
const auditSource=fileURLToPath(import.meta.url),auditSourceHash=hash(auditSource);

function actionKey(action,d,w,bestKey){
  const value=structuredClone(action);
  if(value.kind==='pose')value.poses=value.poses.map(q=>({...q,member:w.placements[q.member].instance})).sort((a,b)=>a.member<b.member?-1:a.member>b.member?1:0);
  else if(value.kind==='reroute'){
    const chain=d.chains[value.chain];delete value.chain;value.network=w.transport_networks[chain.network].id;
    value.directed_cells=chain.cells;value.rate=chain.rate;
  }else if(value.kind==='restore_best')value.best_key=bestKey;
  return digest(value);
}

function movement(before,after,action){
  const selected=new Set(action.poses.map(q=>before.placements[q.member].instance)),now=new Map(after.placements.map(p=>[p.instance,p]));
  const fixed=before.placements.filter(p=>!selected.has(p.instance)),q=action.poses[0],actual=now.get(before.placements[q.member].instance);
  const dx=fixed.length?now.get(fixed[0].instance).x-fixed[0].x:actual.x-q.x;
  const dy=fixed.length?now.get(fixed[0].instance).y-fixed[0].y:actual.y-q.y;
  for(const p of fixed)assert.deepEqual([now.get(p.instance).x-p.x,now.get(p.instance).y-p.y,now.get(p.instance).rotation-p.rotation],[dx,dy,0]);
  return {translated:before.placements.filter(p=>selected.has(p.instance)&&(now.get(p.instance).x-p.x!==dx||now.get(p.instance).y-p.y!==dy)).length,
    rotated:before.placements.filter(p=>selected.has(p.instance)&&now.get(p.instance).rotation!==p.rotation).length,frame_shift:[dx,dy]};
}

function geometryIndex(w,actions,allowed){
  let index=actions.findIndex((a,i)=>allowed[i]&&a.kind==='contract');if(index>=0)return index;
  const centers=['x','y'].map((axis,i)=>w.placements.reduce((n,p)=>n+p[axis]+p[i?'height':'width']/2,0)/w.placements.length);
  let maximum=-Infinity;index=-1;
  for(const [i,a] of actions.entries())if(allowed[i]&&a.kind==='pose'){
    let gain=0;
    for(const q of a.poses){
      const p=w.placements[q.member],swap=Math.abs(q.rotation-p.rotation)%180!==0,pw=swap?p.height:p.width,ph=swap?p.width:p.height;
      gain+=Math.abs(p.x+p.width/2-centers[0])-Math.abs(q.x+pw/2-centers[0]);
      gain+=Math.abs(p.y+p.height/2-centers[1])-Math.abs(q.y+ph/2-centers[1]);
    }
    gain/=a.poses.length;if(gain>maximum){maximum=gain;index=i;}
  }
  if(index>=0)return index;
  index=actions.findIndex((a,i)=>allowed[i]&&a.kind==='restore_best');return index>=0?index:allowed.findIndex(Boolean);
}

async function main(o){
  context.entity=path.resolve(o.screen);
  assert(['training','evaluation'].includes(o.phase));
  const training=o.phase==='training',p=read(path.join(o.screen,'protocol.json')),reportFile=path.join(o.screen,training?'training-result.json':'report.json'),r=read(reportFile);
  assert(training?['complete','budget_exhausted'].includes(r.status):r.status==='complete');assert.equal(r.protocol_sha256,hash(path.join(o.screen,'protocol.json')));
  for(const [f,h] of Object.entries({...p.sources,...p.source_sha256}))assert.equal(hash(f),h,f);
  const expected=new Map(),checkpointHashes=new Map();
  context.stage='protocol-accounting';
  if(training){
    const source=read(r.source_report);assert.equal(hash(r.source_report),r.source_report_sha256);
    for(const [file,h] of Object.entries(r.analysis_source_sha256))assert.equal(hash(file),h,file);
    assert.deepEqual(r.rows,source.rows);assert.deepEqual(r.groups,source.groups);
    assert.equal(r.training_complete,r.status==='complete');
    if(r.training_complete){
      assert.equal(source.status,'complete');assert.equal(source.teacher_labels_used,false);assert.equal(source.future_evaluation_opened,false);
      assert.deepEqual(r.arms.map(a=>a.initialization),p.config.initializations);
    }else{
      assert.equal(source.status,'incomplete');assert.equal(source.code,'on-policy.training-failed');assert.equal(source.stage,'training');
      assert.equal(source.message,`on-policy.arm-budget: ${source.initialization}`);
      assert.equal(r.termination.code,'on-policy.arm-budget');assert.equal(r.termination.initialization,source.initialization);assert.equal(r.termination.group,source.group);
      assert.deepEqual(r.arms.map(a=>a.initialization),[...source.arms.map(a=>a.initialization),source.initialization]);
      const ended=r.arms.at(-1);assert.equal(ended.groups,source.group);assert.equal(ended.seconds,null);assert.equal(ended.elapsed_seconds_lower_bound,p.config.arm_seconds);
      assert(ended.groups<p.config.groups);assert.equal(ended.terminal_role,ended.groups?'last_completed_update':'initial_no_update');
    }
    assert.deepEqual(r.arms.map(a=>a.initialization),p.config.initializations.slice(0,r.arms.length));
    assert.deepEqual(r.unstarted_initializations,p.config.initializations.slice(r.arms.length));
    const proof=read(path.join(o.screen,'checkpoint-audit.json'));assert.equal(proof.status,'passed');assert.equal(proof.report_sha256,hash(reportFile));
    assert.equal(proof.source_report_sha256,r.source_report_sha256);assert.equal(proof.training_complete,r.training_complete);
    for(const row of proof.rows){assert.equal(hash(row.path),row.sha256);checkpointHashes.set(row.path,row.policy_sha256);}
    assert.equal(p.schedule.length,p.config.groups);
    assert.equal(new Set(p.training_cases).size,p.training_cases.length);
    const counts=p.training_cases.map(id=>p.schedule.filter(s=>s.case===id).length);assert(Math.max(...counts)-Math.min(...counts)<=1);
    const first=p.schedule.slice(0,p.training_cases.length).map(s=>s.case);assert.deepEqual([...first].sort(),[...p.training_cases].sort());
    for(const [g,s] of p.schedule.entries()){
      assert.equal(s.group,g);assert.equal(s.case,first[g%first.length]);assert.equal(p.cases.find(c=>c.id===s.case).split,'train');
      assert.deepEqual(s.sample_seeds,Array.from({length:p.config.group_episodes},(_,i)=>p.config.training_seed_base+g*p.config.group_episodes+i));
    }
    for(const arm of r.arms){
      assert(Number.isInteger(arm.groups)&&arm.groups>=0&&arm.groups<=p.config.groups);
      assert.equal(arm.planned_groups,p.config.groups);assert.equal(arm.completed_group_episodes,arm.groups*p.config.group_episodes);
      assert(Number.isInteger(arm.unapplied_episodes)&&arm.unapplied_episodes>=0&&arm.unapplied_episodes<=p.config.group_episodes);
      if(arm.terminal_role==='planned_final'){assert.equal(arm.groups,p.config.groups);assert.equal(arm.unapplied_episodes,0);}
      assert(checkpointHashes.has(arm.initial));assert(checkpointHashes.has(arm.terminal));
      assert.equal(hash(arm.initial),arm.initial_sha256);assert.equal(hash(arm.terminal),arm.terminal_sha256);
      for(let g=0;g<arm.groups+(arm.unapplied_episodes?1:0);g++){
        const s=p.schedule[g],count=g<arm.groups?p.config.group_episodes:arm.unapplied_episodes;
        for(let attempt=0;attempt<count;attempt++)expected.set(`${arm.initialization}:${g}:${attempt}`,{case:s.case,seed:s.sample_seeds[attempt],applied:g<arm.groups});
      }
    }
    assert.equal(r.groups.length,r.arms.reduce((n,a)=>n+a.groups,0));assert.equal(r.completed_groups,r.groups.length);
    assert.equal(r.completed_group_episodes,r.groups.length*p.config.group_episodes);
    assert.equal(r.unapplied_episodes,r.arms.reduce((n,a)=>n+a.unapplied_episodes,0));
    assert.equal(r.planned_selected_actions,p.config.initializations.length*p.config.groups*p.config.group_episodes*p.config.horizon);
    assert.equal(r.sampled_selected_actions,r.rows.reduce((n,e)=>n+e.selected,0));
    assert.equal(r.gradient_replayed_selected_actions,r.completed_group_episodes*p.config.horizon);
  }else{
    assert.equal(r.training_performed,false);assert.equal(r.policy_immutable,true);assert.equal(p.training_performed,false);
    const trainProof=read(path.join(p.training,'training-audit.json'));assert.equal(trainProof.status,'passed');assert.equal(trainProof.report_sha256,hash(path.join(p.training,'training-result.json')));
    const trainResult=read(path.join(p.training,'training-result.json'));
    assert.equal(p.training_result_status,trainResult.status);
    assert.deepEqual(p.training_completed_updates,Object.fromEntries(trainResult.arms.map(a=>[a.initialization,a.groups])));
    assert.deepEqual(Object.keys(p.checkpoints),trainResult.arms.flatMap(a=>['initial','terminal'].map(phase=>`${a.initialization}-${phase}`)));
    for(const arm of trainResult.arms)for(const phase of ['initial','terminal']){
      const record=p.checkpoints[`${arm.initialization}-${phase}`];
      assert.equal(record.path,arm[phase]);assert.equal(record.sha256,arm[`${phase}_sha256`]);
      assert.equal(record.completed_updates,phase==='initial'?0:arm.groups);
      assert.equal(record.terminal_role,phase==='initial'?'initial':arm.terminal_role);
    }
    assert.deepEqual(p.methods,[...Object.keys(p.checkpoints),'geometry']);
    for(const [method,record] of Object.entries(p.checkpoints))assert.equal(hash(record.path),record.sha256,method);
    const admission=read(path.join(p.admission,'report.json')),accepted=admission.rows.filter(r=>r.status==='accepted'&&r.factory_validated),prior=read(path.join(p.training,'protocol.json'));
    assert.equal(admission.attempted,admission.rows.length);assert.equal(admission.accepted,accepted.length);
    assert.deepEqual(p.cases.slice(0,prior.cases.length),prior.cases);
    assert.deepEqual(p.cases.slice(prior.cases.length).map(c=>c.id),accepted.map(r=>r.id));
    for(const method of p.methods)for(const c of p.cases)for(const bank of p.config.evaluation_banks)expected.set(`${method}:${c.id}:${bank}`,{case:c.id,seed:bank});
  }
  assert.equal(r.rows.length,expected.size);
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,f])=>[k,read(f)])),roots=new Map();
  for(const c of p.cases){
    context.stage='original-request';context.entity=c.id;
    for(const [key,name] of [['initial','initial.json'],['request','request.json'],['original','original-request.json']])assert.equal(hash(c[key]),c.files_sha256[name]);
    const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},graph=prepareRoot(root,catalogs),initial=transform(root.witness);
    assert.deepEqual(read(c.request),originalRequest(graph,c.dialect));assert.equal(c.instances,initial.placements.length);assert(!roots.has(c.id));
    roots.set(c.id,{case:c,root,graph,initial});
  }
  const probe=new Probe({...paths,validator:path.resolve(o.validator)}),rows=[];let replayed=0,descriptionsBuilt=0,descriptionCacheHits=0;
  try{
    for(const row of r.rows){
      context.stage='episode-replay';context.entity=row.directory;
      const key=training?`${row.initialization}:${row.group}:${row.attempt}`:`${row.method}:${row.case}:${row.bank}`,expect=expected.get(key);assert(expect);expected.delete(key);
      assert.equal(row.case,expect.case);assert.equal(row.bank,expect.seed);
      const data=roots.get(row.case),e=unpack(path.join(row.directory,'episode.json.gz')),w=unpack(path.join(row.directory,'best.json.gz')),
        states=training?unpack(path.join(row.directory,'states.json.gz')):null;
      assert.equal(row.phase,o.phase);assert.equal(row.family,data.case.family);assert.equal(row.split,data.case.split);assert.equal(row.transfer_group,data.case.transfer_group);assert.equal(row.instances,data.case.instances);
      assert.equal(row.selected,e.trace.length);assert(row.selected<=p.config.horizon);assert.equal(e.sample_seed,row.bank);assert.equal(e.geometry,!training&&row.method==='geometry');
      assert.deepEqual(e.config,Object.fromEntries(['horizon','temperature','balanced','memory','episode_seconds','route_work'].map(k=>[k,p.config[k]])));
      assert.equal(row.budget_complete,row.selected===p.config.horizon&&row.seconds<p.config.episode_seconds&&!e.trace.some(t=>t.status==='late'));
      if(training){
        assert(row.budget_complete);assert.equal(states.length,row.selected);
        const group=r.groups.find(g=>g.initialization===row.initialization&&g.group===row.group);
        if(expect.applied){assert(group);assert.equal(e.policy_sha256,group.policy_before_sha256);}
        else{
          assert.equal(group,undefined);const arm=r.arms.find(a=>a.initialization===row.initialization);
          assert.equal(row.group,arm.groups);assert(row.attempt<arm.unapplied_episodes);assert.equal(e.policy_sha256,checkpointHashes.get(arm.terminal));
        }
      }
      let current=data.initial,incumbent=current,bestKey=physical(current),routeWork=0;const seen=new Set([physical(current)]),failed=new Map(),operations=[],kinds={},statuses={},improvements={};
      let descriptionKey=null,cachedDescription=null;
      const initialMetrics=measure(current,catalogs);assert.deepEqual(row.initial_metrics,initialMetrics);assert.deepEqual(e.initial_metrics,initialMetrics);
      assert(checkRoot(data.root,data.graph,current,await probe.check(current)).factory_validated);
      for(const [i,t] of e.trace.entries()){
        context.entity=`${row.directory}:selected-${i+1}`;
        const currentKey=digest(current);
        assert.equal(t.selected,i+1);assert.equal(t.current_before_sha256,currentKey);assert(!t.diagnostics.some(d=>d.code==='factory.executor-error'));
        // One-entry cache keyed by the independently reconstructed full witness.
        // Only geometry/actions are consumed; cached timing fields are not used.
        if(descriptionKey!==currentKey){cachedDescription=describe(current,catalogs);descriptionKey=currentKey;descriptionsBuilt++;}
        else descriptionCacheHits++;
        const d=cachedDescription,before=physical(current),keys=d.actions.map(a=>actionKey(a,d,current,bestKey));
        const blocked=p.config.memory?(failed.get(before)??new Set()):new Set(),allowed=keys.map(k=>!blocked.has(k)),exhausted=!allowed.some(Boolean);
        if(exhausted)allowed[d.actions.findIndex(a=>a.kind==='restore_best')]=true;
        assert.equal(digest(allowed),t.allowed_sha256);assert.equal(allowed.filter(Boolean).length,t.allowed_count);assert.equal(exhausted,t.memory_exhausted);
        assert.equal(t.physical_before,before);assert(allowed[t.vocabulary_index]);assert.deepEqual(d.actions[t.vocabulary_index],t.action);assert.equal(keys[t.vocabulary_index],t.action_key);
        if(e.geometry)assert.equal(t.vocabulary_index,geometryIndex(current,d.actions,allowed));
        else assert(Number.isFinite(t.old_log_probability)&&t.old_log_probability<=1e-6&&Number.isFinite(t.sampling_entropy)&&t.sampling_entropy>=0);
        if(states){
          const record=states[i];assert.deepEqual(record.state.current,current);assert.deepEqual(record.state.initial_metrics,initialMetrics);
          assert.deepEqual(record.state.current_metrics,measure(current,catalogs));assert.deepEqual(record.state.best_metrics,measure(incumbent,catalogs));
          assert.deepEqual(record.allowed,allowed);assert.equal(record.index,t.vocabulary_index);assert.equal(record.observation_sha256,t.observation_sha256);
          assert.equal(record.old_log_probability,t.old_log_probability);assert.equal(record.probabilities_sha256,t.probabilities_sha256);
        }
        const result=t.action.kind==='restore_best'?{status:'candidate',witness:incumbent,route_expansions:0}:execute(current,catalogs,t.action,p.config.route_work);
        assert.equal(result.status,t.executor_status);assert.equal(result.route_expansions,t.route_expansions);assert(t.route_expansions<=p.config.route_work);
        const valid=Boolean(result.witness&&checkRoot(data.root,data.graph,result.witness,await probe.check(result.witness)).factory_validated),candidate=valid?transform(result.witness):current;
        const metrics=measure(candidate,catalogs),improving=valid&&cmp(metrics.score,measure(incumbent,catalogs).score)<0;
        const status=!result.witness?'executor_rejected':!valid?'rejected':improving?'improved':cmp(metrics.score,measure(current,catalogs).score)===0?'accepted_equal':'accepted_exploration';
        if(t.status==='late')assert(t.elapsed_seconds>=p.config.episode_seconds);
        else{
          assert.equal(t.status,status);assert(t.elapsed_seconds<p.config.episode_seconds);
          if(valid){
            if(t.action.kind==='pose')operations.push({selected:i+1,status:t.status,...movement(current,candidate,t.action),before:measure(current,catalogs).score,after:metrics.score});
            current=candidate;if(improving)incumbent=current;
          }
        }
        const after=physical(current),unchanged=before===after,revisited=!unchanged&&seen.has(after);
        assert.equal(t.physical_after,after);assert.equal(t.physical_unchanged,unchanged);assert.equal(t.physical_revisited,revisited);
        if(unchanged||revisited){if(!failed.has(before))failed.set(before,new Set());failed.get(before).add(keys[t.vocabulary_index]);}
        seen.add(after);if(t.status==='improved')bestKey=after;
        assert.equal(t.current_sha256,digest(current));assert.deepEqual(t.current_score,measure(current,catalogs).score);assert.deepEqual(t.best_score,measure(incumbent,catalogs).score);
        kinds[t.action.kind]=(kinds[t.action.kind]??0)+1;statuses[t.status]=(statuses[t.status]??0)+1;routeWork+=t.route_expansions;
        if(t.status==='improved')improvements[t.action.kind]=(improvements[t.action.kind]??0)+1;replayed++;
      }
      assert.equal(digest(incumbent),e.best_sha256);assert.equal(digest(incumbent),digest(w));assert(checkRoot(data.root,data.graph,w,await probe.check(w)).factory_validated);
      assert.deepEqual(measure(w,catalogs),row.best_metrics);assert.deepEqual(e.best_metrics,row.best_metrics);assert(cmp(row.best_metrics.score,row.initial_metrics.score)<=0);
      assert.deepEqual(kinds,row.kinds);assert.deepEqual(statuses,row.statuses);assert.deepEqual(improvements,row.improvements);assert.equal(routeWork,row.route_expansions);
      rows.push({key,case:row.case,selected:row.selected,score:row.best_metrics.score,operations,factory_validated:true,
        episode_sha256:hash(path.join(row.directory,'episode.json.gz')),best_sha256:hash(path.join(row.directory,'best.json.gz')),
        ...(training?{states_sha256:hash(path.join(row.directory,'states.json.gz')),optimizer_update_applied:expect.applied}:{})});
      if(rows.length%8===0)console.log(JSON.stringify({phase:o.phase,episodes:rows.length,replayed_actions:replayed}));
    }
    assert.equal(expected.size,0);
    if(training){
      for(const arm of r.arms){
        const groups=r.groups.filter(g=>g.initialization===arm.initialization);assert.equal(arm.groups,groups.length);
        assert.equal(arm.nonflat_groups,groups.filter(g=>!g.flat_returns).length);assert.equal(arm.reward_gradient_groups,groups.filter(g=>g.reward_gradient_norm>0).length);
        for(const [index,g] of groups.entries()){
          context.stage='group-accounting';context.entity=`${arm.initialization}:group-${index}`;
          const episodeRows=r.rows.filter(row=>row.initialization===arm.initialization&&row.group===index).sort((a,b)=>a.attempt-b.attempt);
          assert.equal(g.group,index);assert.equal(g.case,p.schedule[index].case);assert.deepEqual(g.sample_seeds,p.schedule[index].sample_seeds);
          assert.equal(episodeRows.length,p.config.group_episodes);assert.deepEqual(g.scores,episodeRows.map(row=>row.best_metrics.score));
          const returns=g.scores.map(s=>g.scores.reduce((sum,other)=>sum-cmp(s,other),0)/(g.scores.length-1));assert.deepEqual(g.returns,returns);
          assert.equal(g.flat_returns,returns.every(a=>a===0));assert.equal(g.replayed_actions,p.config.group_episodes*p.config.horizon);assert.equal(g.max_log_probability_error,0);
          if(g.flat_returns)assert.equal(g.reward_gradient_norm,0);
          for(const field of ['reward_loss','entropy_loss','reward_gradient_norm','entropy_gradient_norm','combined_gradient_norm','pre_clip_gradient_norm'])assert(Number.isFinite(g[field]));
          assert(g.combined_gradient_norm<=g.reward_gradient_norm+g.entropy_gradient_norm+1e-6);
          assert(Math.abs(g.combined_gradient_norm-g.pre_clip_gradient_norm)<1e-5*Math.max(1,g.combined_gradient_norm));
          assert.equal(hash(g.checkpoint),g.checkpoint_sha256);assert.equal(checkpointHashes.get(g.checkpoint),g.policy_after_sha256);
          assert.equal(g.behavior_policy_sha256,g.policy_before_sha256);
        }
      }
    }
    assert.equal(hash(auditSource),auditSourceHash,'Audit source changed while running');
    write(o.output,{schema_version:1,status:'passed',phase:o.phase,report_sha256:hash(reportFile),episodes:rows.length,replayed_actions:replayed,rows,
      auditor_source_sha256:{[auditSource]:auditSourceHash},description_cache:{built:descriptionsBuilt,hits:descriptionCacheHits,capacity:1},
      ...(training?{checkpoint_audit_sha256:hash(path.join(o.screen,'checkpoint-audit.json')),groups:r.groups.length,
        source_report_sha256:r.source_report_sha256,training_complete:r.training_complete,unapplied_episodes:r.unapplied_episodes}:{}),
      scope:'Every selected action and best, including unapplied training episodes: full original production/terminal graph, native validity, exact current/best hashes, independent history masks and physical movement; actual completed-group return/checkpoint accounting. Gradient identities are checked by completed training replays and dedicated tests, not recomputed by this JavaScript audit or claimed for unapplied episodes.'});
    console.log(JSON.stringify({status:'passed',phase:o.phase,episodes:rows.length,replayed_actions:replayed}));
  }finally{probe.close();}
}
const options=args(['screen','phase','validator','output']);
main(options).catch(e=>{
  write(options.output,{schema_version:1,status:'failed',phase:options.phase,diagnostics:[{
    code:'on-policy.replay-audit-failed',severity:'error',stage:context.stage,
    entity:context.entity,message:e.message||e.name}]});
  console.error(e.stack);process.exitCode=1;
});
