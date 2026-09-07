import test from 'node:test';
import assert from 'node:assert/strict';
import { read,digest } from '../layout-rl/io.mjs';
import { sampleRequest,partitionFor,validateSampling } from './sampling.mjs';

const catalogs={recipes:read(new URL('../../../data/game/normalized/recipes.json',import.meta.url))};
const policy=read(new URL('./sampling.json',import.meta.url)).sampling;
test('a stream position replays exactly while successive positions change semantic production requests',()=>{
  const position={seed:'streaming-test',ordinal:0,split:'train'},before=digest(catalogs);
  assert.deepEqual(sampleRequest(catalogs,policy,position),sampleRequest(catalogs,policy,position));
  const samples=Array.from({length:32},(_,ordinal)=>sampleRequest(catalogs,policy,{...position,ordinal}));
  assert.ok(new Set(samples.map(s=>s.request.target.item)).size>16);
  assert.ok(new Set(samples.map(s=>s.structure_sha256)).size>24);
  assert.ok(samples.some(s=>s.source_cycles));assert.ok(samples.some(s=>s.trace.some(t=>t.declared_external)));
  assert.equal(digest(catalogs),before);
});
test('target-family partitions cover the full catalog output pool without a recipe shortlist',()=>{
  const items=[...new Set(catalogs.recipes.recipes.flatMap(r=>r.outputs.map(o=>o.item)))],sets=new Map();
  for(const item of items){const split=partitionFor(item,policy);const s=sets.get(split)??new Set();s.add(item);sets.set(split,s);}
  assert.equal([...sets.values()].reduce((n,s)=>n+s.size,0),items.length);
  for(const split of ['train','validation','test'])assert.ok(sets.get(split).size>0);
  for(const split of sets.keys())for(let ordinal=0;ordinal<8;ordinal++)assert.ok(sets.get(split).has(sampleRequest(catalogs,policy,{seed:'partition-test',ordinal,split}).request.target.item));
});
test('choices are catalog-derived, root production is mandatory, and repeated item sourcing is explicit',()=>{
  const sample=sampleRequest(catalogs,policy,{seed:'streaming-test',ordinal:3,split:'train'}),byItem=new Map();
  assert.equal(sample.request.source_selections[0].source.kind,'recipe');
  for(const t of sample.trace){
    if(t.source.kind==='recipe')assert.ok(catalogs.recipes.recipes.find(r=>r.id===t.source.recipe).outputs.some(o=>o.item===t.item));
    if(byItem.has(t.item))assert.deepEqual(byItem.get(t.item),t.source);byItem.set(t.item,t.source);
  }
});
test('an exhausted hierarchy guard rejects instead of adding an external shortcut',()=>{
  assert.throws(()=>sampleRequest(catalogs,{...policy,max_source_nodes:1},{seed:'streaming-test',ordinal:3,split:'train'}),e=>e.code==='stream-source-guard');
});
test('target loads above one multiply demand, not per-instance utilization or source choices',()=>{
  const position={seed:'streaming-test',ordinal:3,split:'train'};
  const low=sampleRequest(catalogs,{...policy,target_loads:[{numerator:1,denominator:1}]},position);
  const high=sampleRequest(catalogs,{...policy,target_loads:[{numerator:8,denominator:1}]},position);
  assert.deepEqual(high.request.source_selections,low.request.source_selections);
  assert.equal(high.request.target.quantity,low.request.target.quantity*8);
  assert.equal(high.request.target.duration_ms,low.request.target.duration_ms);
  assert.deepEqual(high.target_load,{numerator:8,denominator:1});
  assert.equal(high.structure_sha256,low.structure_sha256);
  assert.notEqual(high.request_sha256,low.request_sha256);
  assert.throws(()=>validateSampling({...policy,schema_version:1}),/schema 2/);
  assert.throws(()=>validateSampling({...policy,utilizations:[]}),/obsolete/);
  assert.throws(()=>validateSampling({...policy,target_loads:[{numerator:1,denominator:0}]}),/target-load/);
});
