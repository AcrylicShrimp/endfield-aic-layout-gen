// Audit all original-root/training/inverse bests without inferring success from labels.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {args,read,write,hash,Probe,sourcePaths} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {transform} from '../../docs/benchmarks/layout-rl/geometry.mjs';
import {density,routeDiagnostics} from '../../docs/benchmarks/layout-production/metrics.mjs';
import {measure} from './full_factory_audit.mjs';
const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const ascii=v=>JSON.stringify(v).replace(/[\u007f-\uffff]/g,c=>'\\u'+c.charCodeAt(0).toString(16).padStart(4,'0'));
const canonical=v=>Array.isArray(v)?v.map(canonical).sort((a,b)=>ascii(a)<ascii(b)?-1:ascii(a)>ascii(b)?1:0):
  v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])])):v;
const identity=w=>createHash('sha256').update(ascii(canonical(w))).digest('hex');
const compare=(a,b)=>{const i=a.findIndex((x,i)=>x!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
async function main(o){
  const dir=path.resolve(o.screen),p=read(path.join(dir,'protocol.json')),r=read(path.join(dir,'screen.json')),data=read(path.join(p.curriculum_path,'curriculum.json'));
  assert.equal(r.status,'complete');assert.equal(hash(path.join(dir,'protocol.json')),r.protocol_sha256);
  assert.equal(hash(path.join(p.curriculum_path,'curriculum.json')),p.curriculum_sha256);
  assert.equal(hash(path.join(p.curriculum_path,'protocol.json')),p.curriculum_protocol_sha256);
  for(const [file,expected]of Object.entries(p.source_sha256))assert.equal(hash(file),expected,file);
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>{assert.equal(hash(v),p.catalog_sha256[k]);return[k,read(v)];}));
  const cases=new Map([...p.cases,...data.pairs.map(x=>x.case)].map(c=>[c.id,c])),roots=new Map(),initials=new Map(),metrics=new Map();
  for(const c of cases.values()){
    assert.equal(hash(c.initial),c.initial_sha256);const initial=transform(read(c.initial)),root={id:c.id,dialect:c.dialect,request:read(c.original),witness:initial};
    roots.set(c.id,{root,prepared:prepareRoot(root,catalogs)});initials.set(c.id,identity(initial));metrics.set(c.id,measure(initial,catalogs));
  }
  const labels=['untrained-d0','untrained-d2',...p.arms.map(a=>a.id+'-final'),'uniform'],expected=new Set(),expectedCells=new Set(),expectedInverse=new Set();
  for(const label of labels){
    for(const c of p.cases)for(const bank of p.banks){expectedCells.add(`${label}:${c.id}:${bank}`);for(let a=0;a<4;a++)expected.add(`${label}--${c.id}--${bank}--${a}`);}
    for(const pair of p.validation_pairs)for(const selection of ['sample','greedy'])expectedInverse.add(`${label}:${pair}:${selection}`);
  }
  for(const a of p.arms.filter(a=>a.method!=='imitation'))for(let g=0;g<p.training_actions/32;g++)for(let n=0;n<4;n++)expected.add(`${a.id}-train-${g}-${n}`);
  assert.equal(r.rows.length,expected.size);assert.equal(r.cells.length,expectedCells.size);assert.equal(r.inverse.length,expectedInverse.size);
  const probe=new Probe({...paths,validator:path.resolve(o.validator)}),rows=[],lookup=new Map(),selected=new Set(r.cells.map(c=>c.best));
  async function check(row,caseId,inverse=false){
    const where=path.join(dir,row.directory),w=unpack(path.join(where,'best.json.gz')),e=unpack(path.join(where,'episode.json.gz')),{root,prepared}=roots.get(caseId);
    assert(checkRoot(root,prepared,w,await probe.check(w)).factory_validated,row.directory);
    assert.deepEqual(measure(w,catalogs),row.best_metrics);assert.deepEqual(row.initial_metrics,metrics.get(caseId));assert(compare(row.best_metrics.score,row.initial_metrics.score)<=0);
    assert.equal(e.trace.length,inverse?1:8);assert.equal(e.selected,inverse?1:8);let best=row.initial_metrics.score,previous=initials.get(caseId),expanded=0,native=0,unchanged=0,revisited=0;
    const seen=new Set([previous]);
    for(const [i,t]of e.trace.entries()){
      assert.equal(t.selected,i+1);assert(t.route_expansions<=p.route_work);expanded+=t.route_expansions;
      assert(compare(t.best_score,best)<=0);best=t.best_score;
      assert(!t.diagnostics.some(d=>d.code==='factory.executor-error'));
      if(t.acceptance){native++;assert(t.acceptance.candidate_validated);}
      if(t.status==='improved')assert(t.acceptance?.candidate_validated);
      if(!inverse){assert.equal(t.physical_before,previous);assert.equal(t.physical_unchanged,t.physical_after===previous);assert.equal(t.physical_revisited,t.physical_after!==previous&&seen.has(t.physical_after));
        unchanged+=t.physical_unchanged;revisited+=t.physical_revisited;previous=t.physical_after;seen.add(previous);assert.equal(t.masked,0);}
    }
    assert.deepEqual(best,row.best_metrics.score);
    if(!inverse){assert.equal(identity(w),row.physical_best_sha256);assert.equal(expanded,row.route_expansions);assert.equal(native,row.native_proposals);assert.equal(unchanged,row.physical_unchanged);assert.equal(revisited,row.physical_revisited);}
    const result={directory:row.directory,case:caseId,factory_validated:true,score:row.best_metrics.score,best_sha256:hash(path.join(where,'best.json.gz')),episode_sha256:hash(path.join(where,'episode.json.gz'))};
    if(selected.has(row.directory)){result.density=density(w,catalogs);result.routes=routeDiagnostics(w);}rows.push(result);
  }
  try{
    for(const row of r.rows){assert(expected.delete(row.directory));await check(row,row.case);lookup.set(row.directory,row);}
    assert.equal(expected.size,0);
    for(const c of r.cells){assert(expectedCells.delete(`${c.arm}:${c.case}:${c.bank}`));assert.equal(c.selected,32);assert(c.seconds<=p.cell_seconds);
      const attempts=c.attempts.map(n=>lookup.get(n));assert.equal(attempts.length,4);
      for(const [i,a]of attempts.entries()){assert.equal(a.arm,c.arm);assert.equal(a.case,c.case);assert.equal(a.bank,c.bank);assert.equal(a.attempt,i);}
      const best=attempts.reduce((a,b)=>compare(b.best_metrics.score,a.best_metrics.score)<0?b:a);assert.equal(best.directory,c.best);assert.deepEqual(best.best_metrics,c.best_metrics);
    }
    assert.equal(expectedCells.size,0);
    for(const row of r.inverse){assert(expectedInverse.delete(`${row.arm}:${row.pair}:${row.selection}`));await check(row,row.pair,true);
      const pair=data.pairs.find(p=>p.id===row.pair),e=unpack(path.join(dir,row.directory,'episode.json.gz'));assert.equal(e.trace[0].status,row.status);
      assert.equal(JSON.stringify(e.trace[0].action)===JSON.stringify(pair.inverse),row.teacher_chosen);
    }
    assert.equal(expectedInverse.size,0);
    const groups=new Set();
    for(const g of r.groups){
      const arm=p.arms.find(a=>a.id===g.arm);assert(arm);assert(Number.isFinite(g.loss)&&Number.isFinite(g.gradient_norm));
      const id=`${g.arm}:${g.update??g.group}`;assert(!groups.has(id));groups.add(id);
      if(arm.method==='imitation'){assert(g.update>=0&&g.update<p.imitation_updates);assert(p.training_pairs.includes(g.pair));assert(g.teacher_probability>0&&g.teacher_probability<=1);}
      else{
        assert(g.group>=0&&g.group<p.training_actions/32);let caseId=p.training_roots[g.group%p.training_roots.length];
        if(arm.method==='mixed_pg'){
          const root=p.training_roots[Math.floor(g.group/2)%p.training_roots.length];caseId=root;
          const lessons=data.pairs.filter(x=>x.root===root&&x.split==='training');
          if(g.group%2)caseId=lessons[Math.floor(g.group/(2*p.training_roots.length))%lessons.length].case.id;
        }
        assert.equal(g.case,caseId);const members=r.rows.filter(x=>x.phase==='training'&&x.arm===g.arm&&x.group===g.group);
        assert.equal(members.length,4);assert.deepEqual(members.map(x=>x.best_metrics.score),g.scores);
        assert.deepEqual(g.scores.map(s=>g.scores.reduce((n,t)=>n-compare(s,t),0)/3),g.ranks);
      }
    }
    assert.equal(groups.size,p.arms.reduce((n,a)=>n+(a.method==='imitation'?p.imitation_updates:p.training_actions/32),0));
    write(path.resolve(o.output),{status:'passed',episodes:rows.length,cells:r.cells.length,inverse:r.inverse.length,rows,
      scope:'Original production graphs, independent metrics, complete matrix, budget accounting and recorded ranks; full optimizer reconstruction not claimed',
      provenance:[path.join(dir,'protocol.json'),path.join(dir,'screen.json'),o.validator,...sourcePaths(fileURLToPath(import.meta.url))].map(file=>({path:file,sha256:hash(file)}))});
    console.log(JSON.stringify({status:'passed',episodes:rows.length,cells:r.cells.length,inverse:r.inverse.length}));
  }finally{probe.close();}
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
