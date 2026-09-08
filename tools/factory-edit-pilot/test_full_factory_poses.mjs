import assert from 'node:assert/strict';
import fs from 'node:fs';
import {productionEdges,poseGroups,translationActions,poseOptions,poseActions} from './full_factory_poses.mjs';
const rate={numerator:1,denominator:1};
const t=(id,instance,direction,x,y=0)=>({id,endpoint:{kind:'facility',instance},direction,position:{x,y},rate});
const network=(terminals,segments=[])=>({transport:'belt',item:'synthetic',terminals,segments,cells:terminals.map(t=>t.position)});
const placements=['a','b','c','d'].map((instance,i)=>({instance,x:i*3,y:2,width:1,height:1}));
const w={placements,logistics_components:[],transport_networks:[network([t('a','a','output',0),t('b','b','input',1),t('c','c','output',4),t('d','d','input',5)],
  [{from:{x:0,y:0},to:{x:1,y:0}},{from:{x:4,y:0},to:{x:5,y:0}}])]};
assert.deepEqual(productionEdges(w),[[0,1],[2,3]]); // Same commodity, two disconnected islands.
w.transport_networks.push(network([t('back','b','output',7),t('back-in','a','input',7)]));
assert(poseGroups(w,true).groups.some(g=>g.source==='cycle'&&g.members.join(',')==='0,1'));
const shared={...w,transport_networks:[network([t('a','a','output',0),t('b','b','input',0),t('c','c','input',0)])]};
assert.deepEqual(poseGroups(shared,true).groups.find(g=>g.source==='terminal_closure').members,[0,1,2]);
assert(!poseGroups(shared,false).groups.some(g=>g.members.includes(0)));
shared.transport_networks[0].terminals.push({id:'outside',endpoint:{kind:'external',node:'outside'},direction:'input',position:{x:0,y:0},rate});
assert(poseGroups(shared,true).omitted.some(g=>g.code==='factory.group-external-anchor'));
assert.throws(()=>translationActions(w,{mode:'unknown'}),e=>e.code==='factory.vocabulary-options');
console.log('Relocation reachability, cycle, required closure and external-anchor checks passed');

const catalogs=Object.fromEntries(['facilities','recipes','items','transports','components'].map(k=>[k,JSON.parse(fs.readFileSync(new URL(`../../data/game/normalized/${k==='components'?'logistics-components':k}.json`,import.meta.url)))]));
const factory=JSON.parse(fs.readFileSync(new URL('../../docs/benchmarks/constructive-bound-interconnect/fixtures/crossing-witness.json',import.meta.url))),before=structuredClone(factory);
const p=factory.placements[0],valid={member:0,x:p.x+1,y:p.y,rotation:p.rotation};
for(const poses of [[],[valid,valid],[{...valid,member:-1}],[{...valid,x:NaN}],[{...valid,rotation:45}],[{...valid,x:p.x}]])
  assert.throws(()=>poseOptions(factory,catalogs,poses),e=>e.code==='factory.pose-parameters');
assert.throws(()=>poseActions(factory,catalogs,{compound_preflights:0}),e=>e.code==='factory.vocabulary-options');
const domain=poseActions(factory,catalogs,{compound_preflights:1,compound_scans:8});
assert(domain.report.compound_preflights<=1);
assert(domain.actions.every(a=>a.kind==='pose'&&a.poses.length>0));
assert.deepEqual(factory,before);
console.log('Pose input bounds, catalog geometry, explicit preflight budget and immutable parent checks passed');
