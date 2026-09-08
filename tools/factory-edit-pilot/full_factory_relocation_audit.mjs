// Original-graph, physical-metric and complete-matrix audit for the relocation vocabulary.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,Probe,sourcePaths} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {density,routeDiagnostics} from '../../docs/benchmarks/layout-production/metrics.mjs';
import {measure} from './full_factory_audit.mjs';
import {transform} from '../../docs/benchmarks/layout-rl/geometry.mjs';
const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const ascii=v=>JSON.stringify(v).replace(/[\u007f-\uffff]/g,c=>'\\u'+c.charCodeAt(0).toString(16).padStart(4,'0'));
const canonical=v=>Array.isArray(v)?v.map(canonical).sort((a,b)=>ascii(a)<ascii(b)?-1:ascii(a)>ascii(b)?1:0):
  v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])])):v;
const physical=w=>createHash('sha256').update(ascii(canonical(w))).digest('hex');
const compare=(a,b)=>{const i=a.findIndex((x,i)=>x!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
async function main(options){
  const inputs=read(options.inputs),directory=path.resolve(options.relocation),p=read(path.join(directory,'protocol.json')),
    report=read(path.join(directory,'relocation.json'));
  assert.equal(report.status,'complete');assert.equal(hash(path.join(directory,'protocol.json')),report.protocol_sha256);
  assert.equal(hash(options.inputs),p.inputs_sha256);
  for(const [name,expected]of Object.entries(p.source_sha256))assert.equal(hash(name),expected,name);
  const paths=Object.fromEntries(Object.entries(p.catalogs).map(([k,v])=>{assert.equal(hash(v.path),v.sha256);return[k,v.path];}));
  const catalogs=Object.fromEntries(Object.entries(paths).map(([k,p])=>[k,read(p)]));
  assert.equal(hash(p.cases.find(c=>c.id==='closure-strong').initial),p.strong_sha256);
  assert.equal(hash(p.cases.find(c=>c.id==='closure-incumbent').initial),p.incumbent_sha256);
  const initialMetrics=new Map(p.cases.map(c=>[c.id,measure(read(c.initial),catalogs)]));
  const initialKeys=new Map(p.cases.map(c=>[c.id,physical(transform(read(c.initial)))]));
  const roots=new Map(inputs.rows.map(r=>{
    assert.equal(hash(r.original),r.files_sha256['original-request.json']);
    const root={id:r.id,dialect:r.dialect,request:read(r.original),witness:read(r.initial)};
    return[r.id,{root,prepared:prepareRoot(root,catalogs)}];
  }));
  roots.set('closure-strong',roots.get('closure-original'));roots.set('closure-incumbent',roots.get('closure-original'));
  const expected=new Set(),expectedCells=new Set(),expectedGroups=new Set();
  const configs=[...p.arms.flatMap(a=>['untrained','final'].map(phase=>({...a,label:a.id+'-'+phase}))),...p.controls.map(a=>({...a,label:a.id}))];
  for(const c of configs)for(const input of p.cases)for(const bank of p.banks){
    expectedCells.add(`${c.label}:${input.id}:${bank}`);
    for(let i=0;i<p.evaluation_actions/c.horizon;i++)expected.add(`${c.label}--${input.id}--${bank}--${i}`);
  }
  for(const a of p.arms)for(let g=0;g<p.training_actions/(p.group_episodes*a.horizon);g++){
    expectedGroups.add(`${a.id}:${g}`);
    for(let i=0;i<p.group_episodes;i++)expected.add(`${a.id}-train-${g}-${i}`);
  }
  assert.equal(report.rows.length,expected.size);assert.equal(report.cells.length,expectedCells.size);assert.equal(report.groups.length,expectedGroups.size);
  const probe=new Probe({...paths,validator:path.resolve(options.validator)}),rows=[],lookup=new Map(),selected=new Set(report.cells.map(c=>c.best));
  try{
    for(const row of report.rows){
      assert(expected.delete(row.directory),row.directory);
      const where=path.join(directory,row.directory),w=unpack(path.join(where,'best.json.gz')),e=unpack(path.join(where,'episode.json.gz'));
      const {root,prepared}=roots.get(row.case),validation=await probe.check(w);
      assert(checkRoot(root,prepared,w,validation).factory_validated,row.directory);
      assert.deepEqual(row.initial_metrics,initialMetrics.get(row.case));
      assert.deepEqual(measure(w,catalogs),row.best_metrics);assert.equal(physical(w),row.physical_best_sha256);
      assert(compare(row.best_metrics.score,row.initial_metrics.score)<=0);
      assert.equal(e.trace.length,row.selected);assert.equal(row.selected,row.config.horizon);
      let before=e.trace[0].physical_before,best=row.initial_metrics.score,expanded=0,native=0,unchanged=0,revisited=0;
      assert.equal(before,initialKeys.get(row.case));
      const seen=new Set([before]);
      for(const [i,t]of e.trace.entries()){
        assert.equal(t.selected,i+1);assert.equal(t.physical_before,before);
        assert.equal(t.physical_unchanged,t.physical_after===before);
        assert.equal(t.physical_revisited,t.physical_after!==before&&seen.has(t.physical_after));
        assert(t.route_expansions<=p.route_work);expanded+=t.route_expansions;
        assert(t.masked>=0&&t.masked<t.available);if(!row.config.memory)assert.equal(t.masked,0);
        if(t.acceptance)native++;if(t.physical_unchanged)unchanged++;if(t.physical_revisited)revisited++;
        assert(compare(t.best_score,best)<=0);best=t.best_score;before=t.physical_after;seen.add(before);
        assert(!t.diagnostics.some(d=>d.code==='factory.executor-error'));
      }
      assert.equal(expanded,row.route_expansions);assert.equal(native,row.native_proposals);
      assert.equal(unchanged,row.physical_unchanged);assert.equal(revisited,row.physical_revisited);
      assert.deepEqual(best,row.best_metrics.score);
      const result={directory:row.directory,factory_validated:true,score:row.best_metrics.score,
                    episode_sha256:hash(path.join(where,'episode.json.gz')),best_sha256:hash(path.join(where,'best.json.gz'))};
      if(selected.has(row.directory)){result.density=density(w,catalogs);result.routes=routeDiagnostics(w);}
      rows.push(result);lookup.set(row.directory,row);
    }
    assert.equal(expected.size,0);
    for(const cell of report.cells){
      assert(cell.seconds<=p.cell_seconds);
      assert(expectedCells.delete(`${cell.arm}:${cell.case}:${cell.bank}`));
      const config=configs.find(c=>c.label===cell.arm),attempts=cell.attempts.map(n=>lookup.get(n));
      assert.equal(attempts.length,p.evaluation_actions/config.horizon);
      for(const [i,r]of attempts.entries()){
        assert.equal(r.arm,cell.arm);assert.equal(r.case,cell.case);assert.equal(r.bank,cell.bank);assert.equal(r.attempt,i);
      }
      assert.equal(attempts.reduce((sum,r)=>sum+r.selected,0),p.evaluation_actions);
      const best=attempts.reduce((a,b)=>compare(b.best_metrics.score,a.best_metrics.score)<0?b:a);
      assert.equal(best.directory,cell.best);assert.deepEqual(best.best_metrics,cell.best_metrics);
    }
    assert.equal(expectedCells.size,0);
    for(const g of report.groups){
      assert(expectedGroups.delete(`${g.arm}:${g.group}`));assert.equal(g.case,p.training_roots[g.group%p.training_roots.length]);
      const members=report.rows.filter(r=>r.phase==='training'&&r.arm===g.arm&&r.group===g.group);
      assert.equal(members.length,p.group_episodes);assert.deepEqual(members.map(r=>r.best_metrics.score),g.scores);
      assert.deepEqual(g.scores.map(s=>g.scores.reduce((n,t)=>n-compare(s,t),0)/(p.group_episodes-1)),g.ranks);
      assert(Number.isFinite(g.loss)&&Number.isFinite(g.gradient_norm));
    }
    assert.equal(expectedGroups.size,0);
    write(path.resolve(options.output),{schema_version:1,status:'passed',episodes:rows.length,cells:report.cells.length,rows,
      scope:'Original graph and independent JS metrics; geometry is the existing Rust validator; sorted state identity checked on this dataset',
      provenance:[options.inputs,path.join(directory,'protocol.json'),path.join(directory,'relocation.json'),path.resolve(options.validator),
        ...sourcePaths(fileURLToPath(import.meta.url))].map(p=>({path:p,sha256:hash(p)}))});
    console.log(JSON.stringify({status:'passed',episodes:rows.length,cells:report.cells.length}));
  }finally{probe.close();}
}
main(args(['inputs','relocation','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
