// Fresh original-graph validation and selected-action replay of the cold-runtime control.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {args,read,write,hash,Probe} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {transform} from '../../docs/benchmarks/layout-rl/geometry.mjs';
import {measure} from './full_factory_audit.mjs';
import {execute} from './full_factory_executor.mjs';
const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const cmp=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
const canonical=v=>Array.isArray(v)?v.map(canonical).sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b))):
  v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])])):v;
async function main(o){
  const p=read(path.join(o.screen,'protocol.json')),r=read(path.join(o.screen,'report.json'));
  assert.equal(r.status,'complete');assert.equal(r.policy_immutable,true);assert.equal(hash(path.join(o.screen,'protocol.json')),r.protocol_sha256);
  for(const [file,digest] of Object.entries({...p.sources,...p.source_sha256}))assert.equal(hash(file),digest,file);
  assert.equal(p.description_cache,'fresh runtime per episode');assert.equal(p.cases.length,1);
  const c=p.cases[0],root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},paths=read(p.catalogs),
    catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>[k,read(v)])),graph=prepareRoot(root,catalogs),
    probe=new Probe({...paths,validator:path.resolve(o.validator)}), rows=[];
  const expected=new Set(p.methods);assert.equal(expected.size,r.rows.length);
  try{
    for(const row of r.rows){
      assert(expected.delete(row.method));assert.equal(row.case,c.id);
      const ep=unpack(path.join(row.directory,'episode.json.gz')),best=unpack(path.join(row.directory,'best.json.gz'));
      assert(checkRoot(root,graph,best,await probe.check(best)).factory_validated);
      assert.deepEqual(measure(best,catalogs),row.best_metrics);assert.equal(ep.trace.length,row.selected);
      assert.equal(row.budget_complete,row.selected===p.horizon&&!ep.trace.some(t=>t.status==='late'));
      let current=transform(root.witness),incumbent=current,completed=0,improvingPoses=0;
      for(const [i,t] of ep.trace.entries()){
        assert.equal(t.selected,i+1);assert(t.route_expansions<=p.route_work);
        const result=t.action.kind==='restore_best'?{status:'candidate',witness:incumbent,route_expansions:0}:execute(current,catalogs,t.action,p.route_work);
        assert.equal(result.status,t.executor_status);assert.equal(result.route_expansions,t.route_expansions);
        const valid=Boolean(result.witness&&checkRoot(root,graph,result.witness,await probe.check(result.witness)).factory_validated);
        const metrics=valid?measure(result.witness,catalogs):null;
        if(!['late','budget_exhausted'].includes(t.status)){
          const comparison=valid?cmp(metrics.score,measure(incumbent,catalogs).score):null;
          const status=!result.witness?'executor_rejected':!valid?'rejected':comparison<0?'improved':
            cmp(metrics.score,measure(current,catalogs).score)===0?'accepted_equal':'accepted_exploration';
          assert.equal(t.status,status);completed++;
          if(valid){if(t.status==='improved'&&t.action.kind==='pose')improvingPoses++;current=transform(result.witness);if(comparison<0)incumbent=current;}
        }
        assert.deepEqual(measure(current,catalogs).score,t.current_score);assert.deepEqual(measure(incumbent,catalogs).score,t.best_score);
      }
      assert.deepEqual(canonical(incumbent),canonical(best));
      rows.push({method:row.method,factory_validated:true,score:row.best_metrics.score,selected:row.selected,
        completed_actions_replayed:completed,improving_poses:improvingPoses,best_sha256:hash(path.join(row.directory,'best.json.gz')),
        episode_sha256:hash(path.join(row.directory,'episode.json.gz'))});
    }
  }finally{probe.close();}
  write(o.output,{schema_version:1,status:'passed',rows,report_sha256:hash(path.join(o.screen,'report.json')),
    scope:'All-best original coverage/metrics and selected-action executor replay; completed transition status and incumbent sequence; vocabulary membership is not independently re-enumerated'});
  console.log(JSON.stringify({status:'passed',rows}));
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e);process.exitCode=1;});
