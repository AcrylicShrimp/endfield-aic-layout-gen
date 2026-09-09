// Independently admit all rehearsal contrasts and replay all evaluation transitions.
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

const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const cmp=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
const sorted=v=>Array.isArray(v)?v.map(sorted):v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,sorted(v[k])])):v;
const digest=v=>createHash('sha256').update(JSON.stringify(sorted(v))).digest('hex');

function movement(before,after,action){
  const selected=new Set(action.poses.map(q=>before.placements[q.member].instance)),now=new Map(after.placements.map(p=>[p.instance,p]));
  const fixed=before.placements.filter(p=>!selected.has(p.instance));
  const q=action.poses[0],actual=now.get(before.placements[q.member].instance);
  const dx=fixed.length?now.get(fixed[0].instance).x-fixed[0].x:actual.x-q.x;
  const dy=fixed.length?now.get(fixed[0].instance).y-fixed[0].y:actual.y-q.y;
  for(const p of fixed)assert.deepEqual([now.get(p.instance).x-p.x,now.get(p.instance).y-p.y,now.get(p.instance).rotation-p.rotation],[dx,dy,0]);
  return {translated:before.placements.filter(p=>selected.has(p.instance)&&(now.get(p.instance).x-p.x!==dx||now.get(p.instance).y-p.y!==dy)).length,
    rotated:before.placements.filter(p=>selected.has(p.instance)&&now.get(p.instance).rotation!==p.rotation).length,frame_shift:[dx,dy]};
}

