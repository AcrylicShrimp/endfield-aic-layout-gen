import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { SequentialEnvironment,actionFields,tokenFields } from './environment.mjs';
import { read,Probe,catalogKeys,digest } from '../layout-rl/io.mjs';

const repo=fileURLToPath(new URL('../../../',import.meta.url));
const options=Object.fromEntries(catalogKeys.map(k=>[k,path.join(repo,'data/game/normalized',`${k==='components'?'logistics-components':k}.json`)]));
options.validator=path.join(repo,'target/release/aic-witness-probe');
const catalogs=Object.fromEntries(catalogKeys.map(k=>[k,read(options[k])]));
const fixturePath=new URL('../layout-production/fixtures/iron-interaction.json',import.meta.url),fixture=read(fixturePath);
const root={id:'iron-fixture',dialect:'chain-factory',request:read(new URL(fixture.request,fixturePath)),witness:fixture.witness};
const plan=read(new URL('./plan.json',import.meta.url)).environment;
let probe;
test.before(()=>{probe=new Probe(options);});test.after(()=>probe.close());
const environment=(p=probe,config=plan)=>new SequentialEnvironment([root],catalogs,p,config);
const findMove=(e,compact_after)=>e.actions.findIndex(a=>a.kind==='contact'&&a.compact_after===compact_after&&a.params.moves[0].instances.join()==='S'&&a.params.moves[0].dx===-3&&a.params.moves[0].dy===5);

test('expanded features are finite, include every configured family, and charge static filtering',async()=>{
  const e=environment(),o=await e.reset(root.id);
  assert.equal(tokenFields.length,15);assert.equal(actionFields.length,31);
  assert.ok(o.features.actions.every(a=>a.length===31&&a.every(Number.isFinite)));
  assert.deepEqual(new Set(e.actions.map(a=>a.kind)),new Set(plan.levels.map(l=>l.kind)));
  assert.ok(e.work.raw_generated>e.actions.length);assert.equal(e.work.static_rejected+e.actions.length,e.work.raw_generated);
  assert.equal(e.work.validations,0);assert.equal(e.work.route_expansions,0);
});

test('a validated worse transition advances current but cannot replace the incumbent',async()=>{
  const e=environment();await e.reset(root.id);const before=digest(root.witness),index=findMove(e,false);assert.ok(index>=0);
  let o=await e.step(index);assert.equal(o.transition.accepted,true);assert.deepEqual(o.current_score,[364,55,13]);assert.deepEqual(o.best_score,[364,35,9]);
  assert.equal(e.work.worse_transitions,1);assert.equal(digest(root.witness),before);
  o=await e.step(e.actions.findIndex(a=>a.kind==='compact'));
  assert.deepEqual(o.best_score,[308,47,13]);assert.deepEqual(o.current_score,o.best_score);assert.equal(e.work.validations,3);
});

test('explicit compound variant independently validates both phases',async()=>{
  const e=environment();await e.reset(root.id);const index=findMove(e,true);assert.ok(index>=0);
  const o=await e.step(index);assert.deepEqual(o.best_score,[308,47,13]);assert.equal(o.transition.phases.length,2);
  assert.ok(o.transition.phases.every(p=>p.status==='valid'));assert.equal(e.work.validations,2);
});

test('unchanged actions cannot repeat at an identical state and termination has padding only',async()=>{
  const e=environment(probe,{...plan,levels:[{id:'compact',kind:'compact'}]});await e.reset(root.id);
  const o=await e.step(0);assert.equal(o.done,true);assert.equal(o.transition.accepted,true);assert.equal(e.summary().waste.unchanged,1);
  const ended=await e.step(0);assert.equal(ended.transition.performed,false);assert.equal(e.work.proposals,1);
  assert.equal(ended.features.actions.length,1);assert.equal(ended.features.actions[0].length,31);
});

test('invalid intermediate rejects without compaction and infrastructure errors propagate',async()=>{
  let calls=0;const invalid={check:async(w,t)=>{const r=await probe.check(w,t);return ++calls>1?{...r,covered_geometry_validated:false}:r;}};
  const e=environment(invalid);await e.reset(root.id);const o=await e.step(findMove(e,true));
  assert.equal(o.transition.status,'validation-rejected');assert.equal(e.work.compactions,0);assert.deepEqual(o.best_score,[364,35,9]);
  e.probe={check:async()=>{throw new Error('fixture-validator-crashed');}};
  await assert.rejects(e.step(e.actions.findIndex(a=>a.kind==='compact')),/fixture-validator-crashed/);
});

test('late validation cannot advance current or replace the incumbent',async()=>{
  let e,calls=0;const late={check:async(w,t)=>{const r=await probe.check(w,t);if(++calls>1)e.started=performance.now()-e.elapsedLimit;return r;}};
  e=environment(late);await e.reset(root.id);const o=await e.step(findMove(e,true));
  assert.equal(o.transition.status,'late-result');assert.equal(o.done,true);assert.equal(e.work.compactions,0);assert.deepEqual(o.best_score,[364,35,9]);
});
