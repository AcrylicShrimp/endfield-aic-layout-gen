// Verify selected whole paths, new lesson contexts and exact curriculum quotas.
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
const equal=(a,b)=>JSON.stringify(sorted(a))===JSON.stringify(sorted(b));

async function main(o){
  const p=read(path.join(o.screen,'protocol.json')),a=read(path.join(o.screen,'admission.json')),lessons=read(path.join(o.screen,'lessons.json')),
    prior=read(path.join(p.prior_screen,'protocol.json')),inherited=read(path.join(p.prior_screen,'lessons.json')),
    priorAdmission=read(path.join(p.prior_screen,'admission.json')),priorProof=read(path.join(p.prior_screen,'lessons-audit.json'));
  assert.equal(a.status,'complete');assert.equal(a.protocol_sha256,hash(path.join(o.screen,'protocol.json')));assert.equal(a.lessons_sha256,hash(path.join(o.screen,'lessons.json')));
  assert.equal(priorProof.status,'passed');assert.equal(priorProof.admission_sha256,hash(path.join(p.prior_screen,'admission.json')));assert.equal(a.prior_admission_audit_sha256,hash(path.join(p.prior_screen,'lessons-audit.json')));
  assert.equal(priorAdmission.lessons_sha256,hash(path.join(p.prior_screen,'lessons.json')));assert.deepEqual(lessons.slice(0,inherited.length),inherited);
  assert.equal(a.inherited_lessons,inherited.length);assert.equal(a.new_lessons,lessons.length-inherited.length);assert.equal(a.lessons,lessons.length);
  assert.equal(a.targets,lessons.reduce((n,l)=>n+l.targets.length,0));assert.equal(new Set(lessons.map(l=>l.id)).size,lessons.length);
  for(const [file,h] of Object.entries({...p.sources,...p.source_sha256}))assert.equal(hash(file),h,file);
  for(const key of ['cases','catalogs','native_library','checkpoint','parent','schema','depth','width','large','original_large','methods'])assert.deepEqual(p[key],prior[key]);
  const permitted=new Set(['role_weights','updates','observation_cache_bytes']);
  assert.deepEqual(Object.fromEntries(Object.entries(p.config).filter(([k])=>!permitted.has(k))),Object.fromEntries(Object.entries(prior.config).filter(([k])=>!permitted.has(k))));
  assert.equal(hash(path.join(o.screen,'rehearsal-contrasts.json.gz')),hash(path.join(p.prior_screen,'rehearsal-contrasts.json.gz')));assert.equal(a.rehearsal_sha256,hash(path.join(o.screen,'rehearsal-contrasts.json.gz')));
  const roles=new Map(),lessonMap=new Map(lessons.map(l=>[l.id,l])),small=p.cases.filter(c=>c.split==='train'&&![p.large,p.original_large].includes(c.id));
  for(const l of lessons){
    assert.equal(l.split,'train');assert.equal(hash(l.initial),l.initial_sha256);
    roles.set(l.id,l.root===p.large?(l.ordinal===0?'initiating_pose':'continuation'):l.root===p.original_large?'original_contraction':l.ordinal===0?'small_initial':'small_followup');
  }
  assert.deepEqual(Object.fromEntries(roles),p.lesson_roles);
  const blockSize=Object.values(p.config.role_weights).reduce((n,v)=>n+v,0),scheduleCounts=[];
  for(const seed of p.config.seeds){
    const order=p.schedules[String(seed)],counts={},byRoot=new Map();assert.equal(order.length,p.config.updates);assert.equal(order.length%blockSize,0);
    for(let start=0;start<order.length;start+=blockSize){
      const block={};
      for(const [offset,step] of order.slice(start,start+blockSize).entries()){
        assert.equal(step.update,start+offset);const role=roles.get(step.lesson),item=lessonMap.get(step.lesson);assert(role);
        block[role]=(block[role]??0)+1;counts[role]=(counts[role]??0)+1;
        if(role==='small_followup'){
          if(!byRoot.has(item.root))byRoot.set(item.root,[]);byRoot.get(item.root).push(item.ordinal);
        }
      }
      assert.deepEqual(block,p.config.role_weights);
    }
    assert.equal(byRoot.size,small.length);const lengths=[...byRoot.values()].map(v=>v.length);assert(Math.max(...lengths)-Math.min(...lengths)<=1);
    for(const [root,ordinals] of byRoot){const size=lessons.filter(l=>l.root===root&&l.ordinal>0).length;assert.deepEqual(ordinals,ordinals.map((_,i)=>1+i%size));}
    const initialCounts=small.map(c=>order.filter(s=>lessonMap.get(s.lesson).root===c.id&&lessonMap.get(s.lesson).ordinal===0).length);
    assert(Math.max(...initialCounts)-Math.min(...initialCounts)<=1);scheduleCounts.push({seed,...counts});
  }
  const pool=[];
  for(const [source_order,screen] of p.teacher_screens.entries()){
    const result=read(path.join(screen,'report.json')),proof=read(path.join(screen,'audit.json'));assert.equal(result.status,'complete');assert.equal(proof.status,'passed');assert.equal(proof.report_sha256,hash(path.join(screen,'report.json')));
    for(const [row_order,row] of result.rows.entries())pool.push({...row,source_order,row_order,source_screen:screen});
  }
  assert.deepEqual(p.selections.map(s=>s.root),small.map(c=>c.id));
  for(const selection of p.selections){
    const candidates=pool.filter(r=>r.case===selection.root&&r.split==='train'&&r.budget_complete&&cmp(r.best_metrics.score,r.initial_metrics.score)<0)
      .sort((a,b)=>cmp(a.best_metrics.score,b.best_metrics.score)||a.source_order-b.source_order||a.row_order-b.row_order),chosen=candidates[0];assert(chosen);
    for(const key of ['source_screen','source_order','row_order','method','bank','directory'])assert.deepEqual(selection[key],chosen[key]);assert.deepEqual(selection.score,chosen.best_metrics.score);
  }
  const traces=unpack(path.join(o.screen,'teacher-traces.json.gz'));assert.equal(a.teacher_traces_sha256,hash(path.join(o.screen,'teacher-traces.json.gz')));assert.equal(traces.length,p.selections.length);
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>[k,read(v)])),probe=new Probe({...paths,validator:path.resolve(o.validator)}),rows=[];let actions=0;
  try{
    for(const [ordinal,selection] of p.selections.entries()){
      const c=small.find(c=>c.id===selection.root),root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},graph=prepareRoot(root,catalogs),
        initial=transform(root.witness),initialMetrics=measure(initial,catalogs),source=unpack(path.join(selection.directory,'episode.json.gz')),trace=traces[ordinal];
      assert.equal(trace.root,c.id);assert.equal(selection.prefix,source.trace.findIndex(t=>equal(t.best_score,selection.score))+1);assert.equal(trace.trace.length,selection.prefix);
      assert(checkRoot(root,graph,initial,await probe.check(initial)).factory_validated);
      let current=initial;
      for(const [i,t] of trace.trace.entries()){
        const recorded=source.trace[i],d=describe(current,catalogs),item=lessonMap.get(`${c.id}--lesson-${i}`),metric=measure(current,catalogs);
        assert.equal(t.selected,i+1);assert.equal(recorded.status,'improved');assert.equal(recorded.unchanged,false);assert.deepEqual(t.action,recorded.action);assert(item);
        assert(item.targets.some(target=>equal(target.action,t.action)));
        assert.equal(item.current_sha256,digest(current));assert.deepEqual(item.initial_metrics,metric);assert.deepEqual(item.episode_initial_metrics,initialMetrics);assert.deepEqual(item.episode_best_metrics,metric);
        const s=metric.score,original=initialMetrics.score;
        assert.deepEqual(item.observation_context,[1-i/p.config.horizon,Math.log1p(metric.width),Math.log1p(metric.height),Math.log1p(current.placements.length),s[0]/original[0],s[1]/original[1],s[0]/original[0],s[1]/original[1]].map(Math.fround));
        assert.equal(digest(transform(read(item.initial))),digest(current));
        const target=item.targets.find(target=>equal(target.action,t.action));assert.deepEqual(d.actions[target.vocabulary_index],t.action);
        if(i){assert.equal(item.targets.length,1);assert.deepEqual(item.results,[t]);}
        const result=execute(current,catalogs,t.action,p.config.route_work);assert.equal(result.status,t.executor_status);assert.equal(result.route_expansions,t.route_expansions);assert(result.route_expansions<=p.config.route_work);
        assert(checkRoot(root,graph,result.witness,await probe.check(result.witness)).factory_validated);const next=transform(result.witness),nextMetrics=measure(next,catalogs);assert(cmp(nextMetrics.score,metric.score)<0);
        for(const key of ['status','executor_status','route_expansions','current_score','best_score'])assert.deepEqual(t[key],recorded[key]);
        assert.equal(t.status,'improved');assert(t.elapsed_seconds<p.config.episode_seconds);assert.equal(digest(next),t.current_sha256);assert.deepEqual(nextMetrics.score,t.current_score);assert.deepEqual(nextMetrics.score,t.best_score);
        current=next;actions++;
      }
      assert.deepEqual(measure(current,catalogs).score,selection.score);assert.equal(digest(current),source.best_sha256);assert.equal(digest(current),trace.best_sha256);
      assert.equal(digest(current),digest(unpack(path.join(selection.directory,'best.json.gz'))));
      rows.push({root:c.id,selected:trace.trace.length,score:selection.score,best_sha256:digest(current)});
    }
    assert.equal(actions,a.teacher_actions);
    assert.equal(lessons.filter(l=>roles.get(l.id)==='small_followup').length,actions-p.selections.length);
    write(o.output,{schema_version:1,status:'passed',admission_sha256:hash(path.join(o.screen,'admission.json')),prior_admission_audit_sha256:hash(path.join(p.prior_screen,'lessons-audit.json')),
      inherited_lessons:inherited.length,new_lessons:a.new_lessons,replayed_actions:actions,paths:rows,schedule_counts:scheduleCounts,
      scope:'Best measured complete training-root paths; all selected transitions under full original graph/native validity/metrics/exact hashes; inherited target identity, new episode contexts and exact role quotas'});
    console.log(JSON.stringify({status:'passed',replayed_actions:actions,lessons:lessons.length,schedule_counts:scheduleCounts}));
  }finally{probe.close();}
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
