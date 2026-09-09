// Reenumerate membership and independently replay every large-root teacher trial.
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
const canonical=v=>Array.isArray(v)?v.map(canonical).sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b))):
  v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])])):v;
function category(a,w){
  if(a.kind!=='pose')return null;
  const rotating=a.poses.some(q=>q.rotation!==w.placements[q.member].rotation);
  if(a.poses.length>1)return rotating?'compound_rotation':'group_translation';
  if(rotating)return 'single_rotation';
  const q=a.poses[0],p=w.placements[q.member];return Math.abs(q.x-p.x)+Math.abs(q.y-p.y)<=2?'local_single':'distant_single';
}
function movement(before,after,action){
  const selected=new Set(action.poses.map(q=>before.placements[q.member].instance)),now=new Map(after.placements.map(p=>[p.instance,p]));
  const fixed=before.placements.filter(p=>!selected.has(p.instance));assert(fixed.length);
  const reference=fixed[0],dx=now.get(reference.instance).x-reference.x,dy=now.get(reference.instance).y-reference.y;
  for(const p of fixed){const q=now.get(p.instance);assert.deepEqual([q.x-p.x,q.y-p.y,q.rotation-p.rotation],[dx,dy,0]);}
  return {translated:before.placements.filter(p=>selected.has(p.instance)&&(now.get(p.instance).x-p.x!==dx||now.get(p.instance).y-p.y!==dy)).length,
    rotated:before.placements.filter(p=>selected.has(p.instance)&&now.get(p.instance).rotation!==p.rotation).length,frame_shift:[dx,dy]};
}
async function main(o){
  const p=read(path.join(o.screen,'protocol.json')),r=read(path.join(o.screen,'report.json')),outcomes=unpack(path.join(o.screen,'outcomes.json.gz'));
  assert.equal(r.status,'complete');assert.equal(r.protocol_sha256,hash(path.join(o.screen,'protocol.json')));
  assert.equal(r.outcomes_sha256,hash(path.join(o.screen,'outcomes.json.gz')));assert.equal(r.selected,r.planned);assert.equal(outcomes.length,r.selected);
  assert.equal(p.distinct_originals,1);assert.equal(p.training_performed,false);assert.equal(p.cases.length,2);
  for(const [file,h] of Object.entries({...p.sources,...p.source_sha256}))assert.equal(hash(file),h,file);
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>[k,read(v)]));
  const probe=new Probe({...paths,validator:path.resolve(o.validator)}),states=[];let serial=0;
  try{
    for(const prepared of p.prepared){
      const c=prepared.case,root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},graph=prepareRoot(root,catalogs);
      assert.equal(hash(c.initial),c.initial_sha256);assert.equal(c.family_split,'train');assert.equal(c.evaluation_role,'fresh-size-development');
      assert(checkRoot(root,graph,root.witness,await probe.check(root.witness)).factory_validated);
      const initial=transform(root.witness),metrics=measure(initial,catalogs),description=describe(initial,catalogs),seen=new Set(),rows=outcomes.filter(row=>row.state===c.state);
      assert.deepEqual(metrics,prepared.initial_metrics);assert.equal(rows.length,prepared.actions.length);
      for(const [kind,report] of Object.entries(prepared.populations)){
        const population=description.actions.filter(a=>category(a,initial)===kind).length;
        assert.equal(population,report.population);assert.equal(report.selected,Math.min(report.quota,population));
        assert.equal(rows.filter(row=>row.stratum===kind).length,report.selected);
      }
      let best=initial;const candidates=new Map();let validCount=0,translatedCount=0,improved=0,work=0;
      for(const [i,row] of rows.entries()){
        assert.equal(row.id,serial++);assert.equal(row.completed,true);assert.deepEqual(row.action,prepared.actions[i].action);
        assert.equal(row.vocabulary_index,prepared.actions[i].vocabulary_index);assert(!seen.has(row.vocabulary_index));seen.add(row.vocabulary_index);
        assert.deepEqual(row.action,description.actions[row.vocabulary_index]);assert.equal(category(row.action,initial),row.stratum);
        const result=execute(initial,catalogs,row.action,p.config.route_work),t=row.trace;
        assert.equal(result.status,t.executor_status);assert.equal(result.route_expansions,t.route_expansions);assert(result.route_expansions<=p.config.route_work);work+=result.route_expansions;
        const valid=Boolean(result.witness&&checkRoot(root,graph,result.witness,await probe.check(result.witness)).factory_validated),
          candidate=valid?transform(result.witness):initial,metric=valid?measure(candidate,catalogs):null;
        const status=!result.witness?'executor_rejected':!valid?'rejected':cmp(metric.score,metrics.score)<0?'improved':
          cmp(metric.score,metrics.score)===0?'accepted_equal':'accepted_exploration';
        assert.equal(row.factory_validated,valid);assert.equal(t.status,status);assert.deepEqual(row.candidate_metrics,metric);
        assert.equal(digest(candidate),row.current_sha256);assert.deepEqual(t.current_score,valid?metric.score:metrics.score);
        assert.deepEqual(t.best_score,valid&&cmp(metric.score,metrics.score)<0?metric.score:metrics.score);
        if(valid){
          const moved=movement(initial,candidate,row.action);assert.deepEqual(row.displacement,moved);assert(moved.translated||moved.rotated);
          validCount++;if(moved.translated)translatedCount++;if(status==='improved')improved++;
          if(!candidates.has(row.stratum)||cmp(metric.score,candidates.get(row.stratum).score)<0)candidates.set(row.stratum,{row:row.id,score:metric.score,witness:candidate});
          if(cmp(metric.score,measure(best,catalogs).score)<0)best=candidate;
        }else assert.equal(row.displacement,null);
      }
      const summary=r.states.find(s=>s.state===c.state);assert.equal(validCount,summary.valid);assert.equal(translatedCount,summary.translated);assert.equal(work,summary.route_expansions);
      assert.deepEqual(measure(best,catalogs),summary.best_metrics);assert.deepEqual(canonical(best),canonical(unpack(summary.best_path)));
      assert.equal(candidates.size,Object.keys(summary.selected_candidates).length);
      for(const [kind,chosen] of Object.entries(summary.selected_candidates)){
        const actual=candidates.get(kind);assert.equal(actual.row,chosen.row);assert.deepEqual(actual.score,chosen.score);
        assert.deepEqual(canonical(actual.witness),canonical(unpack(chosen.path)));
      }
      states.push({state:c.state,replayed:rows.length,valid:validCount,translated:translatedCount,improved,route_expansions:work,best:summary.best_metrics.score});
      console.log(JSON.stringify(states.at(-1)));
    }
  }finally{probe.close();}
  assert.equal(serial,outcomes.length);
  write(o.output,{schema_version:1,status:'passed',states,report_sha256:hash(path.join(o.screen,'report.json')),
    scope:'All selected ordinary-vocabulary memberships and independent executor/native original-graph/metrics replay; exact per-trial current hashes and selected witnesses'});
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
