// Replay every charged contraction and establish exhaustion of the ordinary operator.
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
const sorted=v=>Array.isArray(v)?v.map(sorted):v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,sorted(v[k])])):v;
const digest=v=>createHash('sha256').update(JSON.stringify(sorted(v))).digest('hex');
async function main(o){
  const p=read(path.join(o.screen,'protocol.json')),r=read(path.join(o.screen,'report.json')),
    e=JSON.parse(gunzipSync(fs.readFileSync(path.join(o.screen,'preparation.json.gz'))));
  assert.equal(r.status,'contraction_exhausted');assert.equal(r.protocol_sha256,hash(path.join(o.screen,'protocol.json')));
  assert.equal(r.preparation_sha256,hash(path.join(o.screen,'preparation.json.gz')));assert.equal(r.selected,e.trace.length);
  assert.equal(r.remaining_contractions,0);assert(r.selected<=p.proposals);assert(r.seconds<p.seconds);
  for(const [file,h] of Object.entries({...p.sources,...p.source_sha256}))assert.equal(hash(file),h,file);
  const c=p.case,root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},paths=read(p.catalogs),
    catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>[k,read(v)])),graph=prepareRoot(root,catalogs),
    probe=new Probe({...paths,validator:path.resolve(o.validator)}),states=[];
  let current=transform(root.witness);
  try{
    assert(checkRoot(root,graph,current,await probe.check(current)).factory_validated);
    assert.deepEqual(measure(current,catalogs),r.initial_metrics);
    for(const [i,t] of e.trace.entries()){
      const d=describe(current,catalogs),choices=d.actions.filter(a=>a.kind==='contract');
      assert.equal(choices.length,t.available_contractions);assert.deepEqual(t.action,choices[0]);assert.deepEqual(t.action,d.actions[t.vocabulary_index]);
      const result=execute(current,catalogs,t.action,p.route_work);assert.equal(result.status,t.executor_status);
      assert.equal(result.route_expansions,t.route_expansions);assert.equal(result.route_expansions,0);
      assert(checkRoot(root,graph,result.witness,await probe.check(result.witness)).factory_validated);
      const next=transform(result.witness),metric=measure(next,catalogs);assert(metric.score[0]<measure(current,catalogs).score[0]);
      assert.equal(t.status,'improved');assert.equal(t.selected,i+1);assert.deepEqual(metric.score,t.current_score);assert.deepEqual(metric.score,t.best_score);
      assert.equal(digest(next),t.current_sha256);assert(t.elapsed_seconds<p.seconds);current=next;
      states.push({selected:i+1,axis:t.action.axis,score:metric.score,sha256:t.current_sha256});
      if(states.length%32===0)console.log(JSON.stringify({replayed:states.length,score:metric.score}));
    }
    assert.equal(describe(current,catalogs).actions.filter(a=>a.kind==='contract').length,0);
    assert.deepEqual(current,read(path.join(o.screen,'best.json')));assert.deepEqual(measure(current,catalogs),r.best_metrics);
    assert.equal(hash(r.case.initial),r.case.initial_sha256);assert.equal(r.case.original,c.original);assert.equal(r.case.request,c.request);
    assert.deepEqual(read(path.join(o.screen,'inputs.json')).rows,[r.case]);
  }finally{probe.close();}
  write(o.output,{schema_version:1,status:'passed',replayed:states.length,remaining_contractions:0,states,
    report_sha256:hash(path.join(o.screen,'report.json')),scope:'Full original graph and independent metrics at every state; first ordinary contraction membership; final zero-contraction operator proof'});
  console.log(JSON.stringify({status:'passed',replayed:states.length}));
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
