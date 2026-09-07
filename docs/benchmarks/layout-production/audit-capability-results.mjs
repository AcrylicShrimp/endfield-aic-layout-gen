import assert from 'node:assert/strict';
import path from 'node:path';
import { read } from '../layout-rl/io.mjs';
import { compare } from '../layout-rl/score.mjs';

// Domain sizes and improvements below are historical pilot/strong observations.
// Audit reported fields; artifact bytes and factory validity are not revalidated.
const scope='historical-pilot-strong-capability-reported-fields-only';
let input=null;
try {
  const argv=process.argv.slice(2);
  if(argv.length!==2||argv[0]!=='--input'||!argv[1]||argv[1].startsWith('--'))
    throw new Error('Usage: node audit-capability-results.mjs --input <capability-results.json>');
  input=path.resolve(argv[1]);
  const data=read(input);
  assert.equal(data.schema_version,1);
  assert.equal(data.status,'capability-measured-not-ready');
  assert.equal(data.scope,'one-step-development-domains');
  assert.ok(data.experiments.pilot.rows.length>0);
  assert.ok(data.experiments.strong.rows.length>0);
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
  console.log(JSON.stringify({status:'passed',scope,input}));
} catch(error) {
  console.error(JSON.stringify({code:'capability-results-audit-failed',severity:'error',
    stage:'evidence-audit',scope,entity:input,message:error.message}));
  process.exitCode=1;
}
