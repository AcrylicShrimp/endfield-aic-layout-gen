// Replay every collected action; independently establish original-graph ranking labels.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {args,read,write,hash,Probe} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {transform} from '../../docs/benchmarks/layout-rl/geometry.mjs';
import {describe,execute} from './full_factory_executor.mjs';
import {measure} from './full_factory_audit.mjs';
const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const canonical=v=>Array.isArray(v)?v.map(canonical).sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b))):
  v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])])):v;
const compare=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
async function main(o){
  const dir=path.resolve(o.collection),p=read(path.join(dir,'protocol.json')),r=read(path.join(dir,'contrasts.json'));
  assert.equal(r.status,'measured');assert.equal(hash(path.join(dir,'protocol.json')),r.protocol_sha256);
  assert.equal(hash(path.join(p.parent,'protocol.json')),p.parent_protocol_sha256);
  for(const [f,h]of Object.entries(p.source_sha256))assert.equal(hash(f),h,f);
  for(const [f,h]of Object.entries(read(path.join(p.parent,'protocol.json')).sources))assert.equal(hash(f),h,f);
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,f])=>[k,read(f)]));
  const probe=new Probe({...paths,validator:path.resolve(o.validator)},5000),audited=[],ids=new Set();
  try{
    assert.equal(r.cases.length,p.cases.length);
    for(const c of p.cases){
      const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},graph=prepareRoot(root,catalogs);
      const w=transform(root.witness),metrics=measure(w,catalogs),d=describe(w,catalogs);
      assert(checkRoot(root,graph,w,await probe.check(w)).factory_validated,c.id);
      const local=r.rows.filter(row=>row.root===c.id),summary=r.cases.find(row=>row.case===c.id);
      assert.equal(local.length,summary.selected);assert.equal(d.actions.length,summary.offered);
      assert.deepEqual(read(path.join(dir,c.id+'-attempts.json')),local);
      const actions=new Set();
      for(const row of local){
        assert(!ids.has(row.id));ids.add(row.id);assert.equal(row.split,c.split);assert.equal(row.family,c.family);
        const key=JSON.stringify(row.action);assert(!actions.has(key));actions.add(key);
        assert(d.actions.some(a=>JSON.stringify(a)===key),row.id);assert.deepEqual(row.initial_metrics,metrics);
        const replay=execute(w,catalogs,row.action,p.route_work);
        assert.equal(replay.status,row.result.executor_status,row.id);assert.equal(replay.route_expansions,row.result.route_expansions);
        assert(replay.route_expansions<=p.route_work);assert(!replay.diagnostics?.some(d=>d.code==='factory.executor-error'));
        const valid=Boolean(replay.witness&&checkRoot(root,graph,replay.witness,await probe.check(replay.witness)).factory_validated);
        const candidate=valid?measure(replay.witness,catalogs):null;
        const completed=!['late','budget_exhausted'].includes(row.result.status);
        assert.equal(row.completed,completed);
        const improving=completed&&valid&&compare(candidate.score,metrics.score)<0;
        assert.equal(row.improving,improving,row.id);
        if(completed){
          const expected=!replay.witness?'executor_rejected':!valid?'rejected':improving?'improved':
            compare(candidate.score,metrics.score)===0?'accepted_equal':'accepted_exploration';
          assert.equal(row.result.status,expected,row.id);
          assert.deepEqual(row.result.current_score,valid?candidate.score:metrics.score);
        }
        assert.deepEqual(row.best_metrics,improving?candidate:metrics);
        if(improving){
          assert.equal(hash(row.best),row.best_sha256);const saved=unpack(row.best);
          assert.deepEqual(canonical(saved),canonical(transform(replay.witness)));
          assert(checkRoot(root,graph,saved,await probe.check(saved)).factory_validated);
        }
        audited.push({id:row.id,root:c.id,split:c.split,completed,improving,candidate_validated:valid});
      }
    }
    assert.equal(ids.size,r.rows.length);
    write(o.output,{schema_version:1,status:'passed',replayed:audited.length,improving:audited.filter(r=>r.improving).length,
      rows:audited,scope:'Every selected action replayed with original-graph coverage and independent full-score labels',
      protocol_sha256:hash(path.join(dir,'protocol.json')),contrasts_sha256:hash(path.join(dir,'contrasts.json')),
      validator_sha256:hash(o.validator)});
    console.log(JSON.stringify({status:'passed',replayed:audited.length,improving:audited.filter(r=>r.improving).length}));
  }finally{probe.close();}
}
main(args(['collection','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
