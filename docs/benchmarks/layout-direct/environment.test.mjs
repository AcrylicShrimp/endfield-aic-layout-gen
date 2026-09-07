import test from 'node:test';
import assert from 'node:assert/strict';
import { read,catalogKeys,Probe,digest } from '../layout-rl/io.mjs';
import { DirectEnvironment } from './environment.mjs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repo=fileURLToPath(new URL('../../../',import.meta.url)),plan=read(new URL('./sampling.json',import.meta.url));
const options=Object.fromEntries(catalogKeys.map(k=>[k,path.join(repo,'data/game/normalized',`${k==='components'?'logistics-components':k}.json`)]));
options.validator=path.join(repo,'target/release/aic-witness-probe');
const catalogs=Object.fromEntries(catalogKeys.map(k=>[k,read(options[k])]));
const fp=new URL('../layout-production/fixtures/iron-interaction.json',import.meta.url),f=read(fp);
const root={id:'direct-iron-fixture',dialect:'chain-factory',request:read(new URL(f.request,fp)),witness:f.witness};
let probe;test.before(()=>{probe=new Probe(options);});test.after(()=>probe.close());
const action={kind:'relocate',instances:['S'],dx:-3,dy:5,compact_after:false};
test('direct observations expose complete grids and graph vocabulary, not candidate mutations',async()=>{
  const e=new DirectEnvironment(catalogs,probe,plan.environment),o=await e.reset(root);
  assert.equal(o.state.grid.length,10);assert.ok(o.state.grid.every(c=>c.length===o.state.bounds.width*o.state.bounds.height&&c.every(Number.isFinite)));
  assert.ok(o.state.facilities.every(p=>p.ports.length>0));assert.ok(o.state.terminals.every(t=>t.rate));
  assert.equal(o.state.actions,undefined);assert.equal(o.state.action_mask,undefined);
});
test('direct arbitrary XY movement can cross a valid worse state before compaction improves',async()=>{
  const e=new DirectEnvironment(catalogs,probe,plan.environment);await e.reset(root);const before=digest(root.witness);
  let o=await e.step(action);assert.deepEqual(o.current_score,[364,55,13]);assert.deepEqual(o.best_score,[364,35,9]);
  o=await e.step({kind:'compact',compact_after:false});assert.deepEqual(o.best_score,[308,47,13]);assert.equal(e.work.validations,3);
  assert.equal(digest(root.witness),before);
});
test('compound phases are independently checked and invalid intermediate cannot be compacted',async()=>{
  let calls=0;const invalid={check:async(w,t)=>{const r=await probe.check(w,t);return ++calls>1?{...r,covered_geometry_validated:false}:r;}};
  const e=new DirectEnvironment(catalogs,invalid,plan.environment);await e.reset(root);const o=await e.step({...action,compact_after:true});
  assert.equal(o.transition.status,'validation-rejected');assert.equal(e.work.compactions,0);assert.deepEqual(o.best_score,[364,35,9]);
});
test('late validated result and malformed DTO cannot corrupt or advance retained state',async()=>{
  let e,calls=0;const late={check:async(w,t)=>{const r=await probe.check(w,t);if(++calls>1)e.started=performance.now()-e.elapsedLimit;return r;}};
  e=new DirectEnvironment(catalogs,late,plan.environment);await e.reset(root);
  await assert.rejects(e.step({...action,dx:0.5}),/Malformed/);assert.equal(e.work.proposals,0);
  const o=await e.step(action);assert.equal(o.transition.status,'late-result');assert.deepEqual(o.best_score,[364,35,9]);assert.equal(o.done,true);
  assert.equal((await e.step(action)).transition.performed,false);assert.equal(e.work.proposals,1);
});
