import assert from 'node:assert/strict';
import {productionEdges,relocationGroups,relocationActions} from './full_factory_relocation.mjs';
const rate={numerator:1,denominator:1};
const t=(id,instance,direction,x,y=0)=>({id,endpoint:{kind:'facility',instance},direction,position:{x,y},rate});
const network=(terminals,segments=[])=>({transport:'belt',item:'synthetic',terminals,segments,cells:terminals.map(t=>t.position)});
const placements=['a','b','c','d'].map((instance,i)=>({instance,x:i*3,y:2,width:1,height:1}));
const w={placements,logistics_components:[],transport_networks:[network([t('a','a','output',0),t('b','b','input',1),t('c','c','output',4),t('d','d','input',5)],
  [{from:{x:0,y:0},to:{x:1,y:0}},{from:{x:4,y:0},to:{x:5,y:0}}])]};
assert.deepEqual(productionEdges(w),[[0,1],[2,3]]); // Same commodity, two disconnected islands.
w.transport_networks.push(network([t('back','b','output',7),t('back-in','a','input',7)]));
assert(relocationGroups(w,true).groups.some(g=>g.source==='cycle'&&g.members.join(',')==='0,1'));
const shared={...w,transport_networks:[network([t('a','a','output',0),t('b','b','input',0),t('c','c','input',0)])]};
assert.deepEqual(relocationGroups(shared,true).groups.find(g=>g.source==='terminal_closure').members,[0,1,2]);
assert(!relocationGroups(shared,false).groups.some(g=>g.members.includes(0)));
shared.transport_networks[0].terminals.push({id:'outside',endpoint:{kind:'external',node:'outside'},direction:'input',position:{x:0,y:0},rate});
assert(relocationGroups(shared,true).omitted.some(g=>g.code==='factory.group-external-anchor'));
assert.throws(()=>relocationActions(w,{mode:'unknown'}),e=>e.code==='factory.vocabulary-options');
console.log('Relocation reachability, cycle, required closure and external-anchor checks passed');
