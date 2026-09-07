import test from 'node:test';
import assert from 'node:assert/strict';
import { observationSupport,sizeStratum,internalTopology } from './support.mjs';
import { read,digest } from '../layout-rl/io.mjs';

test('observation limits are measured without allocating or cropping a complete factory',()=>{
  const catalogs={components:read(new URL('../../../data/game/normalized/logistics-components.json',import.meta.url))};
  const w={placements:[{x:0,y:0,width:2,height:2}],transport_networks:[{cells:[{x:99,y:0}]}],logistics_components:[]},before=digest(w);
  const r=observationSupport(w,catalogs,{grid_cells:199,max_tokens:1});
  assert.equal(r.supported,false);assert.equal(r.grid_cells,200);assert.equal(r.tokens,2);
  assert.deepEqual(r.diagnostics.map(d=>d.code),['direct-grid-guard','direct-token-guard']);
  assert.equal(observationSupport(w,catalogs,{grid_cells:200,max_tokens:2}).supported,true);
  assert.equal(digest(w),before);
});
test('component footprint and rotation participate in bounds from catalog data',()=>{
  const spec={id:'synthetic-component',footprint:{width:2,height:3},allowed_rotations:[90]};
  const w={placements:[{x:0,y:0,width:1,height:1}],transport_networks:[],logistics_components:[{component:spec.id,rotation:90,position:{x:2,y:0}}]};
  const r=observationSupport(w,{components:{components:[spec]}},{grid_cells:100,max_tokens:10});
  assert.deepEqual(r.bounds,{minX:0,minY:0,maxX:4,maxY:1,width:5,height:2});
});
test('solved topology distinguishes source references from active cycles and disconnected parallel groups',()=>{
  const nodes=['a','b','c','d','e'].map(id=>({id})),edges=[['a','b'],['b','a'],['b','c'],['a','c'],['d','e'],['outside','d']].map(([source,target])=>({source,target}));
  assert.deepEqual(internalTopology(nodes,edges),{weak_components:2,weak_component_sizes:[3,2],cyclic_scc_sizes:[2],branching_facilities:2,merging_facilities:1});
  assert.deepEqual([undefined,1,2,16,17,64,65,128,129].map(sizeStratum),['unknown-flow','single','small','small','medium','medium','large','large','above-envelope']);
});
