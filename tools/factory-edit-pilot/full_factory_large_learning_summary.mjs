import path from 'node:path';
import fs from 'node:fs';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {args,read,write,hash} from '../../docs/benchmarks/layout-rl/io.mjs';
const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const compare=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
const mean=values=>values.reduce((sum,v)=>sum+v,0)/values.length;
function main(o){
  const p=read(path.join(o.screen,'protocol.json')),r=read(path.join(o.screen,'report.json')),audit=read(path.join(o.screen,'audit.json'));
  assert.equal(r.status,'complete');assert.equal(audit.status,'passed');assert.equal(audit.report_sha256,hash(path.join(o.screen,'report.json')));
  const group=c=>c.id===p.large?'prepared-large':c.id===p.original_large?'original-large':c.split==='train'?'rehearsal':c.family;
  const groups=[...new Set(p.cases.map(group))],byCase=new Map(p.cases.map(c=>[c.id,c])),measurements=[],comparisons=[];
  for(const method of p.methods)for(const g of groups){
    const rows=r.rows.filter(row=>row.method===method&&group(byCase.get(row.case))===g),keys=new Set(rows.map(row=>`${method}:${row.case}:${row.bank}`));
    const audited=audit.rows.filter(row=>keys.has(row.key)),operations=audited.flatMap(row=>row.operations);
    assert.equal(audited.length,rows.length);
    measurements.push({method,group:g,episodes:rows.length,complete:rows.filter(row=>row.budget_complete).length,
      mean_area_reduction_percent:mean(rows.map(row=>100*(1-row.best_metrics.score[0]/row.initial_metrics.score[0]))),
      valid_translating_actions:operations.filter(op=>op.translated>0).length,improving_translating_actions:operations.filter(op=>op.translated>0&&op.status==='improved').length,
      mean_seconds:mean(rows.map(row=>row.seconds))});
  }
  for(const method of p.methods.filter(m=>m.startsWith('learned')))for(const control of ['parent','geometry'])for(const g of groups){
    let wins=0,ties=0,losses=0;
    for(const row of r.rows.filter(row=>row.method===method&&group(byCase.get(row.case))===g)){
      const other=r.rows.find(other=>other.method===control&&other.case===row.case&&other.bank===row.bank);
      assert(other);const c=compare(row.best_metrics.score,other.best_metrics.score);if(c<0)wins++;else if(c>0)losses++;else ties++;
    }
    comparisons.push({method,control,group:g,wins,ties,losses});
  }
  const labels=read(path.join(o.screen,'label-scores.json')),labelSummary=[];
  for(const method of p.methods.filter(m=>m!=='geometry')){
    const rows=labels.filter(row=>row.method===method&&row.root!==p.large&&row.root!==p.original_large);
    labelSummary.push({method,group:'rehearsal',lessons:rows.length,top1:rows.filter(row=>row.top1).length,mean_mass:mean(rows.map(row=>row.target_mass)),
      initial_lessons:rows.filter(row=>row.ordinal===0).length,initial_top1:rows.filter(row=>row.ordinal===0&&row.top1).length,
      followup_lessons:rows.filter(row=>row.ordinal>0).length,followup_top1:rows.filter(row=>row.ordinal>0&&row.top1).length});
  }
  const continuationReport=read(path.join(p.continuations,'report.json')),teacher=unpack(path.join(p.continuations,'trajectories.json.gz')).find(t=>t.id===continuationReport.targets[0]);
  const largeEpisodes=r.rows.filter(row=>[p.large,p.original_large].includes(row.case)).map(row=>{
    const episode=unpack(path.join(row.directory,'episode.json.gz'));let prefix=0;
    if(row.case===p.large)while(prefix<teacher.summary.trace.length&&prefix<episode.trace.length&&JSON.stringify(teacher.summary.trace[prefix].action)===JSON.stringify(episode.trace[prefix].action))prefix++;
    return {...row,teacher_prefix:row.case===p.large?prefix:null,actions:episode.trace.map(t=>({action:t.action,status:t.status,score:t.current_score,best:t.best_score}))};
  });
  const updates=read(path.join(o.screen,'updates.json')),learning=[];
  for(const seed of p.config.seeds){
    const rows=updates.filter(row=>row.seed===seed),largeRoot=new Set(read(path.join(o.screen,'lessons.json')).filter(l=>l.root===p.large).map(l=>l.id));
    learning.push({seed,updates:rows.length,mean_first32_loss:mean(rows.slice(0,32).map(row=>row.loss)),mean_last32_loss:mean(rows.slice(-32).map(row=>row.loss)),
      max_gradient_norm:Math.max(...rows.map(row=>row.gradient_norm)),large_updates:rows.filter(row=>largeRoot.has(row.lesson)).length,
      first_large_pose_updates:rows.filter(row=>row.lesson===`${p.large}--lesson-0`).length});
  }
  write(o.output,{schema_version:1,status:'complete',report_sha256:hash(path.join(o.screen,'report.json')),audit_sha256:hash(path.join(o.screen,'audit.json')),
    measurements,comparisons,label_summary:labelSummary,large_labels:labels.filter(row=>row.root===p.large),original_large_labels:labels.filter(row=>row.root===p.original_large),learning,large_episodes:largeEpisodes,
    scope:'Opened-development comparison with two adaptation schedules from one fixed pretrained checkpoint; equal cold-runtime episode budgets'});
  console.log(JSON.stringify({status:'complete',measurements,learning,label_summary:labelSummary,large_scores:largeEpisodes.map(row=>({method:row.method,case:row.case,bank:row.bank,score:row.best_metrics.score,teacher_prefix:row.teacher_prefix}))}));
}
main(args(['screen','output']));
