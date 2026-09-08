// Compare retained cold controls without treating elapsed time as a paired benchmark.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {args,read,write,hash} from '../../docs/benchmarks/layout-rl/io.mjs';
const o=args(['before','after','output']),sources=[],rows=[];
const load=(directory,file,gzip=false)=>{
  const p=path.join(directory,file);sources.push({path:p,sha256:hash(p)});
  return gzip?JSON.parse(gunzipSync(fs.readFileSync(p))):read(p);
};
const reports=[o.before,o.after].map(d=>load(d,'report.json'));
for(const r of reports)assert.equal(r.status,'complete');
assert.equal(reports[0].rows.length,reports[1].rows.length);
for(const a of reports[1].rows){
  const b=reports[0].rows.find(r=>r.method===a.method&&r.case===a.case);assert(b);
  const old=load(b.directory,'episode.json.gz',true),now=load(a.directory,'episode.json.gz',true);
  assert.equal(old.trace[0].physical_before,now.trace[0].physical_before);
  assert.deepEqual(load(b.directory,'best.json.gz',true),load(a.directory,'best.json.gz',true));
  assert.deepEqual(old.trace.map(t=>t.action),now.trace.map(t=>t.action));
  const initial=e=>({available:e.trace[0].available,...e.trace[0].poses});
  rows.push({method:a.method,exact_best_equal:true,exact_selected_actions_equal:true,
    before_initial:initial(old),after_initial:initial(now)});
}
write(o.output,{schema_version:1,status:'passed',rows,sources,
  scope:'Exact retained witness/action equality and initial domain counts; timing is observational'});
console.log(JSON.stringify({status:'passed',methods:rows.length}));
