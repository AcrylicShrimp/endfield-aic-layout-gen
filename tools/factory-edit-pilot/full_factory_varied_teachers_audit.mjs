// Independent full-original replay for varied preparation and continuation teachers.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {args,read,write,hash,Probe} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {transform} from '../../docs/benchmarks/layout-rl/geometry.mjs';
import {describe,execute} from './full_factory_executor.mjs';
import {measure} from './full_factory_audit.mjs';
import {originalRequest} from './full_factory_inputs.mjs';

const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const cmp=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
const sorted=v=>Array.isArray(v)?v.map(sorted):v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,sorted(v[k])])):v;
const digest=v=>createHash('sha256').update(JSON.stringify(sorted(v))).digest('hex');
const validStatus=s=>['improved','accepted_equal','accepted_exploration'].includes(s);
function stratum(a,w){
  if(a.kind!=='pose')return a.kind;
  const rotation=a.poses.some(q=>q.rotation!==w.placements[q.member].rotation);
  if(a.poses.length>1)return rotation?'compound_rotation':'group_translation';
  if(rotation)return 'single_rotation';
  const q=a.poses[0],p=w.placements[q.member];return Math.abs(q.x-p.x)+Math.abs(q.y-p.y)<=2?'local_single':'distant_single';
}
function movement(before,after,action){
  if(action.kind!=='pose')return null;
  const selected=new Set(action.poses.map(q=>before.placements[q.member].instance)),now=new Map(after.placements.map(p=>[p.instance,p])),fixed=before.placements.filter(p=>!selected.has(p.instance));assert(fixed.length);
  const dx=now.get(fixed[0].instance).x-fixed[0].x,dy=now.get(fixed[0].instance).y-fixed[0].y;
  for(const p of fixed)assert.deepEqual([now.get(p.instance).x-p.x,now.get(p.instance).y-p.y,now.get(p.instance).rotation-p.rotation],[dx,dy,0]);
  return {translated:before.placements.filter(p=>selected.has(p.instance)&&(now.get(p.instance).x-p.x!==dx||now.get(p.instance).y-p.y!==dy)).length,
    rotated:before.placements.filter(p=>selected.has(p.instance)&&now.get(p.instance).rotation!==p.rotation).length,frame_shift:[dx,dy]};
}

