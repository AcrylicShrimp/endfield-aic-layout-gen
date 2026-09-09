// Summarize all pose strata and two separately declared contraction controls.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {args,read,write,hash,Probe} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {describe,execute} from './full_factory_executor.mjs';
import {measure} from './full_factory_audit.mjs';
const cmp=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
async function main(o){
  const p=read(path.join(o.screen,'protocol.json')),r=read(path.join(o.screen,'report.json')),
    outcomes=JSON.parse(gunzipSync(fs.readFileSync(path.join(o.screen,'outcomes.json.gz'))));
  assert.equal(r.status,'complete');assert.equal(hash(path.join(o.screen,'protocol.json')),r.protocol_sha256);
  assert.equal(hash(path.join(o.screen,'outcomes.json.gz')),r.outcomes_sha256);
  for(const [file,h] of Object.entries({...p.sources,...p.source_sha256}))assert.equal(hash(file),h,file);
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>[k,read(v)])),
    probe=new Probe({...paths,validator:path.resolve(o.validator)}),states=[];
  try{
    for(const c of p.cases){
      const rows=outcomes.filter(row=>row.state===c.state),root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},graph=prepareRoot(root,catalogs),
        description=describe(root.witness,catalogs),contracts=description.actions.filter(a=>a.kind==='contract'),action=contracts[0];assert(action);
      const result=execute(root.witness,catalogs,action,p.config.route_work);assert.equal(result.status,'candidate');
      assert(checkRoot(root,graph,result.witness,await probe.check(result.witness)).factory_validated);
      const metric=measure(result.witness,catalogs),state=r.states.find(s=>s.state===c.state),diagnostics={};
      for(const row of rows)for(const d of row.trace.diagnostics)diagnostics[d.code]=(diagnostics[d.code]??0)+1;
      const strata=Object.keys(p.config.quotas).map(kind=>{
        const xs=rows.filter(row=>row.stratum===kind);
        return {kind,selected:xs.length,valid:xs.filter(row=>row.factory_validated).length,
          translated:xs.filter(row=>row.displacement?.translated).length,improved:xs.filter(row=>row.trace.status==='improved').length,
          area_improved:xs.filter(row=>row.candidate_metrics&&row.candidate_metrics.score[0]<state.initial_metrics.score[0]).length};
      });
      states.push({state:c.state,strata,diagnostics,available_contractions:contracts.length,
        independent_contraction:{action,score:metric.score,route_expansions:result.route_expansions,factory_validated:true,
          comparison_to_best_pose:cmp(metric.score,state.best_metrics.score)},best_pose:state.best_metrics.score});
    }
  }finally{probe.close();}
  write(o.output,{schema_version:1,status:'complete',pose_trials:outcomes.length,independent_controls:states.length,states,
    report_sha256:hash(path.join(o.screen,'report.json')),scope:'Separate one-step contraction controls; pose denominator and labels remain unchanged'});
  console.log(JSON.stringify(states));
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
