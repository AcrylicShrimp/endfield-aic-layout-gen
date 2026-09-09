// Admit all newly measured original contractions and verify inherited lesson identity.
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

async function main(o){
  const p=read(path.join(o.screen,'protocol.json')),a=read(path.join(o.screen,'admission.json')),lessons=read(path.join(o.screen,'lessons.json')),
    previous=read(path.join(p.prior_screen,'protocol.json')),priorLessons=read(path.join(p.prior_screen,'lessons.json')),
    priorAudit=read(path.join(p.prior_screen,'lessons-audit.json')),priorAdmission=read(path.join(p.prior_screen,'admission.json'));
  assert.equal(a.status,'complete');assert.equal(a.protocol_sha256,hash(path.join(o.screen,'protocol.json')));assert.equal(a.lessons_sha256,hash(path.join(o.screen,'lessons.json')));
  assert.equal(priorAudit.status,'passed');assert.equal(a.prior_admission_audit_sha256,hash(path.join(p.prior_screen,'lessons-audit.json')));
  assert.equal(priorAudit.admission_sha256,hash(path.join(p.prior_screen,'admission.json')));assert.equal(priorAdmission.lessons_sha256,hash(path.join(p.prior_screen,'lessons.json')));
  for(const [file,h] of Object.entries({...p.sources,...p.source_sha256}))assert.equal(hash(file),h,file);
  assert.deepEqual(lessons.slice(0,-1),priorLessons);assert.equal(a.inherited_lessons,priorLessons.length);
  assert.equal(a.lessons,lessons.length);assert.equal(a.targets,lessons.reduce((n,l)=>n+l.targets.length,0));
  for(const key of ['checkpoint','parent','cases','catalogs','native_library','schema','depth','width','large','original_large','methods'])assert.deepEqual(p[key],previous[key]);
  const {role_weights,...sameConfig}=p.config;assert.deepEqual(sameConfig,previous.config);
  assert.equal(hash(path.join(o.screen,'rehearsal-contrasts.json.gz')),hash(path.join(p.prior_screen,'rehearsal-contrasts.json.gz')));
  assert.equal(a.rehearsal_sha256,hash(path.join(o.screen,'rehearsal-contrasts.json.gz')));
  const roles=new Map();
  for(const l of lessons){
    assert.equal(l.split,'train');assert.equal(hash(l.initial),l.initial_sha256);
    roles.set(l.id,l.root===p.large?(l.ordinal===0?'initiating_pose':'continuation'):l.root===p.original_large?'original_contraction':'rehearsal');
  }
  assert.deepEqual(Object.fromEntries(roles),p.lesson_roles);
  const blockSize=Object.values(role_weights).reduce((n,v)=>n+v,0),lessonMap=new Map(lessons.map(l=>[l.id,l])),scheduleCounts=[];
  for(const seed of p.config.seeds){
    const order=p.schedules[String(seed)];assert.equal(order.length,p.config.updates);assert.equal(order.length%blockSize,0);
    const allCounts={},smallCounts={},continuations=[];
    for(let start=0;start<order.length;start+=blockSize){
      const block={};
      for(const [offset,step] of order.slice(start,start+blockSize).entries()){
        assert.equal(step.update,start+offset);const role=roles.get(step.lesson);assert(role);
        block[role]=(block[role]??0)+1;allCounts[role]=(allCounts[role]??0)+1;
        if(role==='continuation')continuations.push(lessonMap.get(step.lesson).ordinal);
        if(role==='rehearsal')smallCounts[step.lesson]=(smallCounts[step.lesson]??0)+1;
      }
      assert.deepEqual(block,role_weights);
    }
    const continuationCount=lessons.filter(l=>roles.get(l.id)==='continuation').length;
    assert.deepEqual(continuations,continuations.map((_,i)=>1+i%continuationCount));
    assert.equal(Object.keys(smallCounts).length,lessons.filter(l=>roles.get(l.id)==='rehearsal').length);
    assert(Math.max(...Object.values(smallCounts))-Math.min(...Object.values(smallCounts))<=1);
    scheduleCounts.push({seed,...allCounts});
  }
  const c=p.cases.find(c=>c.id===p.original_large),paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>[k,read(v)])),
    root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},graph=prepareRoot(root,catalogs),initial=transform(root.witness),metric=measure(initial,catalogs),
    d=describe(initial,catalogs),contractions=d.actions.map((action,i)=>({action,i})).filter(r=>r.action.kind==='contract'),rows=unpack(path.join(o.screen,'original-contractions.json.gz')),
    probe=new Probe({...paths,validator:path.resolve(o.validator)}),positives=[];
  assert.equal(a.original_contractions_sha256,hash(path.join(o.screen,'original-contractions.json.gz')));assert.equal(rows.length,contractions.length);assert.equal(a.original_contractions,rows.length);
  try{
    assert(checkRoot(root,graph,initial,await probe.check(initial)).factory_validated);
    for(const [i,row] of rows.entries()){
      assert.equal(row.id,`original-contraction-${i}`);assert.equal(row.vocabulary_index,contractions[i].i);assert.deepEqual(row.action,contractions[i].action);
      const t=row.result,result=execute(initial,catalogs,row.action,p.config.route_work);
      assert.deepEqual(t.action,row.action);assert.equal(t.selected,1);assert.equal(t.executor_status,result.status);assert.equal(t.route_expansions,result.route_expansions);
      const valid=Boolean(result.witness&&checkRoot(root,graph,result.witness,await probe.check(result.witness)).factory_validated),candidate=valid?transform(result.witness):initial,
        next=measure(candidate,catalogs),improved=valid&&cmp(next.score,metric.score)<0;
      const status=!result.witness?'executor_rejected':!valid?'rejected':improved?'improved':cmp(next.score,metric.score)===0?'accepted_equal':'accepted_exploration';
      assert.equal(row.completed,true);assert.equal(row.status,status);assert.equal(t.status,status);assert(t.elapsed_seconds<p.config.episode_seconds);
      assert.equal(t.current_sha256,digest(candidate));assert.deepEqual(t.current_score,next.score);assert.deepEqual(t.best_score,improved?next.score:metric.score);
      assert.deepEqual(row.best_metrics,improved?next:metric);if(improved)positives.push(row);
    }
    const minimum=positives.map(r=>r.best_metrics.score).sort(cmp)[0],targets=positives.filter(r=>cmp(r.best_metrics.score,minimum)===0),item=lessons.at(-1);
    assert.deepEqual(minimum,a.original_target_score);assert.deepEqual(targets.map(r=>r.id),a.original_target_ids);
    assert.equal(item.root,p.original_large);assert.equal(item.ordinal,0);assert.equal(item.initial,c.initial);assert.equal(item.current_sha256,digest(initial));
    for(const key of ['initial_metrics','episode_initial_metrics','episode_best_metrics'])assert.deepEqual(item[key],metric);
    assert.deepEqual(item.observation_context,[1,Math.log1p(metric.width),Math.log1p(metric.height),Math.log1p(initial.placements.length),1,1,1,1].map(Math.fround));
    assert.deepEqual(item.targets,targets.map(r=>({id:r.id,action:r.action,vocabulary_index:r.vocabulary_index})));assert.deepEqual(item.results,targets.map(r=>r.result));
    write(o.output,{schema_version:1,status:'passed',admission_sha256:hash(path.join(o.screen,'admission.json')),inherited_lessons:priorLessons.length,
      prior_admission_audit_sha256:hash(path.join(p.prior_screen,'lessons-audit.json')),new_replayed_actions:rows.length,new_targets:targets.length,original_target_score:minimum,schedule_counts:scheduleCounts,
      scope:'Prior audited lessons retained verbatim; every newly offered original contraction independently replayed for ordinary membership, original graph/native validity, metrics, exact witness hashes and tied targets; exact role quotas'});
    console.log(JSON.stringify({status:'passed',new_replayed_actions:rows.length,targets:targets.length,score:minimum,schedule_counts:scheduleCounts}));
  }finally{probe.close();}
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
