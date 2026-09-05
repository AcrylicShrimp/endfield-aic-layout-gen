import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { buildFixture } from './build.mjs';
import { render } from './view.mjs';
import { buildDedicated } from './dedicated.mjs';
import { crossingBundle } from './bundle.mjs';
import { dto, capacity } from '../constructive-interconnect/rates.mjs';
import { orderInputs } from './order-ports.mjs';
import { restrictedOrders } from './restricted-order.mjs';
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
test('full-rate dedicated pairs use six lanes and two preplanned bridge cells',()=>{
  const spec=JSON.parse(fs.readFileSync(new URL('./full-fixture.json',import.meta.url))),result=buildDedicated(catalogs,spec);
  assert.equal(result.bindings.length,6);assert.equal(result.witness.logistics_components.length,2);
  assert(result.witness.logistics_components.every(c=>c.kind==='bridge'));
  assert.deepEqual(result.witness.utilizations.C0,{numerator:1,denominator:1});
  assert.deepEqual(result.witness.utilizations.A0,{numerator:1,denominator:1});
  assert.deepEqual(result.witness.utilizations.B,{numerator:2,denominator:3});
  assert.equal(result.witness.transport_networks.flatMap(n=>n.terminals).filter(t=>t.endpoint.kind==='external').length,5);
  const bad=structuredClone(spec);bad.links[1].target_port=bad.links[0].target_port;
  assert.throws(()=>buildDedicated(catalogs,bad),/reused port/);
  const missing=structuredClone(spec);missing.links.at(-1).via=[[12,8],[12,18]];
  assert.throws(()=>buildDedicated(catalogs,missing),/misses bundle channel/);
});
test('orthogonal bundle matrices have one bridge per distinct-axis lane pair',()=>{
  for(const transport of ['belt','pipe']){
    const items=catalogs.items.items.filter(i=>i.transport===transport).slice(0,2),cap=dto(capacity(catalogs.transports.transports.find(t=>t.kind===transport).capacity));
    const lane=(id,i)=>({id,item:items[i].id,transport,rate:cap});
    for(let n=1;n<=6;n++)for(let m=1;m<=6;m++){
      const h=Array.from({length:n},(_,i)=>lane('h'+i,0)),v=Array.from({length:m},(_,i)=>lane('v'+i,1)),r=crossingBundle(h,v,[2,2],catalogs);
      assert.equal(r.components.length,n*m);assert.equal(new Set(r.components.map(c=>JSON.stringify(c.position))).size,n*m);
      assert.equal(r.channels.length,n+m);assert.deepEqual(r.core,{x:2,y:2,width:m,height:n});
    }
    assert.throws(()=>crossingBundle([lane('h',0)],[lane('v',0)],[2,2],catalogs),/commodity-disjoint/);
    assert.throws(()=>crossingBundle([lane('h',0)],[undefined],[2,2],catalogs),/Unknown/);
    const over=lane('h',0);over.rate={numerator:cap.numerator*2,denominator:cap.denominator};
    assert.throws(()=>crossingBundle([over],[lane('v',1)],[2,2],catalogs),/capacity/);
  }
});
test('source-order port assignment removes both avoidable crossings without changing rates',()=>{
  const spec=JSON.parse(fs.readFileSync(new URL('./full-fixture.json',import.meta.url))),proposal=orderInputs(catalogs,spec);
  const before=buildDedicated(catalogs,spec),after=buildDedicated(catalogs,proposal.spec);
  assert.deepEqual(after.witness.utilizations,before.witness.utilizations);
  assert.deepEqual(after.bindings.map(l=>l.rate),before.bindings.map(l=>l.rate));
  assert.equal(after.witness.logistics_components.length,0);
  assert.equal(proposal.spec.links.find(l=>l.id==='B-C1').target_port,'input-belt-0');
  assert.equal(proposal.decisions.find(d=>d.source==='A1'&&d.kind==='align-exclusive-supplier').dy,1);
  assert.equal(proposal.accepted,false);
});
test('two actual item suppliers feeding both consumers retain a scoped order obstruction',()=>{
  const spec=JSON.parse(fs.readFileSync(new URL('./two-supplier-fixture.json',import.meta.url))),result=buildDedicated(catalogs,spec);
  assert.equal(new Set(result.bindings.map(l=>l.item)).size,2);
  assert.equal(result.witness.placements.length,4);assert.equal(result.witness.logistics_components.length,1);
  const proof=restrictedOrders(spec.links);assert.equal(proof.examined,64);assert.equal(proof.minimum_inversions,1);
  assert.equal(proof.global_infeasibility_proven,false);
  assert.equal(restrictedOrders(spec.links,1).exhaustive,false);
});
