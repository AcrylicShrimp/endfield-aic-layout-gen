// Summarize audited return learning separately from frozen behavioral performance.
import path from 'node:path';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {args,read,write,hash} from '../../docs/benchmarks/layout-rl/io.mjs';
const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const cmp=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
const mean=a=>a.length?a.reduce((a,b)=>a+b,0)/a.length:null;
const distribution=a=>{const s=[...a].sort((a,b)=>a-b);return {count:s.length,min:s[0]??null,median:s.length?s[Math.floor(s.length/2)]:null,max:s.at(-1)??null,mean:mean(s)};};
const area=r=>100*(1-r.best_metrics.score[0]/r.initial_metrics.score[0]);
const empty=m=>100*m.empty_xy_cells/m.score[0];
const add=(a,b)=>{for(const [k,v] of Object.entries(b))a[k]=(a[k]??0)+v;return a;};

function paired(rows,method,reference){
  const selected=rows.filter(r=>r.method===method),result={wins:0,ties:0,losses:0};
  for(const row of selected){
    const other=rows.find(r=>r.method===reference&&r.case===row.case&&r.bank===row.bank);assert(other);
    const value=cmp(row.best_metrics.score,other.best_metrics.score);result[value<0?'wins':value>0?'losses':'ties']++;
  }
  return result;
}

function summarize(rows,proofRows,method){
  const selected=rows.filter(r=>r.method===method),operations=proofRows.filter(r=>selected.some(x=>`${x.method}:${x.case}:${x.bank}`===r.key)).flatMap(r=>r.operations);
  return {episodes:selected.length,complete_episodes:selected.filter(r=>r.budget_complete).length,mean_area_reduction_percent:mean(selected.map(area)),
    mean_final_empty_xy_percent:mean(selected.map(r=>empty(r.best_metrics))),seconds:selected.reduce((n,r)=>n+r.seconds,0),
    kinds:selected.reduce((a,r)=>add(a,r.kinds),{}),statuses:selected.reduce((a,r)=>add(a,r.statuses),{}),
    route_expansions:selected.reduce((n,r)=>n+r.route_expansions,0),pose_operations:operations.length,
    physically_translated_operations:operations.filter(o=>o.translated>0).length,physically_rotated_operations:operations.filter(o=>o.rotated>0).length,
    immediate_improving_pose_operations:operations.filter(o=>o.status==='improved'&&(o.translated||o.rotated)).length,
    immediate_area_improving_pose_operations:operations.filter(o=>o.status==='improved'&&o.after[0]<o.before[0]&&(o.translated||o.rotated)).length,
    improving_reroutes:selected.reduce((n,r)=>n+(r.improvements.reroute??0),0)};
}

function costs(groups,episodes){
  const sum=(rows,key)=>rows.reduce((n,r)=>n+r[key],0),seconds=sum(groups,'seconds'),episodeSeconds=sum(episodes,'seconds');
  const observation=sum(episodes,'observation_seconds'),selection=sum(episodes,'selection_seconds'),recording=sum(episodes,'recording_seconds');
  return {groups:groups.length,episodes:episodes.length,selected:sum(episodes,'selected'),group_seconds:seconds,
    episode_seconds:episodeSeconds,observation_seconds:observation,selection_seconds:selection,recording_seconds:recording,
    other_episode_seconds:episodeSeconds-observation-selection-recording,
    outside_episode_seconds:seconds-episodeSeconds,selected_per_group_second:seconds?sum(episodes,'selected')/seconds:null,
    group_seconds_distribution:distribution(groups.map(g=>g.seconds))};
}

