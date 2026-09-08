// Independently replay teacher lessons and validate all retained family-transfer bests.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { gunzipSync } from 'node:zlib';
import { args, read, write, hash, Probe } from '../../docs/benchmarks/layout-rl/io.mjs';
import { prepareRoot, checkRoot } from '../../docs/benchmarks/layout-rl/coverage.mjs';
import { transform } from '../../docs/benchmarks/layout-rl/geometry.mjs';
import { describe, execute } from './full_factory_executor.mjs';
import { measure } from './full_factory_audit.mjs';

const unpack = p => JSON.parse(gunzipSync(fs.readFileSync(p)));
const canonical = v => Array.isArray(v) ? v.map(canonical).sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b))) :
  v && typeof v === 'object' ? Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])])) : v;
const equal = (a,b) => assert.deepEqual(canonical(a),canonical(b));
const compare = (a,b) => { const i=a.findIndex((v,i)=>v!==b[i]); return i<0?0:Math.sign(a[i]-b[i]); };

async function main(o) {
  const directory=path.resolve(o.screen),p=read(path.join(directory,'protocol.json')),r=read(path.join(directory,'report.json'));
  assert.equal(r.status,'complete');assert.equal(hash(path.join(directory,'protocol.json')),r.protocol_sha256);
  for(const [file,expected] of Object.entries(p.sources))assert.equal(hash(file),expected,file);
  for(const [file,expected] of Object.entries(p.source_sha256))assert.equal(hash(file),expected,file);
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,f])=>[k,read(f)]));
  const cases=new Map(p.cases.map(c=>[c.id,c])),roots=new Map(p.cases.map(c=>{
    const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)};
    return[c.id,{root,graph:prepareRoot(root,catalogs)}];
  }));
  const lessons=read(path.join(directory,'lessons.json')).lessons,updates=read(path.join(directory,'updates.json'));
  assert.equal(lessons.length,r.lessons);assert.equal(updates.length,p.plan.updates);assert.equal(r.updates,p.plan.updates);
  const lessonIds=new Map(lessons.map(l=>[l.id,l]));assert.equal(lessonIds.size,lessons.length);
  for(const [i,u] of updates.entries()) {
    assert.equal(u.update,i);assert.equal(lessonIds.get(u.lesson).split,'train');
    assert(Number.isFinite(u.loss)&&Number.isFinite(u.gradient_norm));
  }
  const probe=new Probe({...paths,validator:path.resolve(o.validator)},5000),audited=[],previous=new Map();
  const check=async(id,w)=>{const {root,graph}=roots.get(id);const validation=await probe.check(w);
    const coverage=checkRoot(root,graph,w,validation);assert(coverage.factory_validated,JSON.stringify({id,coverage}));};
  try {
    for(const l of lessons) {
      const c=cases.get(l.root),w=read(l.initial);assert.equal(hash(l.initial),l.initial_sha256);
      assert.equal(l.family,c.family);assert.equal(l.split,c.split);
      const prior=previous.get(l.root);assert.equal(l.ordinal,prior?prior.ordinal+1:0);
      equal(w,prior?prior.witness:transform(roots.get(l.root).root.witness));
      await check(l.root,w);assert.deepEqual(measure(w,catalogs),l.initial_metrics);
      assert.deepEqual(measure(roots.get(l.root).root.witness,catalogs),l.episode_initial_metrics);
      assert.deepEqual(l.episode_best_metrics,l.initial_metrics);
      const metrics=l.initial_metrics,initial=l.episode_initial_metrics;
      const context=[1-l.ordinal/p.plan.horizon,Math.log1p(metrics.width),Math.log1p(metrics.height),Math.log1p(w.placements.length),
        metrics.score[0]/Math.max(1,initial.score[0]),metrics.score[1]/Math.max(1,initial.score[1]),
        metrics.score[0]/Math.max(1,initial.score[0]),metrics.score[1]/Math.max(1,initial.score[1])].map(Math.fround);
      assert.deepEqual(l.observation_context,context);
      const description=describe(w,catalogs);assert(description.actions.some(a=>JSON.stringify(a)===JSON.stringify(l.action)));
      const result=execute(w,catalogs,l.action,p.plan.route_work);assert.equal(result.status,'candidate');
      await check(l.root,result.witness);assert.deepEqual(measure(result.witness,catalogs),l.result_metrics);
      assert(compare(l.result_metrics.score,l.initial_metrics.score)<0);
      previous.set(l.root,{ordinal:l.ordinal,witness:transform(result.witness)});
    }
    const expected=new Set(['untrained','learned','uniform','geometry'].flatMap(method=>p.cases.flatMap(c=>p.plan.banks.map(bank=>`${method}:${c.id}:${bank}`))));
    assert.equal(r.rows.length,expected.size);
    for(const row of r.rows) {
      const key=`${row.method}:${row.case}:${row.bank}`;assert(expected.delete(key),key);
      const c=cases.get(row.case);assert.equal(row.family,c.family);assert.equal(row.split,c.split);
      const w=unpack(path.join(row.directory,'best.json.gz')),episode=unpack(path.join(row.directory,'episode.json.gz'));
      await check(row.case,w);assert.deepEqual(measure(w,catalogs),row.best_metrics);
      assert.deepEqual(measure(roots.get(row.case).root.witness,catalogs).score,row.initial_metrics.score);
      assert(compare(row.best_metrics.score,row.initial_metrics.score)<=0);
      assert.equal(episode.trace.length,row.selected);assert(row.selected<=p.plan.horizon);
      assert.equal(row.budget_complete,row.selected===p.plan.horizon&&!episode.trace.some(t=>t.status==='late'));
      let best=row.initial_metrics.score;
      for(const [i,t] of episode.trace.entries()) {
        assert.equal(t.selected,i+1);assert(t.route_expansions<=p.plan.route_work);
        assert(compare(t.best_score,best)<=0);best=t.best_score;
        assert(!t.diagnostics.some(d=>d.code==='factory.executor-error'));
      }
      assert.deepEqual(best,row.best_metrics.score);assert.deepEqual(episode.best_metrics,row.best_metrics);
      audited.push({key,factory_validated:true,score:row.best_metrics.score,selected:row.selected,
        best_sha256:hash(path.join(row.directory,'best.json.gz')),episode_sha256:hash(path.join(row.directory,'episode.json.gz'))});
    }
    assert.equal(expected.size,0);
    write(o.output,{schema_version:1,status:'passed',teacher_replayed:lessons.length,training_updates:updates.length,
      episodes:audited.length,rows:audited,scope:'Teacher trajectory replay and all-best original-graph coverage/independent metrics; no evaluation action replay claim',
      report_sha256:hash(path.join(directory,'report.json')),validator_sha256:hash(o.validator),
      checkpoints:Object.fromEntries(['untrained.pt','learned.pt'].map(f=>[f,hash(path.join(directory,f))]))});
    console.log(JSON.stringify({status:'passed',lessons:lessons.length,episodes:audited.length}));
  } finally {probe.close();}
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
