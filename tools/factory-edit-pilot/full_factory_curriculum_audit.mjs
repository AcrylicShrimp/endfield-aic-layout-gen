// Independent full-factory inverse admission and original-production coverage audit.
import assert from 'node:assert/strict';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {args,read,write,hash,Probe,sourcePaths} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {transform} from '../../docs/benchmarks/layout-rl/geometry.mjs';
import {measure} from './full_factory_audit.mjs';
import {describe,execute} from './full_factory_executor.mjs';
import {fileURLToPath} from 'node:url';
const ascii=v=>JSON.stringify(v).replace(/[\u007f-\uffff]/g,c=>'\\u'+c.charCodeAt(0).toString(16).padStart(4,'0'));
const canonical=v=>Array.isArray(v)?v.map(canonical).sort((a,b)=>ascii(a)<ascii(b)?-1:ascii(a)>ascii(b)?1:0):
  v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])])):v;
const identity=w=>createHash('sha256').update(ascii(canonical(w))).digest('hex');
const less=(a,b)=>{const i=a.findIndex((x,i)=>x!==b[i]);return i>=0&&a[i]<b[i];};
async function main(o){
  const directory=path.resolve(o.curriculum),p=read(path.join(directory,'protocol.json')),r=read(path.join(directory,'curriculum.json'));
  assert.equal(r.status,'complete');assert.equal(hash(path.join(directory,'protocol.json')),r.protocol_sha256);
  assert.equal(hash(o.inputs),p.inputs_sha256);
  for(const [file,expected]of Object.entries(p.source_sha256))assert.equal(hash(file),expected,file);
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>{assert.equal(hash(v),p.catalog_sha256[k]);return[k,read(v)];}));
  const cases=new Map(p.cases.map(c=>[c.id,c])),roots=new Map(p.training_roots.map(id=>{
    const c=cases.get(id);assert.equal(hash(c.initial),c.initial_sha256);
    const root={id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)};
    return[id,{root,prepared:prepareRoot(root,catalogs),initial:transform(root.witness)}];
  }));
  const probe=new Probe({...paths,validator:path.resolve(o.validator)}),rows=[],seen=new Set();
  try{
    for(const pair of r.pairs){
      const {root,prepared,initial}=roots.get(pair.root),w=read(pair.case.initial);assert.equal(hash(pair.case.initial),pair.case.initial_sha256);
      assert(!seen.has(pair.physical_key));seen.add(pair.physical_key);assert.equal(identity(w),pair.physical_key);
      assert(checkRoot(root,prepared,w,await probe.check(w)).factory_validated);
      assert.deepEqual(measure(w,catalogs),pair.initial_metrics);assert(less(pair.root_metrics.score,pair.initial_metrics.score));
      const forward=execute(initial,catalogs,pair.forward,5000);assert.equal(forward.status,'candidate');
      assert.equal(identity(transform(forward.witness)),pair.physical_key);
      const d=describe(w,catalogs);assert(d.actions.some(a=>JSON.stringify(a)===JSON.stringify(pair.inverse)));
      const inverse=execute(w,catalogs,pair.inverse,5000);assert.equal(inverse.status,'candidate');
      assert(checkRoot(root,prepared,inverse.witness,await probe.check(inverse.witness)).factory_validated);
      assert.deepEqual(measure(inverse.witness,catalogs),pair.inverse_metrics);assert(less(pair.inverse_metrics.score,pair.initial_metrics.score));
      assert.equal(pair.restores_root,!less(pair.root_metrics.score,pair.inverse_metrics.score));
      rows.push({id:pair.id,root:pair.root,split:pair.split,factory_validated:true,initial:pair.initial_metrics.score,inverse:pair.inverse_metrics.score,
                 restores_root:pair.restores_root,input_sha256:hash(pair.case.initial)});
    }
    for(const id of p.training_roots){
      const local=r.pairs.filter(x=>x.root===id);assert(local.length>=16);assert.equal(local.filter(x=>x.split==='validation').length,8);
      assert.equal(r.generation.find(x=>x.root===id).counts.admitted,local.length);
    }
    const discoveries=[];
    for(const g of r.generation)if(g.discovery){const {root,prepared}=roots.get(g.root),w=read(g.discovery);
      assert(checkRoot(root,prepared,w,await probe.check(w)).factory_validated);assert.deepEqual(measure(w,catalogs).score,g.best);
      discoveries.push({root:g.root,score:g.best,sha256:hash(g.discovery)});
    }
    write(path.resolve(o.output),{status:'passed',pairs:rows.length,rows,discoveries,scope:'Forward and offered inverse replay, original graphs and independent metrics',
      provenance:[path.join(directory,'protocol.json'),path.join(directory,'curriculum.json'),o.inputs,o.validator,...sourcePaths(fileURLToPath(import.meta.url))].map(file=>({path:file,sha256:hash(file)}))});
    console.log(JSON.stringify({status:'passed',pairs:rows.length,discoveries:discoveries.length}));
  }finally{probe.close();}
}
main(args(['inputs','curriculum','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
