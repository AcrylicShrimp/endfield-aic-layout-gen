import test from 'node:test';
import assert from 'node:assert/strict';
import { read } from '../layout-rl/io.mjs';
import { compare } from '../layout-rl/score.mjs';

test('committed capability snapshot preserves domain coverage, budgets and negative-result scope',()=>{
  const data=read(new URL('./capability-results.json',import.meta.url));
  assert.equal(data.test_evaluations,0);assert.equal(data.no_global_impossibility_claim,true);
  for(const experiment of Object.values(data.experiments))for(const row of experiment.rows){
    assert.equal(row.visited,Object.values(row.counts).reduce((a,b)=>a+b,0));assert.equal(row.visited,row.work.proposals);
    assert.equal(row.enumeration_complete,row.visited===row.domain_size);assert.ok(row.visited<=row.budget.proposals);
    assert.ok(row.work.route_expansions<=row.budget.route_expansions);assert.equal(row.factory_validated,true);
    assert.ok(compare(row.best,row.initial)<=0);assert.equal(row.counts['validation-rejected']??0,0);
    assert.equal(row.artifacts.length,2);assert.ok(row.artifacts.every(a=>/^[a-f0-9]{64}$/.test(a.sha256)));
  }
  const strong=data.experiments.strong.rows;
  assert.ok(strong.every(r=>r.enumeration_complete));assert.ok(strong.every(r=>!r.diagnostics['local-work-budget']));
  assert.ok(strong.every(r=>r.best[0]===r.initial[0]));
  assert.ok(strong.find(r=>r.level.id==='rotate-t2').best[1]<strong[0].initial[1]);
  assert.equal(strong.find(r=>r.level.id==='group16-t8').domain_size,11456);
});
