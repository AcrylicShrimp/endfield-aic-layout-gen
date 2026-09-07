import test from 'node:test';
import assert from 'node:assert/strict';
import { density, mechanicalWaste, largestEmptyRectangle } from './metrics.mjs';

test('XY occupancy counts overlapping transport layers once and does not crop border voids', () => {
  const w = { placements: [{ x: -1, y: 2, width: 2, height: 2 }],
    transport_networks: [{ transport: 'belt', cells: [{ x: 2, y: 2 }] }, { transport: 'pipe', cells: [{ x: 2, y: 2 }] }], logistics_components: [] };
  const original = JSON.stringify(w), r = density(w, { components: { components: [] } });
  assert.equal(r.area, 8); assert.equal(r.occupied_xy_cells, 5); assert.equal(r.empty_xy_cells, 3);
  assert.equal(r.transport_layer_cells, 2); assert.equal(r.belt_pipe_overlap_xy, 1);
  assert.equal(r.density_gate, false); assert.equal(r.optimality_certified, false);
  assert.equal(JSON.stringify(w), original);
});

test('largest empty rectangle matches exhaustive enumeration for all small occupancy masks', () => {
  const bounds = { minX: 0, minY: 0, maxX: 2, maxY: 2, width: 3, height: 3 };
  for (let mask = 0; mask < 512; mask++) {
    const occupied = new Set(Array.from({ length: 9 }, (_, i) => i).filter(i => mask & 1 << i).map(i => `${i%3},${Math.floor(i/3)}`));
    let best = 0;
    for (let x=0;x<3;x++) for(let y=0;y<3;y++) for(let right=x;right<3;right++) for(let bottom=y;bottom<3;bottom++) {
      let clear = true;
      for(let a=x;a<=right;a++) for(let b=y;b<=bottom;b++) clear &&= !occupied.has(`${a},${b}`);
      if(clear) best=Math.max(best,(right-x+1)*(bottom-y+1));
    }
    assert.equal(largestEmptyRectangle(occupied,bounds).area,best);
  }
});

test('one percent gate uses exact cell counts and loaded component footprints', () => {
  const catalogs = { components: { components: [{ id: 'synthetic-component', footprint: { width: 1, height: 2 }, allowed_rotations: [90] }] } };
  const w = { placements: [{ x: 0, y: 0, width: 10, height: 9 }], transport_networks: [{ transport: 'belt', cells: Array.from({ length: 7 }, (_, x) => ({ x, y: 9 })) }],
    logistics_components: [{ component: 'synthetic-component', position: { x: 8, y: 9 }, rotation: 90 }] };
  assert.equal(density(w,catalogs).density_gate,true);
  assert.equal(density(w,catalogs).empty_xy_cells,1);
  w.transport_networks[0].cells.pop();
  assert.equal(density(w,catalogs).density_gate,false);
});

test('waste categories separate invalid, unchanged, repeated and useful exploratory states', () => {
  const r = mechanicalWaste({ work: { proposals: 5, worse_transitions: 1, improvements: 1 }, trace: [
    { parent_sha256: 'a', accepted: false }, { parent_sha256: 'a', accepted: true, witness_sha256: 'a' },
    { parent_sha256: 'a', accepted: true, witness_sha256: 'b' }, { parent_sha256: 'b', accepted: true, witness_sha256: 'a' },
    { parent_sha256: 'a', accepted: true, witness_sha256: 'c' }] });
  assert.equal(r.futile,3); assert.equal(r.novel_valid,2); assert.equal(r.futile_fraction,.6);
  assert.equal(r.valid_worse_transitions,1);
  assert.equal(mechanicalWaste({work:{proposals:0},trace:[]}).futile_fraction,null);
});

test('density bounds are checked before enumerating a far-away body', () => {
  const w = { placements: [{x:0,y:0,width:2,height:2},{x:100,y:100,width:2,height:2}], transport_networks: [], logistics_components: [] };
  assert.throws(() => density(w, {}, {cellBudget:16}), /area budget exceeded/);
  assert.throws(() => density(w, {}, {cellBudget:0}), /Invalid density cell budget/);
  w.placements = [{x:Number.MAX_SAFE_INTEGER,y:0,width:2,height:2}];
  assert.throws(() => density(w, {}), /Noninteger occupied coordinate/);
});
