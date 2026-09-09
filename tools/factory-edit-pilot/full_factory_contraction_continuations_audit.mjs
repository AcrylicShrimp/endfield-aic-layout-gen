// Replay measured first edits and every ensuing ordinary contraction on the full graph.
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
const sorted=v=>Array.isArray(v)?v.map(sorted):v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,sorted(v[k])])):v;
const digest=v=>createHash('sha256').update(JSON.stringify(sorted(v))).digest('hex');
const canonical=v=>Array.isArray(v)?v.map(canonical).sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b))):
  v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])])):v;
const cmp=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
async function main(o){
  const p=read(path.join(o.screen,'protocol.json')),r=read(path.join(o.screen,'report.json')),trajectories=unpack(path.join(o.screen,'trajectories.json.gz')),
    parent=read(path.join(p.parent,'report.json')),parentAudit=read(path.join(p.parent,'audit.json')),rows=unpack(path.join(p.parent,'outcomes.json.gz'));
  assert.equal(r.status,'complete');assert.equal(hash(path.join(o.screen,'protocol.json')),r.protocol_sha256);assert.equal(hash(path.join(o.screen,'trajectories.json.gz')),r.trajectories_sha256);
  assert.equal(parentAudit.status,'passed');assert.equal(parentAudit.report_sha256,hash(path.join(p.parent,'report.json')));assert.equal(hash(path.join(p.parent,'outcomes.json.gz')),parent.outcomes_sha256);
  const firsts=rows.filter(row=>row.completed&&row.factory_validated&&!['insert','restore_best'].includes(row.action.kind));
  assert.deepEqual(p.first_action_ids,firsts.map(row=>row.id));assert.equal(trajectories.length,firsts.length);assert.equal(r.planned,firsts.length);assert.equal(r.trajectories,firsts.length);
  for(const [file,h] of Object.entries({...p.sources,...p.source_sha256}))assert.equal(hash(file),h,file);
  const c=p.case,root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},paths=read(p.catalogs),
    catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>[k,read(v)])),graph=prepareRoot(root,catalogs),initial=transform(root.witness),
    initialMetrics=measure(initial,catalogs),initialDescription=describe(initial,catalogs),probe=new Probe({...paths,validator:path.resolve(o.validator)}),retained=new Map(),results=[];
  assert.equal(initialDescription.actions.filter(a=>a.kind==='contract').length,0);assert.deepEqual(initialMetrics,p.initial_metrics);assert.deepEqual(initialMetrics,r.initial_metrics);
  let best=initial,selected=0;
  try{
    assert(checkRoot(root,graph,initial,await probe.check(initial)).factory_validated);
    for(const [ordinal,row] of trajectories.entries()){
      const source=firsts[ordinal],summary=row.summary;assert.equal(row.id,ordinal);assert.equal(row.source_id,source.id);assert.equal(row.completed,true);
      assert.equal(row.first_kind,source.action.kind);assert.equal(row.first_stratum,source.stratum);assert.equal(summary.trace.length,summary.selected);assert(summary.selected<=p.config.horizon);assert(summary.elapsed_seconds<p.config.episode_seconds);
      let current=initial,incumbent=initial,exposed=null;
      for(const [i,t] of summary.trace.entries()){
        const d=i===0?initialDescription:describe(current,catalogs),contractions=d.actions.filter(a=>a.kind==='contract');
        if(i===0){assert.deepEqual(t.action,source.action);assert.deepEqual(d.actions[source.vocabulary_index],t.action);}
        else{
          if(i===1)exposed=contractions.length;
          assert.deepEqual(t.action,contractions[0]);assert.deepEqual(t.action,d.actions[t.vocabulary_index]);assert.equal(t.available_contractions,contractions.length);
        }
        const result=execute(current,catalogs,t.action,p.config.route_work);assert.equal(result.status,t.executor_status);assert.equal(result.route_expansions,t.route_expansions);assert(result.route_expansions<=p.config.route_work);
        assert(checkRoot(root,graph,result.witness,await probe.check(result.witness)).factory_validated);
        const next=transform(result.witness),metric=measure(next,catalogs),improved=cmp(metric.score,measure(incumbent,catalogs).score)<0,
          status=improved?'improved':cmp(metric.score,measure(current,catalogs).score)===0?'accepted_equal':'accepted_exploration';
        assert.equal(t.status,status);assert.equal(t.selected,i+1);assert.equal(digest(next),t.current_sha256);assert(t.elapsed_seconds<p.config.episode_seconds);
        if(i===0){assert.equal(t.status,source.trace.status);assert.equal(t.current_sha256,source.current_sha256);assert.equal(t.route_expansions,source.trace.route_expansions);}
        else assert.equal(t.status,'improved');
        current=next;if(improved)incumbent=current;
        assert.deepEqual(t.current_score,metric.score);assert.deepEqual(t.best_score,measure(incumbent,catalogs).score);
      }
      const remaining=describe(current,catalogs).actions.filter(a=>a.kind==='contract').length;if(exposed===null)exposed=remaining;
      assert.equal(exposed,row.exposed_contractions);assert.equal(remaining,row.remaining_contractions);
      assert.equal(row.termination,remaining===0?'contraction_exhausted':'horizon');if(remaining)assert.equal(summary.selected,p.config.horizon);
      const metric=measure(incumbent,catalogs);assert.deepEqual(metric,summary.best_metrics);assert.equal(digest(incumbent),summary.best_sha256);
      if(!retained.has(row.first_kind)||cmp(metric.score,retained.get(row.first_kind).score)<0)retained.set(row.first_kind,{trajectory:row.id,source_id:source.id,score:metric.score,witness:incumbent});
      if(cmp(metric.score,measure(best,catalogs).score)<0)best=incumbent;
      selected+=summary.selected;results.push({trajectory:row.id,source_id:source.id,first_kind:row.first_kind,selected:summary.selected,
        exposed,remaining,score:metric.score,area_improved:metric.score[0]<initialMetrics.score[0]});
      if(results.length%24===0)console.log(JSON.stringify({trajectories:results.length,selected,best:measure(best,catalogs).score}));
    }
    assert.equal(selected,r.selected);assert.equal(results.filter(row=>row.exposed>0).length,r.exposed);assert.equal(results.filter(row=>row.area_improved).length,r.area_improved);
    assert.deepEqual(measure(best,catalogs),r.best_metrics);assert.deepEqual(canonical(best),canonical(unpack(path.join(o.screen,'best.json.gz'))));assert.equal(retained.size,Object.keys(r.retained).length);
    for(const [kind,chosen] of Object.entries(r.retained)){
      const actual=retained.get(kind);assert.equal(actual.trajectory,chosen.trajectory);assert.equal(actual.source_id,chosen.source_id);assert.deepEqual(actual.score,chosen.score);
      assert.deepEqual(canonical(actual.witness),canonical(unpack(chosen.path)));
    }
    assert.deepEqual(results.filter(row=>cmp(row.score,r.best_metrics.score)===0&&cmp(row.score,initialMetrics.score)<0).map(row=>row.trajectory),r.targets);
  }finally{probe.close();}
  write(o.output,{schema_version:1,status:'passed',trajectories:results.length,replayed_actions:selected,rows:results,
    report_sha256:hash(path.join(o.screen,'report.json')),scope:'All parent-valid first edits and ordinary contraction memberships; every original graph/metric/current hash/incumbent sequence; exposed and final contraction counts'});
  console.log(JSON.stringify({status:'passed',trajectories:results.length,replayed_actions:selected}));
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