async function main(o){
  assert(['preparation','collection'].includes(o.phase));
  const p=read(path.join(o.screen,'protocol.json')),r=read(path.join(o.screen,'report.json'));
  assert.equal(r.status,'complete');assert.equal(r.training_performed,false);assert.equal(p.training_performed,false);assert.equal(r.protocol_sha256,hash(path.join(o.screen,'protocol.json')));
  for(const [file,h] of Object.entries({...p.sources,...p.source_sha256}))assert.equal(hash(file),h,file);
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>[k,read(v)])),roots=new Map();
  for(const c of p.cases){
    assert.equal(c.split,'train');assert(!roots.has(c.id));
    for(const [key,name] of [['initial','initial.json'],['request','request.json'],['original','original-request.json']])assert.equal(hash(c[key]),c.files_sha256[name]);
    const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},graph=prepareRoot(root,catalogs),initial=transform(root.witness);
    assert.deepEqual(read(c.request),originalRequest(graph,c.dialect));assert.equal(initial.placements.length,c.instances);
    roots.set(c.id,{case:c,root,graph,initial});
  }
  const probe=new Probe({...paths,validator:path.resolve(o.validator)}),rows=[];let replayed=0;
  async function transition(data,current,best,t,limit,d){
    d??=describe(current,catalogs);assert(d.actions.some(a=>digest(a)===digest(t.action)));
    const result=execute(current,catalogs,t.action,p.config.route_work);assert.equal(result.status,t.executor_status);assert.equal(result.route_expansions,t.route_expansions);assert(t.route_expansions<=p.config.route_work);
    assert(!t.diagnostics.some(d=>d.code==='factory.executor-error'));
    const valid=Boolean(result.witness&&checkRoot(data.root,data.graph,result.witness,await probe.check(result.witness)).factory_validated),next=valid?transform(result.witness):current,
      metric=measure(next,catalogs),improving=valid&&cmp(metric.score,measure(best,catalogs).score)<0,
      status=!result.witness?'executor_rejected':!valid?'rejected':improving?'improved':cmp(metric.score,measure(current,catalogs).score)===0?'accepted_equal':'accepted_exploration';
    if(t.status==='late')assert(t.elapsed_seconds>=limit);
    else{
      assert.equal(t.status,status);assert(t.elapsed_seconds<limit);
      if(valid){current=next;if(improving)best=current;}
    }
    assert.equal(digest(current),t.current_sha256);assert.deepEqual(measure(current,catalogs).score,t.current_score);assert.deepEqual(measure(best,catalogs).score,t.best_score);
    replayed++;return {current,best,valid:valid&&t.status!=='late'};
  }
  try{
    if(o.phase==='preparation'){
      const admission=read(path.join(p.admission,'report.json')),accepted=admission.rows.filter(a=>a.status==='accepted'&&a.factory_validated);
      assert.equal(admission.attempted,admission.rows.length);assert.deepEqual(p.cases.map(c=>c.id),accepted.map(c=>c.id));assert.deepEqual(r.rows.map(row=>row.root),p.cases.map(c=>c.id));
      for(const row of r.rows){
        const data=roots.get(row.root),e=unpack(path.join(row.directory,'preparation.json.gz'));
        assert.equal(hash(path.join(row.directory,'preparation.json.gz')),row.trace_sha256);assert.equal(hash(path.join(row.directory,'best.json')),row.best_sha256);
        assert.equal(e.selected,e.trace.length);assert.equal(row.selected,e.selected);assert(e.selected<=p.config.preparation_horizon);
        let current=data.initial,best=current;
        assert(checkRoot(data.root,data.graph,current,await probe.check(current)).factory_validated);assert.deepEqual(row.initial_metrics,measure(current,catalogs));
        for(const [i,t] of e.trace.entries()){
          const d=describe(current,catalogs),choices=d.actions.filter(a=>a.kind==='contract');assert.equal(t.selected,i+1);
          assert.equal(t.available_contractions,choices.length);assert.deepEqual(t.action,choices[0]);assert.deepEqual(t.action,d.actions[t.vocabulary_index]);
          const before=measure(current,catalogs),result=await transition(data,current,best,t,row.episode_seconds,d);
          if(t.status!=='late'){assert.equal(t.status,'improved');assert(measure(result.current,catalogs).score[0]<before.score[0]);}
          current=result.current;best=result.best;
        }
        assert.equal(digest(best),e.best_sha256);assert.equal(digest(best),digest(read(path.join(row.directory,'best.json'))));assert.deepEqual(row.best_metrics,measure(best,catalogs));
        if(row.status==='contraction_exhausted'){
          assert.equal(row.remaining_contractions,0);assert.equal(describe(current,catalogs).actions.filter(a=>a.kind==='contract').length,0);
          assert.equal(row.prepared.root,data.case.id);assert.equal(row.prepared.original,data.case.original);assert.equal(row.prepared.request,data.case.request);
          assert.equal(hash(row.prepared.initial),row.prepared.files_sha256['initial.json']);assert.equal(digest(read(row.prepared.initial)),digest(best));
        }else assert.equal(row.prepared,null);
        rows.push({root:row.root,status:row.status,selected:row.selected,score:row.best_metrics.score,best_sha256:digest(best)});
        console.log(JSON.stringify({root:row.root,replayed_actions:replayed,status:row.status}));
      }
      assert.equal(replayed,r.selected);assert.equal(rows.filter(r=>r.status==='contraction_exhausted').length,r.prepared);
    }else{
      const prep=read(path.join(p.preparation,'report.json')),proof=read(path.join(p.preparation,'audit.json'));
      assert.equal(proof.status,'passed');assert.equal(proof.report_sha256,hash(path.join(p.preparation,'report.json')));assert.deepEqual(p.cases,prep.rows.filter(r=>r.status==='contraction_exhausted').map(r=>r.prepared));
      assert.deepEqual(r.rows.map(row=>row.case),p.cases.map(c=>c.id));
      for(const [ordinal,row] of r.rows.entries()){
        const data=roots.get(row.case),selection=read(path.join(row.directory,'selection.json')),trials=unpack(path.join(row.directory,'trajectories.json.gz')),
          d=describe(data.initial,catalogs),metric=measure(data.initial,catalogs),seen=new Set();
        assert.equal(hash(path.join(row.directory,'selection.json')),row.selection_sha256);assert.equal(hash(path.join(row.directory,'trajectories.json.gz')),row.trajectories_sha256);
        assert.deepEqual(selection.case,data.case);assert.equal(selection.seed,p.config.seed+ordinal);assert.deepEqual(selection.initial_metrics,metric);assert.deepEqual(row.initial_metrics,metric);
        assert.equal(d.actions.filter(a=>a.kind==='contract').length,0);assert.equal(trials.length,row.trials);assert.equal(row.trials,row.planned);assert.equal(row.planned,selection.selections.length);
        for(const [kind,pop] of Object.entries(selection.populations)){
          assert.equal(pop.quota,({...p.config.pose_quotas,...p.config.other_quotas})[kind]);assert.equal(pop.population,d.actions.filter(a=>stratum(a,data.initial)===kind).length);
          assert.equal(pop.selected,Math.min(pop.quota,pop.population));assert.equal(selection.selections.filter(s=>s.stratum===kind).length,pop.selected);
        }
        assert(checkRoot(data.root,data.graph,data.initial,await probe.check(data.initial)).factory_validated);
        let globalBest=data.initial,teacherBest=data.initial,selected=0;const outcomes=[],terms={};
        for(const [i,trial] of trials.entries()){
          const choice=selection.selections[i],e=trial.summary;assert.equal(trial.id,i);assert.deepEqual(trial.action,choice.action);assert.equal(trial.vocabulary_index,choice.vocabulary_index);
          assert.equal(trial.stratum,choice.stratum);assert(!seen.has(choice.vocabulary_index));seen.add(choice.vocabulary_index);assert.deepEqual(d.actions[choice.vocabulary_index],choice.action);assert.equal(trial.stratum,stratum(trial.action,data.initial));
          assert.equal(e.selected,e.trace.length);assert(e.selected>=1&&e.selected<=p.config.horizon);assert.deepEqual(e.initial_metrics,metric);assert.deepEqual(e.trace[0].action,trial.action);
          let current=data.initial,best=current,firstValid=false,moved=null;
          for(const [step,t] of e.trace.entries()){
            assert.equal(t.selected,step+1);const description=step?describe(current,catalogs):d,before=measure(current,catalogs);
            if(step){const choices=description.actions.filter(a=>a.kind==='contract');assert.equal(t.available_contractions,choices.length);assert.deepEqual(t.action,choices[0]);assert.deepEqual(t.action,description.actions[t.vocabulary_index]);}
            const result=await transition(data,current,best,t,trial.episode_seconds,description);
            if(!step){firstValid=result.valid;moved=firstValid?movement(current,result.current,t.action):null;}
            if(step&&step<e.trace.length-1){assert(result.valid);assert(cmp(measure(result.current,catalogs).score,before.score)<0);}
            current=result.current;best=result.best;
          }
          assert.equal(trial.first_valid,firstValid);assert.deepEqual(trial.displacement,moved);assert.equal(digest(best),e.best_sha256);assert.deepEqual(measure(best,catalogs),e.best_metrics);
          assert.equal(trial.completed,['first_rejected','contraction_exhausted','horizon','continuation_failed'].includes(trial.termination));
          if(trial.termination==='first_rejected'){assert.equal(firstValid,false);assert.equal(e.trace.length,1);}
          else if(trial.termination==='contraction_exhausted'){assert(firstValid);assert.equal(describe(current,catalogs).actions.filter(a=>a.kind==='contract').length,0);}
          else if(trial.termination==='horizon'){assert.equal(e.selected,p.config.horizon);assert(e.trace.every(t=>t.status!=='late'));}
          else if(trial.termination==='continuation_failed')assert(e.selected>1);
          else assert.equal(trial.termination,'time_budget');
          if(cmp(e.best_metrics.score,measure(globalBest,catalogs).score)<0)globalBest=best;
          if(trial.completed&&cmp(e.best_metrics.score,measure(teacherBest,catalogs).score)<0)teacherBest=best;
          selected+=e.selected;terms[trial.termination]=(terms[trial.termination]??0)+1;
          outcomes.push({id:i,stratum:trial.stratum,selected:e.selected,first_valid:firstValid,displacement:moved,score:e.best_metrics.score,completed:trial.completed,termination:trial.termination});
        }
        assert.deepEqual(row.best_metrics,measure(globalBest,catalogs));assert.deepEqual(row.teacher_metrics,measure(teacherBest,catalogs));
        assert.equal(hash(path.join(row.directory,'best.json.gz')),row.best_sha256);assert.equal(digest(unpack(path.join(row.directory,'best.json.gz'))),digest(globalBest));
        assert.equal(hash(row.teacher_path),row.teacher_sha256);assert.equal(digest(unpack(row.teacher_path)),digest(teacherBest));assert.equal(row.selected,selected);assert.deepEqual(row.terminations,terms);
        assert.equal(row.first_valid,outcomes.filter(t=>t.first_valid).length);assert.equal(row.area_improved,outcomes.filter(t=>t.score[0]<metric.score[0]).length);
        assert.deepEqual(row.targets,outcomes.filter(t=>t.completed&&cmp(t.score,row.teacher_metrics.score)===0&&cmp(row.teacher_metrics.score,metric.score)<0).map(t=>t.id));
        rows.push({root:row.root,trials:trials.length,selected,targets:row.targets,score:row.teacher_metrics.score,outcomes});
        console.log(JSON.stringify({root:row.root,trials:trials.length,replayed_actions:replayed,targets:row.targets}));
      }
      assert.equal(replayed,r.selected);assert.equal(rows.reduce((n,r)=>n+r.trials,0),r.trials);
    }
    write(o.output,{schema_version:1,status:'passed',phase:o.phase,report_sha256:hash(path.join(o.screen,'report.json')),replayed_actions:replayed,rows,
      scope:'All original graph/native/terminal obligations, ordinary action membership, independent current/incumbent metrics and exact hashes; contraction exhaustion/order and measured complete-trajectory minima'});
    console.log(JSON.stringify({status:'passed',phase:o.phase,replayed_actions:replayed}));
  }finally{probe.close();}
}
main(args(['screen','phase','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
