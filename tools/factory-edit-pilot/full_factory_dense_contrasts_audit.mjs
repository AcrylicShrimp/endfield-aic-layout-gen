// Complete independent replay of measured alternatives on a strip-exhausted original.
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
function stratum(a,w){
  if(a.kind!=='pose')return a.kind;
  const rotation=a.poses.some(q=>q.rotation!==w.placements[q.member].rotation);
  if(a.poses.length>1)return rotation?'compound_rotation':'group_translation';
  if(rotation)return 'single_rotation';
  const q=a.poses[0],p=w.placements[q.member];return Math.abs(q.x-p.x)+Math.abs(q.y-p.y)<=2?'local_single':'distant_single';
}
function displaced(before,after,action){
  if(action.kind!=='pose')return null;
  const ids=new Set(action.poses.map(q=>before.placements[q.member].instance)),now=new Map(after.placements.map(p=>[p.instance,p])),fixed=before.placements.filter(p=>!ids.has(p.instance));assert(fixed.length);
  const a=fixed[0],dx=now.get(a.instance).x-a.x,dy=now.get(a.instance).y-a.y;
  for(const p of fixed){const q=now.get(p.instance);assert.deepEqual([q.x-p.x,q.y-p.y,q.rotation-p.rotation],[dx,dy,0]);}
  return {translated:before.placements.filter(p=>ids.has(p.instance)&&(now.get(p.instance).x-p.x!==dx||now.get(p.instance).y-p.y!==dy)).length,
    rotated:before.placements.filter(p=>ids.has(p.instance)&&now.get(p.instance).rotation!==p.rotation).length,frame_shift:[dx,dy]};
}
async function main(o){
  const p=read(path.join(o.screen,'protocol.json')),r=read(path.join(o.screen,'report.json')),rows=unpack(path.join(o.screen,'outcomes.json.gz'));
  assert.equal(r.status,'complete');assert.equal(r.protocol_sha256,hash(path.join(o.screen,'protocol.json')));assert.equal(r.outcomes_sha256,hash(path.join(o.screen,'outcomes.json.gz')));
  assert.equal(r.selected,r.planned);assert.equal(rows.length,r.selected);assert.equal(rows.length,p.selections.length);
  for(const [file,h] of Object.entries({...p.sources,...p.source_sha256}))assert.equal(hash(file),h,file);
  const prep=read(path.join(p.preparation,'report.json')),prepAudit=read(path.join(p.preparation,'audit.json'));
  assert.equal(prep.status,'contraction_exhausted');assert.equal(prepAudit.status,'passed');assert.equal(prepAudit.report_sha256,hash(path.join(p.preparation,'report.json')));assert.deepEqual(p.case,prep.case);
  const c=p.case,root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},paths=read(p.catalogs),
    catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>[k,read(v)])),graph=prepareRoot(root,catalogs),initial=transform(root.witness),
    description=describe(initial,catalogs),metric=measure(initial,catalogs),probe=new Probe({...paths,validator:path.resolve(o.validator)}),seen=new Set(),retained=new Map(),outcomes=[];
  assert.equal(description.actions.filter(a=>a.kind==='contract').length,0);assert.deepEqual(metric,r.initial_metrics);assert.deepEqual(metric,p.initial_metrics);
  for(const [kind,pop] of Object.entries(p.populations)){
    assert.equal(description.actions.filter(a=>stratum(a,initial)===kind).length,pop.population);
    assert.equal(pop.selected,Math.min(pop.quota,pop.population));assert.equal(rows.filter(row=>row.stratum===kind).length,pop.selected);
  }
  let best=initial,work=0;
  try{
    assert(checkRoot(root,graph,initial,await probe.check(initial)).factory_validated);
    for(const [i,row] of rows.entries()){
      assert.equal(row.id,i);assert.equal(row.completed,true);assert.deepEqual(row.action,p.selections[i].action);assert.equal(row.vocabulary_index,p.selections[i].vocabulary_index);
      assert(!seen.has(row.vocabulary_index));seen.add(row.vocabulary_index);assert.deepEqual(description.actions[row.vocabulary_index],row.action);assert.equal(stratum(row.action,initial),row.stratum);
      const result=row.action.kind==='restore_best'?{status:'candidate',witness:initial,route_expansions:0}:execute(initial,catalogs,row.action,p.config.route_work),t=row.trace;
      assert.equal(result.status,t.executor_status);assert.equal(result.route_expansions,t.route_expansions);assert(result.route_expansions<=p.config.route_work);work+=result.route_expansions;
      const valid=Boolean(result.witness&&checkRoot(root,graph,result.witness,await probe.check(result.witness)).factory_validated),current=valid?transform(result.witness):initial,candidate=valid?measure(current,catalogs):null;
      const status=!result.witness?'executor_rejected':!valid?'rejected':cmp(candidate.score,metric.score)<0?'improved':cmp(candidate.score,metric.score)===0?'accepted_equal':'accepted_exploration';
      assert.equal(row.factory_validated,valid);assert.equal(status,t.status);assert.deepEqual(candidate,row.candidate_metrics);assert.equal(digest(current),row.current_sha256);
      const individualBest=valid&&cmp(candidate.score,metric.score)<0?candidate:metric;
      assert.deepEqual(row.best_metrics,individualBest);assert.deepEqual(t.best_score,individualBest.score);assert.deepEqual(t.current_score,valid?candidate.score:metric.score);
      assert.deepEqual(row.displacement,valid?displaced(initial,current,row.action):null);
      if(valid){
        if(row.action.kind==='pose')assert(row.displacement.translated||row.displacement.rotated);
        if(!retained.has(row.stratum)||cmp(candidate.score,retained.get(row.stratum).score)<0)retained.set(row.stratum,{row:i,score:candidate.score,witness:current});
        if(cmp(candidate.score,measure(best,catalogs).score)<0)best=current;
      }
      outcomes.push({id:i,stratum:row.stratum,factory_validated:valid,status,score:individualBest.score,displacement:row.displacement});
      if(outcomes.length%64===0)console.log(JSON.stringify({replayed:outcomes.length,best:measure(best,catalogs).score}));
    }
    assert.equal(work,r.route_expansions);assert.deepEqual(measure(best,catalogs),r.best_metrics);assert.deepEqual(canonical(best),canonical(unpack(path.join(o.screen,'best.json.gz'))));
    assert.equal(retained.size,Object.keys(r.retained).length);
    for(const [kind,chosen] of Object.entries(r.retained)){
      const actual=retained.get(kind);assert.equal(actual.row,chosen.row);assert.deepEqual(actual.score,chosen.score);assert.deepEqual(canonical(actual.witness),canonical(unpack(chosen.path)));
    }
    assert.deepEqual(outcomes.filter(row=>row.status==='improved'&&cmp(row.score,r.best_metrics.score)===0).map(row=>row.id),r.targets);
  }finally{probe.close();}
  write(o.output,{schema_version:1,status:'passed',replayed:rows.length,outcomes,targets:r.targets,remaining_initial_contractions:0,
    report_sha256:hash(path.join(o.screen,'report.json')),scope:'Every ordinary membership, executor work/status, native original graph, metric, current hash and retained witness; full-score target minima among measured actions'});
  console.log(JSON.stringify({status:'passed',replayed:rows.length,targets:r.targets}));
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