async function main(o){
  assert(['lessons','evaluation'].includes(o.phase));
  const p=read(path.join(o.screen,'protocol.json')),a=read(path.join(o.screen,'admission.json')),lessons=read(path.join(o.screen,'lessons.json'));
  assert.equal(a.status,'complete');assert.equal(a.protocol_sha256,hash(path.join(o.screen,'protocol.json')));assert.equal(a.lessons_sha256,hash(path.join(o.screen,'lessons.json')));
  assert.equal(a.rehearsal_sha256,hash(path.join(o.screen,'rehearsal-contrasts.json.gz')));assert.equal(a.lessons,lessons.length);
  for(const [file,h] of Object.entries({...p.sources,...p.source_sha256}))assert.equal(hash(file),h,file);
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>[k,read(v)])),roots=new Map();
  for(const c of p.cases){
    const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)};
    assert(!roots.has(c.id));roots.set(c.id,{case:c,root,graph:prepareRoot(root,catalogs),initial:transform(root.witness)});
  }
  const probe=new Probe({...paths,validator:path.resolve(o.validator)});
  let replayed=0;
  async function transition(data,current,incumbent,t,description){
    const d=description??describe(current,catalogs);
    assert(d.actions.some(action=>JSON.stringify(action)===JSON.stringify(t.action)),'Ordinary action membership');
    const result=t.action.kind==='restore_best'?{status:'candidate',witness:incumbent,route_expansions:0}:execute(current,catalogs,t.action,p.config.route_work);
    assert.equal(result.status,t.executor_status);assert.equal(result.route_expansions,t.route_expansions);assert(t.route_expansions<=p.config.route_work);
    const valid=Boolean(result.witness&&checkRoot(data.root,data.graph,result.witness,await probe.check(result.witness)).factory_validated);
    const candidate=valid?transform(result.witness):current,metrics=measure(candidate,catalogs),improving=valid&&cmp(metrics.score,measure(incumbent,catalogs).score)<0;
    const status=!result.witness?'executor_rejected':!valid?'rejected':improving?'improved':cmp(metrics.score,measure(current,catalogs).score)===0?'accepted_equal':'accepted_exploration';
    let moved=null;
    if(t.status==='late'){
      assert(t.elapsed_seconds>=p.config.episode_seconds);
    }else{
      assert.equal(status,t.status);assert(t.elapsed_seconds<p.config.episode_seconds);
      if(valid){
        if(t.action.kind==='pose')moved=movement(current,candidate,t.action);
        current=candidate;if(improving)incumbent=current;
      }
    }
    assert.deepEqual(measure(current,catalogs).score,t.current_score);assert.deepEqual(measure(incumbent,catalogs).score,t.best_score);
    if(t.current_sha256)assert.equal(digest(current),t.current_sha256);
    replayed++;
    return {current,incumbent,moved,valid};
  }
  try{
    if(o.phase==='lessons'){
      const old=read(path.join(p.parent,'protocol.json')),source=read(path.join(old.collection,'contrasts.json')).rows,
        rehearsal=unpack(path.join(o.screen,'rehearsal-contrasts.json.gz'));
      const smallRoots=new Set(lessons.filter(l=>l.root!==p.large).map(l=>l.root));
      assert.deepEqual(rehearsal.map(r=>r.id),source.filter(r=>smallRoots.has(r.root)).map(r=>r.id));
      assert.equal(rehearsal.length,a.rehearsal_contrasts);
      const descriptions=new Map();
      for(const name of smallRoots){
        const data=roots.get(name);assert.equal(data.case.split,'train');
        assert(checkRoot(data.root,data.graph,data.initial,await probe.check(data.initial)).factory_validated);
        descriptions.set(name,describe(data.initial,catalogs));
      }
      const rowMap=new Map();
      for(const row of rehearsal){
        const original=source.find(r=>r.id===row.id),data=roots.get(row.root),d=descriptions.get(row.root);
        assert.deepEqual(row.action,original.action);assert.deepEqual(row.old_best_metrics,original.best_metrics);
        if(row.status==='not_offered'){
          assert.equal(row.vocabulary_index,null);assert.equal(row.completed,false);assert(!d.actions.some(a=>JSON.stringify(a)===JSON.stringify(row.action)));
        }else{
          assert.deepEqual(d.actions[row.vocabulary_index],row.action);assert.deepEqual(row.result.action,row.action);assert.equal(row.result.selected,1);
          const result=await transition(data,data.initial,data.initial,row.result,d);
          assert.deepEqual(measure(result.incumbent,catalogs),row.best_metrics);
          assert.equal(row.status,row.result.status);assert.equal(row.completed,!['late','budget_exhausted'].includes(row.status));
        }
        rowMap.set(row.id,row);
      }
      const sourceTrajectories=unpack(path.join(p.continuations,'trajectories.json.gz')),sourceReport=read(path.join(p.continuations,'report.json'));
      const target=sourceTrajectories.find(t=>t.id===sourceReport.targets[0]);
      let largeCurrent=roots.get(p.large).initial,largeBest=largeCurrent;
      for(const item of lessons){
        const data=roots.get(item.root);assert.equal(item.split,'train');assert.equal(item.family,data.case.family);
        assert.equal(hash(item.initial),item.initial_sha256);assert.equal(item.id,`${item.root}--lesson-${item.ordinal}`);
        const current=transform(read(item.initial)),metrics=measure(current,catalogs);
        assert.equal(digest(current),item.current_sha256);assert.deepEqual(metrics,item.initial_metrics);
        assert.deepEqual(item.episode_initial_metrics,measure(data.initial,catalogs));
        assert(checkRoot(data.root,data.graph,current,await probe.check(current)).factory_validated);
        const d=describe(current,catalogs),s=metrics.score,initial=item.episode_initial_metrics.score,best=item.episode_best_metrics.score;
        const context=[1-item.ordinal/p.config.horizon,Math.log1p(metrics.width),Math.log1p(metrics.height),Math.log1p(current.placements.length),
          s[0]/Math.max(1,initial[0]),s[1]/Math.max(1,initial[1]),best[0]/Math.max(1,initial[0]),best[1]/Math.max(1,initial[1])].map(Math.fround);
        assert.deepEqual(context,item.observation_context);
        assert.equal(item.targets.length,item.results.length);
        if(item.root===p.large){
          assert.equal(digest(current),digest(largeCurrent));assert.deepEqual(item.episode_best_metrics,measure(largeBest,catalogs));
          assert.equal(item.targets.length,1);assert.deepEqual(item.targets[0].action,target.summary.trace[item.ordinal].action);
          assert.equal(item.results[0].selected,item.ordinal+1);
          assert.equal(item.results[0].current_sha256,target.summary.trace[item.ordinal].current_sha256);
        }else{
          assert.equal(item.ordinal,0);assert.equal(digest(current),digest(data.initial));assert.deepEqual(item.episode_best_metrics,metrics);
          const positives=rehearsal.filter(r=>r.root===item.root&&r.completed&&r.status==='improved'),minimum=positives.map(r=>r.best_metrics.score).sort(cmp)[0];
          assert(minimum);assert.deepEqual(item.targets.map(t=>t.id),positives.filter(r=>cmp(r.best_metrics.score,minimum)===0).map(r=>r.id));
        }
        for(const [i,t] of item.targets.entries()){
          assert.deepEqual(d.actions[t.vocabulary_index],t.action);assert.deepEqual(t.action,item.results[i].action);
          if(item.root!==p.large)assert.deepEqual(item.results[i],rowMap.get(t.id).result);
          const result=await transition(data,current,item.root===p.large?largeBest:current,item.results[i],d);
          assert(result.valid);if(item.root===p.large){largeCurrent=result.current;largeBest=result.incumbent;}
        }
      }
      assert.deepEqual(measure(largeBest,catalogs),sourceReport.best_metrics);
      assert.equal(lessons.reduce((sum,l)=>sum+l.targets.length,0),a.targets);
      for(const seed of p.config.seeds){
        const order=p.schedules[String(seed)],byRoot=new Map(),lessonMap=new Map(lessons.map(l=>[l.id,l]));
        assert.equal(order.length,p.config.updates);
        for(const [i,step] of order.entries()){
          assert.equal(step.update,i);const item=lessonMap.get(step.lesson);assert(item);assert.equal(item.split,'train');
          if(!byRoot.has(item.root))byRoot.set(item.root,[]);byRoot.get(item.root).push(item.ordinal);
        }
        assert.equal(byRoot.size,new Set(lessons.map(l=>l.root)).size);
        const counts=[...byRoot.values()].map(v=>v.length);assert(Math.max(...counts)-Math.min(...counts)<=1);
        for(const [root,ordinals] of byRoot){const count=lessons.filter(l=>l.root===root).length;assert.deepEqual(ordinals,ordinals.map((_,i)=>i%count));}
      }
      write(o.output,{schema_version:1,status:'passed',lessons:lessons.length,targets:a.targets,rehearsal_contrasts:rehearsal.length,replayed_actions:replayed,
        admission_sha256:hash(path.join(o.screen,'admission.json')),scope:'Every remeasured historical training contrast and all trajectory/rehearsal targets: ordinary membership, full original graph, metrics, exact hashes, incumbent context and schedule membership'});
    }else{
      const report=read(path.join(o.screen,'report.json')),updates=read(path.join(o.screen,'updates.json')),scores=read(path.join(o.screen,'label-scores.json'));
      assert.equal(report.status,'complete');assert.equal(report.policy_immutable,true);assert.equal(report.protocol_sha256,hash(path.join(o.screen,'protocol.json')));
      assert.equal(report.lessons_audit_sha256,hash(path.join(o.screen,'lessons-audit.json')));
      const expectedUpdates=new Set(p.config.seeds.flatMap(seed=>p.schedules[String(seed)].map(s=>`${seed}:${s.update}`)));
      assert.equal(updates.length,expectedUpdates.size);assert.equal(report.updates,updates.length);
      for(const u of updates){assert(expectedUpdates.delete(`${u.seed}:${u.update}`));assert.deepEqual({update:u.update,lesson:u.lesson},p.schedules[String(u.seed)][u.update]);assert(Number.isFinite(u.loss)&&Number.isFinite(u.gradient_norm));}
      const expectedLabels=new Set(p.methods.filter(m=>m!=='geometry').flatMap(m=>lessons.map(l=>`${m}:${l.id}`)));
      assert.equal(scores.length,expectedLabels.size);
      for(const s of scores){assert(expectedLabels.delete(`${s.method}:${s.lesson}`));const item=lessons.find(l=>l.id===s.lesson);assert.equal(s.root,item.root);assert.equal(s.ordinal,item.ordinal);
        assert.equal(s.top1,item.targets.some(t=>JSON.stringify(t.action)===JSON.stringify(s.top_action)));assert(Number.isFinite(s.target_mass)&&s.target_mass>=0&&s.target_mass<=1.000001);assert(Number.isInteger(s.rank)&&s.rank>=1);}
      const expected=new Set(p.methods.flatMap(m=>p.cases.flatMap(c=>p.config.banks.map(b=>`${m}:${c.id}:${b}`)))),rows=[];
      assert.equal(report.rows.length,expected.size);
      for(const row of report.rows){
        const key=`${row.method}:${row.case}:${row.bank}`;assert(expected.delete(key));
        const data=roots.get(row.case),e=unpack(path.join(row.directory,'episode.json.gz')),w=unpack(path.join(row.directory,'best.json.gz'));
        assert.equal(row.family,data.case.family);assert.equal(row.split,data.case.split);assert.equal(row.selected,e.trace.length);assert(row.selected<=p.config.horizon);
        assert.deepEqual(row.initial_metrics,measure(data.initial,catalogs));assert.equal(row.budget_complete,row.selected===p.config.horizon&&!e.trace.some(t=>t.status==='late'));
        assert.equal(e.sample_seed,row.bank);assert.equal(e.config.horizon,p.config.horizon);assert.equal(e.config.geometry,row.method==='geometry');assert.equal(e.config.memory,true);
        let current=data.initial,incumbent=current;const operations=[],improvements={},kinds={},statuses={};let routeWork=0;
        assert(checkRoot(data.root,data.graph,current,await probe.check(current)).factory_validated);
        for(const [i,t] of e.trace.entries()){
          assert.equal(t.selected,i+1);
          const result=await transition(data,current,incumbent,t);
          if(result.moved)operations.push({selected:i+1,status:t.status,...result.moved,before:measure(current,catalogs).score,after:measure(result.current,catalogs).score});
          current=result.current;incumbent=result.incumbent;
          kinds[t.action.kind]=(kinds[t.action.kind]??0)+1;statuses[t.status]=(statuses[t.status]??0)+1;routeWork+=t.route_expansions;
          if(t.status==='improved')improvements[t.action.kind]=(improvements[t.action.kind]??0)+1;
        }
        assert.equal(digest(incumbent),e.best_sha256);assert.equal(digest(incumbent),digest(w));
        assert(checkRoot(data.root,data.graph,w,await probe.check(w)).factory_validated);assert.deepEqual(measure(w,catalogs),row.best_metrics);assert.deepEqual(e.best_metrics,row.best_metrics);
        assert.deepEqual(improvements,row.improvements);assert.deepEqual(kinds,row.kinds);assert.deepEqual(statuses,row.statuses);assert.equal(routeWork,row.route_expansions);
        rows.push({key,method:row.method,case:row.case,bank:row.bank,selected:row.selected,factory_validated:true,score:row.best_metrics.score,operations,
          episode_sha256:hash(path.join(row.directory,'episode.json.gz')),best_sha256:hash(path.join(row.directory,'best.json.gz'))});
        if(rows.length%14===0)console.log(JSON.stringify({episodes:rows.length,replayed_actions:replayed}));
      }
      assert.equal(expected.size,0);
      write(o.output,{schema_version:1,status:'passed',episodes:rows.length,replayed_actions:replayed,training_updates:updates.length,rows,
        report_sha256:hash(path.join(o.screen,'report.json')),checkpoints:Object.fromEntries(p.methods.filter(m=>m.startsWith('learned')).map(m=>[m,hash(path.join(o.screen,`${m}.pt`))])),
        scope:'Every evaluation action and best: ordinary membership, original coverage, native validity, independent metrics, exact incumbent hashes, physical pose movement, update/label membership'});
    }
    console.log(JSON.stringify({status:'passed',phase:o.phase,replayed_actions:replayed}));
  }finally{probe.close();}
}
main(args(['screen','validator','phase','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
