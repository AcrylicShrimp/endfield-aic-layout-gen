import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { gunzipSync } from 'node:zlib';
import { args,read,write,hash,Probe } from '../../docs/benchmarks/layout-rl/io.mjs';
import { prepareRoot,checkRoot } from '../../docs/benchmarks/layout-rl/coverage.mjs';
import { measure } from './full_factory_audit.mjs';
import { transform } from '../../docs/benchmarks/layout-rl/geometry.mjs';
import { describe,execute } from './full_factory_executor.mjs';
const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const canonical=v=>Array.isArray(v)?v.map(canonical).sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b))):
  v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])])):v;
const compare=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
async function main(o){
  const dir=path.resolve(o.screen),p=read(path.join(dir,'protocol.json')),r=read(path.join(dir,'report.json'));
  assert.equal(r.status,'complete');assert.equal(r.policy_immutable,true);assert.equal(hash(path.join(dir,'protocol.json')),r.protocol_sha256);
  for(const [file,expected]of Object.entries(p.sources))assert.equal(hash(file),expected,file);
  for(const [file,expected]of Object.entries(p.source_sha256))assert.equal(hash(file),expected,file);
  const admission=read(p.admission),inputs=read(p.inputs);
  assert.equal(admission.status,'measured');assert.equal(r.attempted,admission.attempted);assert.equal(p.attempted,admission.attempted);
  assert.equal(r.accepted,p.cases.length);assert.equal(r.accepted,admission.accepted);
  const admitted=admission.rows.filter(r=>r.factory_validated).map(r=>r.id).sort();
  assert.deepEqual(p.cases.map(c=>c.id).sort(),admitted);assert.deepEqual(inputs.rows.map(c=>c.id).sort(),admitted);
  for(const c of p.cases){
    const a=admission.rows.find(r=>r.id===c.id);assert.equal(c.family,a.family);assert.equal(c.family_split,a.split);
    assert.equal(c.instances,a.facilities);assert.equal(c.evaluation_role,'fresh-size-development');
    for(const [key,name]of [['initial','initial.json'],['request','request.json'],['original','original-request.json']])
      assert.equal(hash(c[key]),c.files_sha256[name]);
  }
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,f])=>[k,read(f)]));
  const roots=new Map(p.cases.map(c=>{const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)};
    return[c.id,{root,graph:prepareRoot(root,catalogs),family:c.family}];}));
  const expected=new Set(p.methods.flatMap(method=>p.cases.flatMap(c=>p.banks.map(b=>`${method}:${c.id}:${b}`))));
  assert.equal(r.rows.length,expected.size);const probe=new Probe({...paths,validator:path.resolve(o.validator)},5000),rows=[],replays=[];
  try{
    for(const row of r.rows){
      const key=`${row.method}:${row.case}:${row.bank}`;assert(expected.delete(key),key);
      const {root,graph,family}=roots.get(row.case);assert.equal(row.family,family);
      const c=p.cases.find(c=>c.id===row.case);assert.equal(row.instances,c.instances);assert.equal(row.family_split,c.family_split);assert.equal(row.evaluation_role,c.evaluation_role);
      const w=unpack(path.join(row.directory,'best.json.gz')),e=unpack(path.join(row.directory,'episode.json.gz'));
      const coverage=checkRoot(root,graph,w,await probe.check(w));assert(coverage.factory_validated,JSON.stringify(coverage));
      assert.deepEqual(measure(w,catalogs),row.best_metrics);assert.deepEqual(measure(root.witness,catalogs).score,row.initial_metrics.score);
      assert(compare(row.best_metrics.score,row.initial_metrics.score)<=0);assert.equal(e.trace.length,row.selected);assert(row.selected<=p.horizon);
      assert.equal(row.budget_complete,row.selected===p.horizon&&!e.trace.some(t=>t.status==='late'));
      let best=row.initial_metrics.score;const improvements={};
      for(const [i,t]of e.trace.entries()){
        if(t.status==='improved')improvements[t.action.kind]=(improvements[t.action.kind]??0)+1;
        assert.equal(t.selected,i+1);assert(t.route_expansions<=p.route_work);assert(compare(t.best_score,best)<=0);best=t.best_score;
        assert(!t.diagnostics.some(d=>d.code==='factory.executor-error'));
      }
      assert.deepEqual(improvements,row.improvements);assert.deepEqual(best,row.best_metrics.score);assert.deepEqual(e.best_metrics,row.best_metrics);
      if(row.improvements.pose||row.improvements.reroute){
        let current=transform(root.witness),incumbent=current;const operations=[];
        for(const [ordinal,t]of e.trace.entries()){
          assert(!['late','budget_exhausted'].includes(t.status),'Selected capability replay requires completed transitions');
          const d=describe(current,catalogs);assert(d.actions.some(a=>JSON.stringify(a)===JSON.stringify(t.action)),key+':'+ordinal);
          const result=t.action.kind==='restore_best'?{status:'candidate',witness:incumbent,route_expansions:0}:execute(current,catalogs,t.action,p.route_work);
          assert.equal(result.status,t.executor_status,key+':'+ordinal);assert.equal(result.route_expansions,t.route_expansions);
          const valid=Boolean(result.witness&&checkRoot(root,graph,result.witness,await probe.check(result.witness)).factory_validated);
          const metric=valid?measure(result.witness,catalogs):null,comparison=valid?compare(metric.score,measure(incumbent,catalogs).score):null;
          const status=!result.witness?'executor_rejected':!valid?'rejected':comparison<0?'improved':
            compare(metric.score,measure(current,catalogs).score)===0?'accepted_equal':'accepted_exploration';
          assert.equal(status,t.status,key+':'+ordinal);
          if(valid){
            if(t.status==='improved'&&['pose','reroute'].includes(t.action.kind)){
              const moved=current.placements.filter((p,i)=>['x','y','rotation'].some(k=>p[k]!==result.witness.placements[i][k])).length;
              if(t.action.kind==='pose')assert(moved>0,'Improving pose must move or rotate a facility');
              operations.push({selected:t.selected,kind:t.action.kind,moved_facilities:moved,before:measure(current,catalogs).score,after:metric.score});
            }
            current=transform(result.witness);if(comparison<0)incumbent=current;
          }
          assert.deepEqual(measure(current,catalogs).score,t.current_score);assert.deepEqual(measure(incumbent,catalogs).score,t.best_score);
        }
        assert.deepEqual(canonical(incumbent),canonical(w));
        replays.push({key,selected:e.trace.length,operations});
      }
      rows.push({key,factory_validated:true,score:row.best_metrics.score,best_sha256:hash(path.join(row.directory,'best.json.gz')),
        episode_sha256:hash(path.join(row.directory,'episode.json.gz'))});
    }
    assert.equal(expected.size,0);write(o.output,{schema_version:1,status:'passed',episodes:rows.length,rows,
      attempted:admission.attempted,accepted:admission.accepted,checkpoints:Object.fromEntries(Object.entries(p.checkpoints).map(([k,f])=>[k,hash(f)])),
      capability_replays:replays,replayed_actions:replays.reduce((n,r)=>n+r.selected,0),
      scope:'Admission membership and all-best fresh original coverage/metrics; full action replay for every episode with an improving pose/reroute',report_sha256:hash(path.join(dir,'report.json'))});
    console.log(JSON.stringify({status:'passed',episodes:rows.length}));
  }finally{probe.close();}
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