function main(o){
  const p=read(path.join(o.screen,'protocol.json')),r=read(path.join(o.screen,'report.json')),proof=read(path.join(o.screen,'audit.json'));
  assert.equal(r.status,'complete');assert.equal(proof.status,'passed');assert.equal(proof.report_sha256,hash(path.join(o.screen,'report.json')));
  const training=read(path.join(p.training,'training-result.json')),trainingProof=read(path.join(p.training,'training-audit.json')),
    checkpointProof=read(path.join(p.training,'checkpoint-audit.json'));
  assert(['complete','budget_exhausted'].includes(training.status));assert.equal(trainingProof.status,'passed');
  assert.equal(trainingProof.report_sha256,hash(path.join(p.training,'training-result.json')));
  assert.equal(checkpointProof.status,'passed');assert.equal(checkpointProof.report_sha256,trainingProof.report_sha256);
  const methods={},groups={};
  for(const method of p.methods){methods[method]=summarize(r.rows,proof.rows,method);if(method!=='geometry')methods[method].versus_geometry=paired(r.rows,method,'geometry');}
  for(const group of new Set(p.cases.map(c=>c.transfer_group))){
    const rows=r.rows.filter(r=>r.transfer_group===group);groups[group]={};
    for(const method of p.methods){groups[group][method]=summarize(rows,proof.rows,method);if(method!=='geometry')groups[group][method].versus_geometry=paired(rows,method,'geometry');}
  }
  const trainingSummary={};
  for(const arm of training.arms){
    const name=arm.initialization,groupRows=training.groups.filter(g=>g.initialization===name),episodes=training.rows.filter(r=>r.initialization===name);
    const initial=`${name}-initial`,terminal=`${name}-terminal`,applied=episodes.filter(e=>e.group<arm.groups);
    methods[terminal].versus_initial=paired(r.rows,terminal,initial);
    for(const group of Object.keys(groups))groups[group][terminal].versus_initial=paired(r.rows.filter(r=>r.transfer_group===group),terminal,initial);
    const delayed=[],observationBytes=[],stateBytes=[];
    for(const row of episodes){
      const group=groupRows.find(g=>g.group===row.group),e=unpack(path.join(row.directory,'episode.json.gz'));
      observationBytes.push(e.max_observation_array_bytes);stateBytes.push(e.state_json_bytes);
      if(group&&group.returns[row.attempt]>0&&e.trace[0]?.action.kind==='pose'&&e.trace[0].status==='accepted_exploration'&&cmp(row.best_metrics.score,row.initial_metrics.score)<0){
        delayed.push({group:row.group,attempt:row.attempt,case:row.case,return:group.returns[row.attempt],initial:row.initial_metrics.score,
          after_first:e.trace[0].current_score,best:row.best_metrics.score,directory:row.directory});
      }
    }
    trainingSummary[name]={groups:groupRows.length,episodes:episodes.length,selected:episodes.reduce((n,r)=>n+r.selected,0),
      planned_groups:arm.planned_groups,completed_group_episodes:applied.length,unapplied_episodes:episodes.length-applied.length,
      gradient_replayed_selected:applied.reduce((n,r)=>n+r.selected,0),terminal_role:arm.terminal_role,
      nonflat_groups:groupRows.filter(g=>!g.flat_returns).length,reward_gradient_groups:groupRows.filter(g=>g.reward_gradient_norm>0).length,
      flat_groups_with_entropy_gradient:groupRows.filter(g=>g.flat_returns&&g.entropy_gradient_norm>0).length,
      reward_gradient_norm:distribution(groupRows.map(g=>g.reward_gradient_norm)),entropy_gradient_norm:distribution(groupRows.map(g=>g.entropy_gradient_norm)),
      seconds:arm.seconds,elapsed_seconds_lower_bound:arm.elapsed_seconds_lower_bound,parameters:checkpointProof.parameters,delayed_first_pose_positive_return_episodes:delayed,
      costs:costs(groupRows,applied),costs_by_case:Object.fromEntries([...new Set(groupRows.map(g=>g.case))].map(id=>
        [id,costs(groupRows.filter(g=>g.case===id),applied.filter(e=>e.case===id))])),
      unapplied_episode_seconds:episodes.filter(e=>e.group>=arm.groups).reduce((n,e)=>n+e.seconds,0),
      max_observation_array_bytes:distribution(observationBytes),episode_state_json_bytes:distribution(stateBytes),
      memory_scope:'Observation-array payload and uncompressed serialized episode-state bytes; not total process RSS, Python object overhead, native allocations or PyTorch workspace.',
      cost_scope:'Group cost tables include completed update groups only. Unapplied sampled episodes are timed separately. Outside-episode time includes gradient replay, optimizer/checkpoint work and orchestration. Other episode time includes runtime startup, execution/validation, final recording and teardown. A null arm duration is unavailable; its lower bound is not exact elapsed time.',
      caveat:'Positive terminal relative return credits every sampled action. This does not identify a causal contribution for each individual action.'};
  }
  const bestPrefixes=[];
  for(const row of r.rows.filter(r=>r.method.endsWith('-terminal')&&cmp(r.best_metrics.score,r.initial_metrics.score)<0)){
    const e=unpack(path.join(row.directory,'episode.json.gz')),index=e.trace.findIndex(t=>cmp(t.best_score,row.best_metrics.score)===0),prefix=e.trace.slice(0,index+1);
    bestPrefixes.push({method:row.method,case:row.case,transfer_group:row.transfer_group,selected_to_best:index+1,
      prefix_pose_actions:prefix.filter(t=>t.action.kind==='pose').length,prefix_reroute_actions:prefix.filter(t=>t.action.kind==='reroute').length,
      score:row.best_metrics.score,area_reduction_percent:area(row),empty_xy_percent:empty(row.best_metrics),directory:row.directory});
  }
  write(o.output,{schema_version:1,status:'complete',evaluation_report_sha256:hash(path.join(o.screen,'report.json')),audit_sha256:hash(path.join(o.screen,'audit.json')),
    training_result_sha256:hash(path.join(p.training,'training-result.json')),original_training_report_sha256:training.source_report_sha256,
    training_status:training.status,training_complete:training.training_complete,training_summary:trainingSummary,methods,groups,best_prefixes:bestPrefixes,
    scope:'One-seed on-policy pilot with actual completed update counts, paired initial/terminal controls and prospective known-family geometry transfer. A budget-ended arm does not complete its planned update count. No replicated architecture or 1%-target completion claim.'});
  console.log(JSON.stringify({status:'complete',training:trainingSummary,methods},null,2));
}
main(args(['screen','output']));
