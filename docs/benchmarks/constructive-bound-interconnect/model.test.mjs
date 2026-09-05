import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { buildFixture } from './build.mjs';
import { render } from './view.mjs';
const catalogs=Object.fromEntries(['recipes','facilities','items','transports','components'].map(k=>[k,JSON.parse(fs.readFileSync(new URL('../../../data/game/normalized/'+(k==='components'?'logistics-components':k)+'.json',import.meta.url)))]));
const spec=JSON.parse(fs.readFileSync(new URL('./fixture.json',import.meta.url)));
test('catalog-backed connected fixture preserves deterministic unit-grid geometry',()=>{
  const a=buildFixture(catalogs,spec,true,true),b=buildFixture(catalogs,spec,true,true);
  assert.deepEqual(a,b);assert.equal(a.witness.placements.length,5);
  assert.equal(a.witness.logistics_components.length,4);assert.equal(a.witness.transport_networks.length,5);
  const catalogHash=JSON.stringify(catalogs);buildFixture(catalogs,spec,false,false);assert.equal(JSON.stringify(catalogs),catalogHash);
  for(const n of a.witness.transport_networks)for(const s of n.segments)assert.equal(Math.abs(s.from.x-s.to.x)+Math.abs(s.from.y-s.to.y),1);
});
test('split-only and pending external variants retain their explicit scopes',()=>{
  const r=buildFixture(catalogs,spec,false,false);assert.equal(r.witness.placements.length,4);
  assert.equal(r.witness.logistics_components.length,3);assert.equal(r.witness.transport_networks.length,2);
  assert.equal(r.external_connected,false);
  assert(r.witness.transport_networks.every(n=>n.terminals.every(t=>t.endpoint.kind==='facility')));
});
test('full-rate scaling cannot silently pass the single-lane crossing compiler',()=>{
  const s=structuredClone(spec);s.consumer_utilization={numerator:1,denominator:1};
  assert.throws(()=>buildFixture(catalogs,s,true,true),e=>e.code==='interconnect-geometry-rejected');
});
test('visualization includes actual ports, pending scope and script-safe inline JSON',()=>{
  const d=buildFixture(catalogs,spec,true,true);d.case='</script>';
  const html=render(d,{covered_geometry_validated:true,recipe_terminals:[]},{transports:catalogs.transports});
  assert(html.includes('catalog_ports'));assert(html.includes('SCALED CAPACITY VIOLATION'));
  assert(html.includes('not the complete heavy-xiranite'));assert(html.includes('\\u003c/script>'));
});
