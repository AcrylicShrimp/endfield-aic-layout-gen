// Compare matching development episodes and verify repeated controls are unchanged.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {gunzipSync} from 'node:zlib';
import {args,read,write,hash} from '../../docs/benchmarks/layout-rl/io.mjs';
const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const compare=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
function main(o){
  const p=read(path.join(o.screen,'protocol.json')),r=read(path.join(o.screen,'report.json')),a=read(path.join(o.screen,'audit.json')),
    prior=read(path.join(p.prior_screen,'protocol.json')),before=read(path.join(p.prior_screen,'report.json')),beforeAudit=read(path.join(p.prior_screen,'audit.json'));
  assert.equal(r.status,'complete');assert.equal(a.status,'passed');assert.equal(a.report_sha256,hash(path.join(o.screen,'report.json')));
  assert.equal(before.status,'complete');assert.equal(beforeAudit.status,'passed');assert.equal(beforeAudit.report_sha256,hash(path.join(p.prior_screen,'report.json')));
  assert.deepEqual(p.cases,prior.cases);assert.equal(p.checkpoint,prior.checkpoint);
  const permitted=new Set(['role_weights','updates','observation_cache_bytes']),changes={};
  for(const key of new Set([...Object.keys(p.config),...Object.keys(prior.config)])){
    if(JSON.stringify(p.config[key])===JSON.stringify(prior.config[key]))continue;
    assert(permitted.has(key),key);changes[key]={before:prior.config[key],after:p.config[key]};
  }
  const group=row=>row.case===p.large?'prepared-large':row.case===p.original_large?'original-large':row.split==='train'?'rehearsal':row.family;
  const groups=new Map(),rows=[],controlRows=[];
  const semantic=e=>({initial_metrics:e.initial_metrics,best_metrics:e.best_metrics,selected:e.selected,best_sha256:e.best_sha256,
    trace:e.trace.map(t=>Object.fromEntries(['action','status','executor_status','route_expansions','current_score','best_score','physical_before','physical_after','masked'].map(k=>[k,t[k]])))});
  for(const row of r.rows){
    const old=before.rows.find(old=>old.method===row.method&&old.case===row.case&&old.bank===row.bank);assert(old);
    if(['parent','geometry'].includes(row.method)){
      assert.deepEqual(semantic(unpack(path.join(row.directory,'episode.json.gz'))),semantic(unpack(path.join(old.directory,'episode.json.gz'))));
      controlRows.push({method:row.method,case:row.case,bank:row.bank,score:row.best_metrics.score});
      continue;
    }
    const key=`${row.method}:${group(row)}`;
    if(!groups.has(key))groups.set(key,{method:row.method,group:group(row),wins:0,ties:0,losses:0});
    const c=compare(row.best_metrics.score,old.best_metrics.score);groups.get(key)[c<0?'wins':c>0?'losses':'ties']++;
    rows.push({method:row.method,case:row.case,bank:row.bank,before:old.best_metrics.score,after:row.best_metrics.score,comparison:c});
  }
  write(o.output,{schema_version:1,status:'passed',report_sha256:hash(path.join(o.screen,'report.json')),prior_report_sha256:hash(path.join(p.prior_screen,'report.json')),
    unchanged_controls:controlRows.length,control_rows:controlRows,comparisons:[...groups.values()],rows,training_config_changes:changes,
    scope:'Same pretrained parent and inference budgets; training configuration differences recorded; all repeated control actions/statuses/work/metrics match'});
  console.log(JSON.stringify({status:'passed',unchanged_controls:controlRows.length,comparisons:[...groups.values()]}));
}
main(args(['screen','output']));
