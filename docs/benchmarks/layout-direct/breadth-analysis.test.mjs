import test from 'node:test';
import assert from 'node:assert/strict';
import { inspectFrontier } from './breadth-analysis.mjs';

test('no-frontier diagnosis distinguishes a finished seed component from a routing failure',()=>{
  const wiring={nodes:['a','b','c','d'].map(id=>({id,kind:'facility'})),edges:[{id:'ab',source:'a',target:'b'},{id:'cd',source:'c',target:'d'}]};
  const report={request:{target_instance:'a'},status:'growth-exhausted',final_node:{member_instances:['a','b'],internal_requirements:['ab']},
    failed_statistics:{astar_searches:0},failed_placement_attempts:0,attempted_groups_at_failure:[]};
  assert.equal(inspectFrontier(wiring,report).implementation_no_frontier,true);
  report.final_node.member_instances=['a'];report.final_node.internal_requirements=[];
  assert.equal(inspectFrontier(wiring,report).implementation_no_frontier,false);
  assert.equal(inspectFrontier(wiring,report).unresolved_incident_requirements,1);
  report.final_node.member_instances=['a','b'];report.final_node.internal_requirements=['ab'];report.failed_statistics.astar_searches=1;
  assert.equal(inspectFrontier(wiring,report).implementation_no_frontier,false);
});
