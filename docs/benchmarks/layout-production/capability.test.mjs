import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { actionDomain,productionIdentity,screenBodies,probeLevel,validateBudget,validateLevel } from './capability-model.mjs';
import { read,catalogKeys,Probe,digest } from '../layout-rl/io.mjs';
import { semanticKey } from '../layout-rl/coverage.mjs';
import { constructFactory } from '../chain-prototype/factory.mjs';
import { prepareFactory } from '../chain-prototype/factory-input.mjs';

const repo=fileURLToPath(new URL('../../../',import.meta.url));
const options=Object.fromEntries(catalogKeys.map(k=>[k,path.join(repo,'data/game/normalized',`${k==='components'?'logistics-components':k}.json`)]));
options.validator=path.join(repo,'target/release/aic-witness-probe');
const catalogs=Object.fromEntries(catalogKeys.map(k=>[k,read(options[k])]));
const level=(kind,step_limit=0,max_members=1)=>({id:`${kind}-${step_limit}-${max_members}`,kind,step_limit,max_members,junction_policy:'internal'});
const budget={proposals:1000,route_expansions:200000,per_action_expansions:5000,elapsed_ms:10000,validator_ms:3000};
let root,graph,probe;
test.before(async()=>{
  probe=new Probe(options);
  const request=read(new URL('../chain-prototype/citron-factory.json',import.meta.url));
  graph=prepareFactory(request,catalogs);
  const result=await constructFactory(graph,async w=>{const report=await probe.check(w);return {status:report.covered_geometry_validated?0:2,report,artifact:null};});
  assert.equal(result.factory_validated,true);
  root={id:'capability-generated',dialect:'chain-factory',request,witness:result.selected.witness};
});
test.after(()=>probe?.close());

test('production identity releases geometry and port assignment, but not recipe, rates or external identity',()=>{
  const w=root.witness,copy=structuredClone(w),original=digest(w);
  copy.placements[0].rotation=(copy.placements[0].rotation+90)%360;copy.placements[0].x+=10;
  const t=copy.transport_networks.flatMap(n=>n.terminals).find(t=>t.endpoint.kind==='facility');t.endpoint.port='hypothetical-port';
  assert.equal(productionIdentity(copy),productionIdentity(w));assert.notEqual(semanticKey(copy),semanticKey(w));
  copy.placements[0].recipe='different-recipe';assert.notEqual(productionIdentity(copy),productionIdentity(w));
  copy.placements[0].recipe=w.placements[0].recipe;t.rate.numerator++;assert.notEqual(productionIdentity(copy),productionIdentity(w));
  const external=structuredClone(w);external.transport_networks.flatMap(n=>n.terminals).find(t=>t.endpoint.kind==='external').endpoint.side='changed';
  assert.notEqual(productionIdentity(external),productionIdentity(w));assert.equal(digest(w),original);
});

test('raw translation levels retain smaller domains, are deduplicated and round-robin across facilities',()=>{
  const w=root.witness,a=actionDomain(w,graph,catalogs,level('translate',2)),b=actionDomain(w,graph,catalogs,level('translate',4,2));
  assert.equal(a.length,w.placements.length*8);
  assert.equal(new Set(a.slice(0,w.placements.length).map(a=>a.moves[0].instances[0])).size,w.placements.length);
  const signatures=new Set(b.map(JSON.stringify));assert.equal(signatures.size,b.length);assert.ok(a.every(a=>signatures.has(JSON.stringify(a))));
});

test('catalog rotations enumerate every orientation and explicit shift; port domains do not prefilter occupancy',()=>{
  const w=root.witness,a=actionDomain(w,graph,catalogs,level('rotate')),b=actionDomain(w,graph,catalogs,level('rotate',2));
  const expected=w.placements.reduce((sum,p)=>sum+catalogs.facilities.facilities.find(f=>f.id===p.facility).allowed_rotations.filter(r=>r!==p.rotation).length,0);
  assert.equal(a.length,expected);assert.equal(b.length,expected*9);assert.ok(a.every(a=>a.moves[0].dx===0&&a.moves[0].dy===0));
  const ports=actionDomain(w,graph,catalogs,level('port'));assert.ok(ports.length>0);assert.ok(ports.every(a=>a.portAssignments.length===1));
});

test('explicit body screen rejects bounds and overlap without mutating geometry',()=>{
  const w=root.witness,original=digest(w),[a,b]=w.placements;
  assert.equal(screenBodies(w,catalogs,{moves:[{instances:[a.instance],dx:10000,dy:0}]}).code,'local-move-bounds');
  assert.equal(screenBodies(w,catalogs,{moves:[{instances:[a.instance],dx:b.x-a.x,dy:b.y-a.y}]}).code,'local-body-collision');
  assert.equal(digest(w),original);
});

test('one-step probe charges every raw proposal, preserves root and validates retained candidates',async()=>{
  const original=digest(root.witness),r=await probeLevel(root,catalogs,probe,level('rotate',2),budget);
  assert.equal(r.visited,r.domain_size);assert.equal(r.work.proposals,r.trace.length);assert.equal(r.enumeration_complete,true);
  assert.equal(r.trace.reduce((s,t)=>s+t.route_expansions,0),r.work.route_expansions);
  assert.equal(Object.values(r.counts).reduce((a,b)=>a+b,0),r.visited);
  assert.ok(r.work.validations>0);assert.equal(r.factory_validated,true);assert.equal(digest(root.witness),original);
  assert.equal(r.global_infeasibility_proven,false);assert.equal(r.no_opportunity_proven,false);
});

test('proposal and route caps return the original valid incumbent without claiming exhaustion',async()=>{
  const r=await probeLevel(root,catalogs,probe,level('translate',2),{...budget,proposals:1});
  assert.equal(r.visited,1);assert.equal(r.enumeration_complete,false);assert.equal(r.termination,'proposal-budget');assert.equal(r.factory_validated,true);
  const s=await probeLevel(root,catalogs,probe,level('rotate',2),{...budget,route_expansions:1,per_action_expansions:1});
  assert.ok(s.work.route_expansions<=1);assert.equal(s.enumeration_complete,false);assert.equal(s.termination,'route-budget');assert.equal(s.factory_validated,true);
});

test('invalid operational bounds and unknown actions are rejected',()=>{
  assert.throws(()=>validateBudget({...budget,proposals:0}),/budget/);
  assert.throws(()=>validateBudget({...budget,per_action_expansions:300000}),/guard/);
  assert.throws(()=>validateLevel(level('missing')),/level/);
  assert.throws(()=>validateLevel(level('translate',0)),/level/);
  assert.throws(()=>validateLevel({...level('rotate'),id:undefined}),/level/);
});

test('a late independently valid candidate cannot replace the incumbent',async()=>{
  let calls=0;
  const delayed={check:async(w,timeout)=>{
    const result=await probe.check(w,timeout);
    if(++calls>1)await new Promise(resolve=>setTimeout(resolve,60));
    return result;
  }};
  const r=await probeLevel(root,catalogs,delayed,level('port'),{...budget,elapsed_ms:50});
  assert.ok(r.counts['late-result']>0);assert.equal(digest(r.best_witness),digest(root.witness));
  assert.equal(r.termination,'elapsed-budget');assert.equal(r.factory_validated,true);
});

test('validator failures abort instead of being counted as failed geometric actions',async()=>{
  let calls=0;
  const broken={check:async(w,timeout)=>{if(++calls>1)throw new Error('simulated validator failure');return probe.check(w,timeout);}};
  await assert.rejects(probeLevel(root,catalogs,broken,level('port'),budget),/simulated validator failure/);
});
