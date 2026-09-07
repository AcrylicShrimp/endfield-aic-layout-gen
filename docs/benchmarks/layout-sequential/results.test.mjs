import test from 'node:test';
import assert from 'node:assert/strict';
import { summarize } from './results.mjs';

const row=(model,best=[90,5,3])=>({model,root:'fixture',split:'validation',horizon:128,seed:101,audit:{coverage:{factory_validated:true}},
  summary:{factory_validated:true,initial_score:{tuple:[100,5,3]},best_score:{tuple:best},work:{proposals:128,preprocess_ms:1,feature_ms:2,route_ms:3},
    waste:{futile_fraction:.25},density:{empty_fraction:.3},elapsed_ms:10,termination:'proposal-budget'},artifact:{path:'fixture-only',sha256:'fixture-only'}});
const report=()=>({schema_version:1,status:'complete',scope:'fixture-only',test_evaluations:0,config:{evaluation_horizons:[128]},
  training:{optimizer_steps:1,sampled_transitions:8,informative_groups:1,changed_weight_elements:2,groups:[{root:'fixture',sampled_transitions:8}]},
  evaluation:[row('initial'),row('trained',[88,9,4])]});

test('comparison uses exact score priority and keeps root-normalized area separate',()=>{
  const s=summarize(report());assert.equal(s.cohorts[0].wins,1);assert.equal(s.cohorts[0].trained.mean_area_ratio,.88);
  assert.equal(s.readiness,'not-established');assert.equal(s.pairs[0].trained_comparison,-1);
});

test('missing controls, mismatched root scores, invalid audit and bad accounting are rejected',()=>{
  let r=report();r.evaluation.pop();assert.throws(()=>summarize(r),/Missing matched/);
  r=report();r.evaluation[1].summary.initial_score.tuple=[101,5,3];assert.throws(()=>summarize(r),/Mismatched initial/);
  r=report();r.evaluation[1].audit.coverage.factory_validated=false;assert.throws(()=>summarize(r),/invariant/);
  r=report();r.training.optimizer_steps=2;assert.throws(()=>summarize(r),/accounting/);
});
