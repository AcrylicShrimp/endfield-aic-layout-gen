// Full-score comparisons for every admitted factory; preserve failed admission rows.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {args,read,write,hash} from '../../docs/benchmarks/layout-rl/io.mjs';
const cmp=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
const mean=v=>v.length?v.reduce((s,x)=>s+x,0)/v.length:null;
function main(o){
  const p=read(path.join(o.screen,'protocol.json')),r=read(path.join(o.screen,'report.json')),a=read(path.join(o.screen,'audit.json')),admission=read(path.join(p.admission,'report.json'));
  assert.equal(r.status,'complete');assert.equal(a.status,'passed');assert.equal(a.report_sha256,hash(path.join(o.screen,'report.json')));
  const measurements=[],comparisons=[],failures={},caseRows=[];
  const audit=new Map(a.rows.map(row=>[row.key,row]));
  for(const method of p.methods){
    for(const group of [...new Set(p.cases.map(c=>c.transfer_group))]){
      const rows=r.rows.filter(row=>row.method===method&&row.transfer_group===group);
      const ops=rows.flatMap(row=>audit.get(`${row.method}:${row.case}:${row.bank}`).operations);
      measurements.push({method,group,episodes:rows.length,complete:rows.filter(r=>r.budget_complete).length,
        mean_area_reduction_percent:mean(rows.map(r=>100*(1-r.best_metrics.score[0]/r.initial_metrics.score[0]))),
        valid_translations:ops.filter(t=>t.translated>0).length,improving_translations:ops.filter(t=>t.translated>0&&t.status==='improved').length,
        improving_rotations:ops.filter(t=>t.rotated>0&&t.status==='improved').length,mean_seconds:mean(rows.map(r=>r.seconds))});
    }
    for(const control of ['parent','geometry',...(method.startsWith('retention-')?[method.replace('retention-','exposure-')]:[])]){
      if(control===method)continue;
      const counts={method,control,wins:0,ties:0,losses:0,incomplete_pairs:0};
      for(const row of r.rows.filter(r=>r.method===method)){
        const other=r.rows.find(r=>r.method===control&&r.case===row.case&&r.bank===row.bank);assert(other);
        if(!row.budget_complete||!other.budget_complete)counts.incomplete_pairs++;
        counts[cmp(row.best_metrics.score,other.best_metrics.score)<0?'wins':cmp(row.best_metrics.score,other.best_metrics.score)>0?'losses':'ties']++;
      }
      comparisons.push(counts);
    }
  }
  for(const row of r.rows){
    const e=JSON.parse(gunzipSync(fs.readFileSync(path.join(row.directory,'episode.json.gz'))));
    for(const t of e.trace)for(const d of t.diagnostics){const key=`${row.method}:${d.code}`;failures[key]=(failures[key]??0)+1;}
    caseRows.push({method:row.method,case:row.case,bank:row.bank,initial:row.initial_metrics.score,best:row.best_metrics.score,
      selected:row.selected,budget_complete:row.budget_complete,kinds:row.kinds,statuses:row.statuses});
  }
  const result={schema_version:1,status:'complete',report_sha256:hash(path.join(o.screen,'report.json')),audit_sha256:hash(path.join(o.screen,'audit.json')),
    admission:{attempted:admission.attempted,accepted:admission.accepted,failures:admission.rows.filter(r=>r.status!=='accepted').map(r=>({id:r.id,status:r.status,facilities:r.facilities,diagnostics:r.diagnostics}))},
    measurements,comparisons,failures,rows:caseRows,scope:'All prospective transfer outcomes; opened recipe overlap and missing factory admission remain explicit'};
  write(o.output,result);console.log(JSON.stringify({status:'complete',measurements,comparisons}));
}
main(args(['screen','output']));
