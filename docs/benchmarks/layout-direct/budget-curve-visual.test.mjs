import test from 'node:test';
import assert from 'node:assert/strict';
import {selectCases,sharedBounds} from './budget-curve-visual.mjs';

const roots=[{id:'small',witness:{placements:[{}]}},{id:'large',witness:{placements:[{},{},{}]}},{id:'target',witness:{placements:[{},{}]}}];
const fixture=seed=>({seed,evaluations:Object.fromEntries([0,64,256,1024].map(checkpoint=>[checkpoint,{episodes:roots.flatMap(root=>[101,211].map(seed=>({seed,
  summary:{root:root.id,work:{proposals:1024},best_score:{tuple:[100,20,5]}}})))}]))});

test('interim geometry selection uses the fixed first seed, target and largest root at every available checkpoint',()=>{
  const cases=selectCases(roots,[fixture(1901)],256);
  assert.deepEqual(cases.map(c=>[c.root.id,c.checkpoint,c.seed]),[['target',64,101],['large',64,101],['target',256,101],['large',256,101]]);
  assert.ok(cases.every(c=>!c.kind.includes('post-hoc')));
  assert.throws(()=>selectCases(roots,[fixture(2901)],64),/first training seed/);
  assert.throws(()=>selectCases(roots,[fixture(1901)],128),/declared checkpoint/);
});

test('final least-favorable pair searches all seeds and does not select by the most flattering image',()=>{
  const runs=[1901,2901,3901].map(fixture);
  runs[1].evaluations[1024].episodes.find(e=>e.summary.root==='small'&&e.seed===211).summary.best_score.tuple=[110,1,1];
  runs[2].evaluations[1024].episodes.find(e=>e.summary.root==='target'&&e.seed===101).summary.best_score.tuple=[100,500,500];
  const cases=selectCases(roots,runs,1024),worst=cases.at(-1);
  assert.equal(cases.length,7);assert.equal(worst.root.id,'small');assert.equal(worst.model_seed,2901);assert.equal(worst.seed,211);
  assert.deepEqual(worst.delta,[10,-19,-4]);assert.match(worst.kind,/post-hoc/);
  assert.throws(()=>selectCases(roots,runs.slice(0,2),1024),/all three/);
});

test('paired geometry selection rejects missing, duplicated or unequal-effort evaluations',()=>{
  for(const modify of [run=>run.evaluations[64].episodes.pop(),run=>run.evaluations[64].episodes.push(run.evaluations[64].episodes[4]),
                       run=>run.evaluations[64].episodes[4].summary.work.proposals=1000]){
    const run=fixture(1901);modify(run);
    // Even an unselected checkpoint episode must retain the complete evaluation allocation.
    assert.throws(()=>selectCases(roots,[run,fixture(2901),fixture(3901)],1024));
  }
});

test('every checkpoint of one root uses a common geometric scale without inflating footprint metrics',()=>{
  const side=(width,height)=>({score:{width,height,area:width*height}});
  const input=[{root:'a',untrained:side(10,8),trained:side(6,10)},
               {root:'a',untrained:side(10,8),trained:side(12,7)},
               {root:'b',untrained:side(2,4),trained:side(3,3)}];
  const cases=sharedBounds(input);
  assert.deepEqual(cases.map(c=>c.display_bounds),[{width:12,height:10},{width:12,height:10},{width:3,height:4}]);
  assert.equal(cases[0].trained.score.area,60);assert.equal(input[0].display_bounds,undefined);
});
