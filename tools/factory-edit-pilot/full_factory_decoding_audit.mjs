import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { gunzipSync } from 'node:zlib';
import { args,read,write,hash,Probe } from '../../docs/benchmarks/layout-rl/io.mjs';
import { prepareRoot,checkRoot } from '../../docs/benchmarks/layout-rl/coverage.mjs';
import { measure } from './full_factory_audit.mjs';
const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const compare=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
async function main(o){
  const dir=path.resolve(o.screen),p=read(path.join(dir,'protocol.json')),r=read(path.join(dir,'report.json'));
  assert.equal(r.status,'complete');assert.equal(r.policy_immutable,true);assert.equal(hash(path.join(dir,'protocol.json')),r.protocol_sha256);
  assert.equal(hash(path.join(p.parent,'protocol.json')),p.parent_protocol_sha256);
  assert.equal(hash(path.join(p.parent,'report.json')),p.parent_report_sha256);
  assert.equal(hash(path.join(p.parent,'learned.pt')),p.checkpoint_sha256);
  for(const [file,expected]of Object.entries(p.source_sha256))assert.equal(hash(file),expected,file);
  const parent=read(path.join(p.parent,'protocol.json'));
  for(const [file,expected]of Object.entries(parent.sources))assert.equal(hash(file),expected,file);
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,f])=>[k,read(f)]));
  const roots=new Map(p.cases.map(c=>{const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)};
    return[c.id,{root,graph:prepareRoot(root,catalogs),family:c.family}];}));
  const expected=new Set(p.temperatures.flatMap(t=>p.cases.flatMap(c=>p.banks.map(b=>`${t}:${c.id}:${b}`))));
  assert.equal(r.rows.length,expected.size);const probe=new Probe({...paths,validator:path.resolve(o.validator)},5000),rows=[];
  try{
    for(const row of r.rows){
      const key=`${row.temperature}:${row.case}:${row.bank}`;assert(expected.delete(key),key);
      const {root,graph,family}=roots.get(row.case);assert.equal(row.family,family);
      const w=unpack(path.join(row.directory,'best.json.gz')),e=unpack(path.join(row.directory,'episode.json.gz'));
      const coverage=checkRoot(root,graph,w,await probe.check(w));assert(coverage.factory_validated,JSON.stringify(coverage));
      assert.deepEqual(measure(w,catalogs),row.best_metrics);assert.deepEqual(measure(root.witness,catalogs).score,row.initial_metrics.score);
      assert(compare(row.best_metrics.score,row.initial_metrics.score)<=0);assert.equal(e.trace.length,row.selected);assert(row.selected<=p.horizon);
      assert.equal(row.budget_complete,row.selected===p.horizon&&!e.trace.some(t=>t.status==='late'));
      let best=row.initial_metrics.score;
      for(const [i,t]of e.trace.entries()){
        assert.equal(t.selected,i+1);assert(t.route_expansions<=p.route_work);assert(compare(t.best_score,best)<=0);best=t.best_score;
        assert(!t.diagnostics.some(d=>d.code==='factory.executor-error'));
      }
      assert.deepEqual(best,row.best_metrics.score);assert.deepEqual(e.best_metrics,row.best_metrics);
      rows.push({key,factory_validated:true,score:row.best_metrics.score,best_sha256:hash(path.join(row.directory,'best.json.gz')),
        episode_sha256:hash(path.join(row.directory,'episode.json.gz'))});
    }
    assert.equal(expected.size,0);write(o.output,{schema_version:1,status:'passed',episodes:rows.length,rows,
      scope:'All-best original-graph coverage and independent metrics; no full action replay claim',report_sha256:hash(path.join(dir,'report.json'))});
    console.log(JSON.stringify({status:'passed',episodes:rows.length}));
  }finally{probe.close();}
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
