import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { interactionDomain,interactionLevel,validateInteraction } from './interaction-model.mjs';
import { read,digest,catalogKeys,Probe } from '../layout-rl/io.mjs';
import { prepareRoot } from '../layout-rl/coverage.mjs';
import { compare } from '../layout-rl/score.mjs';

const repo=fileURLToPath(new URL('../../../',import.meta.url));
const options=Object.fromEntries(catalogKeys.map(k=>[k,path.join(repo,'data/game/normalized',`${k==='components'?'logistics-components':k}.json`)]));
options.validator=path.join(repo,'target/release/aic-witness-probe');
const catalogs=Object.fromEntries(catalogKeys.map(k=>[k,read(options[k])]));
const fixturePath=new URL('./fixtures/iron-interaction.json',import.meta.url),fixture=read(fixturePath);
const root={id:'iron-interaction-fixture',dialect:'chain-factory',request:read(new URL(fixture.request,fixturePath)),witness:fixture.witness};
const graph=prepareRoot(root,catalogs),level=(kind,compact_after=false,max_members=1)=>({id:`${kind}-${compact_after}-${max_members}`,kind,compact_after,max_members,step_limit:0,junction_policy:'internal'});
const budget={proposals:10000,route_expansions:2000000,per_action_expansions:5000,elapsed_ms:10000,validator_ms:3000};
let probe;
test.before(()=>{probe=new Probe(options);});test.after(()=>probe?.close());

test('void/contact domains retain simultaneous-axis moves and deduplicate explicit parameters',()=>{
  const before=digest(root.witness);
  for(const kind of ['void','contact']){
    const a=interactionDomain(root.witness,graph,catalogs,level(kind,false,16));
    assert.ok(a.actions.length>0);assert.equal(new Set(a.actions.map(JSON.stringify)).size,a.actions.length);
    assert.ok(a.actions.some(a=>a.moves[0].dx!==0&&a.moves[0].dy!==0));
    assert.ok(a.actions.every(a=>a.moves[0].instances.length<=16));
    assert.deepEqual(interactionDomain(root.witness,graph,catalogs,level(kind,true,16)).actions,a.actions);
  }
  assert.equal(digest(root.witness),before);
});

test('compaction alone cannot improve the frozen iron root',async()=>{
  const r=await interactionLevel(root,catalogs,probe,level('compact'),budget);
  assert.deepEqual(r.best,r.initial);assert.equal(r.visited,1);assert.equal(r.work.compactions,1);assert.equal(r.work.validations,2);
  assert.ok(r.trace[0].phases.every(p=>p.factory_validated));
});

test('a valid worse relocation followed by four-row contraction improves area',async()=>{
  const plain=await interactionLevel(root,catalogs,probe,level('contact',false,16),budget);
  const paired=await interactionLevel(root,catalogs,probe,level('contact',true,16),budget);
  assert.deepEqual(plain.best,[364,35,9]);assert.deepEqual(paired.best,[308,47,13]);
  const row=paired.trace.find(t=>t.phases.some(p=>p.new_incumbent));
  assert.ok(compare(row.phases[0].score,paired.initial)>0);assert.deepEqual(row.phases[0].score,[364,55,13]);
  assert.deepEqual(row.phases[1].removed_rows,[7,8,9,10]);assert.equal(row.compaction_enabled_root_improvement,true);
  assert.ok(row.phases.every(p=>p.factory_validated));assert.equal(paired.enumeration_complete,true);
});

test('an invalid intermediate is never sent to compaction',async()=>{
  let calls=0;
  const invalid={check:async(w,timeout)=>{const r=await probe.check(w,timeout);return ++calls>1?{...r,covered_geometry_validated:false}:r;}};
  const r=await interactionLevel(root,catalogs,invalid,level('compact'),budget);
  assert.equal(r.counts['validation-rejected'],1);assert.equal(r.work.compactions,0);assert.deepEqual(r.best,r.initial);
});

test('rejected final compaction preserves independently valid intermediate improvements',async()=>{
  let calls=0;
  const invalidFinal={check:async(w,timeout)=>{const r=await probe.check(w,timeout);return ++calls>1&&calls%2===1?{...r,covered_geometry_validated:false}:r;}};
  const r=await interactionLevel(root,catalogs,invalidFinal,level('port',true),budget);
  assert.ok(r.counts['validation-rejected']>0);assert.ok(compare(r.best,r.initial)<0);assert.equal(r.factory_validated,true);
  assert.ok(r.trace.some(t=>t.phases[0]?.new_incumbent&&t.phases[1]?.status==='validation-rejected'));
});

test('late final compaction cannot replace or discard the timely intermediate',async()=>{
  let calls=0;
  const delayed={check:async(w,timeout)=>{const r=await probe.check(w,timeout);if(++calls===3)await new Promise(resolve=>setTimeout(resolve,60));return r;}};
  const r=await interactionLevel(root,catalogs,delayed,level('port',true),{...budget,elapsed_ms:50});
  assert.equal(r.counts['late-result'],1);assert.equal(r.termination,'elapsed-budget');
  assert.equal(r.trace[0].phases[0].new_incumbent,true);assert.deepEqual(r.best,r.trace[0].phases[0].score);
});

test('work caps are explicit and preserve the original witness',async()=>{
  const original=digest(root.witness),r=await interactionLevel(root,catalogs,probe,level('contact',true,16),{...budget,proposals:1});
  assert.equal(r.visited,1);assert.equal(r.enumeration_complete,false);assert.equal(r.termination,'proposal-budget');
  assert.equal(digest(root.witness),original);assert.equal(r.global_infeasibility_proven,false);
  assert.equal(r.work.proposals,r.trace.length);assert.equal(r.work.route_expansions,r.trace.reduce((s,t)=>s+t.route_expansions,0));
  assert.throws(()=>validateInteraction({...level('void'),compact_after:undefined}),/Invalid interaction/);
});

test('unknown validator faults abort rather than masquerading as geometric rejection',async()=>{
  let calls=0;
  const broken={check:async(w,timeout)=>{if(++calls===2)throw new Error('simulated infrastructure fault');return probe.check(w,timeout);}};
  await assert.rejects(interactionLevel(root,catalogs,broken,level('compact'),budget),/simulated infrastructure fault/);
});
