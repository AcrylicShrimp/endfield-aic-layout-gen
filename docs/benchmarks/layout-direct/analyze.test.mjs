import test from 'node:test';
import assert from 'node:assert/strict';
import { summarize } from './analyze.mjs';

const audit=()=>({coverage:{factory_validated:true},validation:{covered_geometry_validated:true,factory_validated:false}});
function fixture(){
  return {schema_version:1,status:'complete',scope:'dynamic-factory-direct-action-training',elapsed_seconds:1,
    config:{evaluation_horizons:[256],evaluation_seeds:[1]},generation:[],test_evaluations:0,
    training:{groups:[{audits:[audit()]}],optimizer_steps:1},
    evaluation:['initial','uniform','trained'].map((model,i)=>({model,root:'r',horizon:256,seed:1,split:'validation',audit:audit(),
      summary:{best_score:{area:100,tuple:[100,20-i,3]},initial_score:{area:200},waste:{futile_fraction:.5},
        work:{proposals:256,route_expansions:10,preprocess_ms:1},elapsed_ms:10}}))};
}
test('analysis compares exact score tuples and requires both independent audit layers',()=>{
  const r=fixture(),result=summarize(r);
  assert.deepEqual(result.evaluation[0].comparisons,{trained_vs_initial:{wins:1},trained_vs_uniform:{wins:1}});
  assert.equal(result.audited_episodes,4);
  assert.equal(result.evaluation[0].models.trained.mean_area_ratio,.5);
  r.training.groups[0].audits[0].coverage.factory_validated=false;
  assert.throws(()=>summarize(r),/failed independent audit/);
});
test('analysis rejects incomplete runs and missing paired controls',()=>{
  const r=fixture();r.status='failed';assert.throws(()=>summarize(r),/complete streaming run/);
  r.status='complete';r.evaluation.shift();assert.throws(()=>summarize(r),/Missing matched control/);
});
