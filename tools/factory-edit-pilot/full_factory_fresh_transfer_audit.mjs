// Replay every frozen-transfer selection against complete original factory requests.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
import {args,read,write,hash,Probe} from '../../docs/benchmarks/layout-rl/io.mjs';
import {prepareRoot,checkRoot} from '../../docs/benchmarks/layout-rl/coverage.mjs';
import {transform} from '../../docs/benchmarks/layout-rl/geometry.mjs';
import {describe,execute} from './full_factory_executor.mjs';
import {measure} from './full_factory_audit.mjs';
import {originalRequest} from './full_factory_inputs.mjs';

const unpack=p=>JSON.parse(gunzipSync(fs.readFileSync(p)));
const cmp=(a,b)=>{const i=a.findIndex((v,i)=>v!==b[i]);return i<0?0:Math.sign(a[i]-b[i]);};
const sorted=v=>Array.isArray(v)?v.map(sorted):v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,sorted(v[k])])):v;
const digest=v=>createHash('sha256').update(JSON.stringify(sorted(v))).digest('hex');
const equal=(a,b)=>digest(a)===digest(b);

function movement(before,after,action){
  const selected=new Set(action.poses.map(q=>before.placements[q.member].instance)),now=new Map(after.placements.map(p=>[p.instance,p]));
  const fixed=before.placements.filter(p=>!selected.has(p.instance)),q=action.poses[0],actual=now.get(before.placements[q.member].instance);
  const dx=fixed.length?now.get(fixed[0].instance).x-fixed[0].x:actual.x-q.x;
  const dy=fixed.length?now.get(fixed[0].instance).y-fixed[0].y:actual.y-q.y;
  for(const p of fixed)assert.deepEqual([now.get(p.instance).x-p.x,now.get(p.instance).y-p.y,now.get(p.instance).rotation-p.rotation],[dx,dy,0]);
  return {translated:before.placements.filter(p=>selected.has(p.instance)&&(now.get(p.instance).x-p.x!==dx||now.get(p.instance).y-p.y!==dy)).length,
    rotated:before.placements.filter(p=>selected.has(p.instance)&&now.get(p.instance).rotation!==p.rotation).length,frame_shift:[dx,dy]};
}

async function main(o){
  const p=read(path.join(o.screen,'protocol.json')),r=read(path.join(o.screen,'report.json')),admission=read(path.join(p.admission,'report.json'));
  assert.equal(p.training_performed,false);assert.equal(r.training_performed,false);assert.equal(r.policy_immutable,true);assert.equal(r.status,'complete');
  assert.equal(r.protocol_sha256,hash(path.join(o.screen,'protocol.json')));
  for(const [file,h] of Object.entries({...p.sources,...p.source_sha256}))assert.equal(hash(file),h,file);
  assert.deepEqual(p.methods,[...Object.keys(p.checkpoints),'geometry']);
  for(const [method,c] of Object.entries(p.checkpoints))assert.equal(hash(c.path),c.sha256,method);
  for(const [label,screen] of Object.entries(p.parent_screens)){
    const prior=read(path.join(screen,'protocol.json')),report=read(path.join(screen,'report.json')),proof=read(path.join(screen,'audit.json'));
    assert.equal(proof.status,'passed');assert.equal(proof.report_sha256,hash(path.join(screen,'report.json')));assert.equal(report.protocol_sha256,hash(path.join(screen,'protocol.json')));
    for(const seed of [17,29]){
      const checkpoint=p.checkpoints[`${label}-${seed}`];assert.equal(checkpoint.sha256,proof.checkpoints[`learned-${seed}`]);assert.equal(checkpoint.protocol_sha256,hash(path.join(screen,'protocol.json')));
    }
    assert.equal(p.checkpoints.parent.path,prior.checkpoint);assert.equal(hash(prior.checkpoint),p.checkpoints.parent.sha256);
    for(const key of ['schema','depth','width','native_library'])assert.equal(p[key],prior[key]);
  }
  assert.equal(admission.attempted,admission.rows.length);
  const accepted=admission.rows.filter(a=>a.status==='accepted'&&a.factory_validated);assert.equal(accepted.length,admission.accepted);
  assert.deepEqual(p.cases.map(c=>c.id),accepted.map(c=>c.id));
  const paths=read(p.catalogs),catalogs=Object.fromEntries(Object.entries(paths).map(([k,v])=>[k,read(v)])),roots=new Map();
  for(const c of p.cases){
    for(const [key,name] of [['initial','initial.json'],['request','request.json'],['original','original-request.json']])assert.equal(hash(c[key]),c.files_sha256[name]);
    const root={id:c.id,dialect:c.dialect,request:read(c.original),witness:read(c.initial)},graph=prepareRoot(root,catalogs),initial=transform(root.witness);
    assert.deepEqual(read(c.request),originalRequest(graph,c.dialect));assert(!roots.has(c.id));assert.equal(c.instances,initial.placements.length);assert.equal(c.split,'development');
    const row=accepted.find(a=>a.id===c.id);assert.equal(c.family,row.family);assert.equal(c.internal_pipe_requirements,row.internal_pipe_requirements);assert.deepEqual(measure(initial,catalogs),row.metrics);
    roots.set(c.id,{case:c,root,graph,initial});
  }
  const expected=new Set(p.methods.flatMap(m=>p.cases.flatMap(c=>p.config.banks.map(b=>`${m}:${c.id}:${b}`)))),rows=[];
  assert.equal(r.rows.length,expected.size);
  const probe=new Probe({...paths,validator:path.resolve(o.validator)});let replayed=0;
  try{
    for(const row of r.rows){
      const key=`${row.method}:${row.case}:${row.bank}`;assert(expected.delete(key));const data=roots.get(row.case),e=unpack(path.join(row.directory,'episode.json.gz')),w=unpack(path.join(row.directory,'best.json.gz'));
      assert.equal(row.family,data.case.family);assert.equal(row.split,data.case.split);assert.equal(row.instances,data.case.instances);assert.equal(row.transfer_group,data.case.transfer_group);
      assert.equal(row.selected,e.trace.length);assert(row.selected<=p.config.horizon);assert.deepEqual(row.initial_metrics,measure(data.initial,catalogs));assert.deepEqual(e.initial_metrics,row.initial_metrics);
      assert.equal(row.budget_complete,row.selected===p.config.horizon&&!e.trace.some(t=>t.status==='late'));
      assert.equal(e.sample_seed,row.bank);assert.equal(e.config.horizon,p.config.horizon);assert.equal(e.config.geometry,row.method==='geometry');assert.equal(e.config.memory,true);
      let current=data.initial,incumbent=current,routeWork=0;const operations=[],kinds={},statuses={},improvements={};
      assert(checkRoot(data.root,data.graph,current,await probe.check(current)).factory_validated);
      for(const [i,t] of e.trace.entries()){
        assert.equal(t.selected,i+1);assert(!t.diagnostics.some(d=>d.code==='factory.executor-error'));
        const d=describe(current,catalogs);assert(d.actions.some(action=>equal(action,t.action)),'Ordinary action membership');
        const result=t.action.kind==='restore_best'?{status:'candidate',witness:incumbent,route_expansions:0}:execute(current,catalogs,t.action,p.config.route_work);
        assert.equal(result.status,t.executor_status);assert.equal(result.route_expansions,t.route_expansions);assert(t.route_expansions<=p.config.route_work);
        const valid=Boolean(result.witness&&checkRoot(data.root,data.graph,result.witness,await probe.check(result.witness)).factory_validated),candidate=valid?transform(result.witness):current;
        const metrics=measure(candidate,catalogs),improving=valid&&cmp(metrics.score,measure(incumbent,catalogs).score)<0;
        const status=!result.witness?'executor_rejected':!valid?'rejected':improving?'improved':cmp(metrics.score,measure(current,catalogs).score)===0?'accepted_equal':'accepted_exploration';
        if(t.status==='late')assert(t.elapsed_seconds>=p.config.episode_seconds);
        else{
          assert.equal(t.status,status);assert(t.elapsed_seconds<p.config.episode_seconds);
          if(valid){
            if(t.action.kind==='pose')operations.push({selected:i+1,status:t.status,...movement(current,candidate,t.action),before:measure(current,catalogs).score,after:metrics.score});
            current=candidate;if(improving)incumbent=current;
          }
        }
        assert.deepEqual(measure(current,catalogs).score,t.current_score);assert.deepEqual(measure(incumbent,catalogs).score,t.best_score);
        if(t.current_sha256)assert.equal(digest(current),t.current_sha256);
        kinds[t.action.kind]=(kinds[t.action.kind]??0)+1;statuses[t.status]=(statuses[t.status]??0)+1;routeWork+=t.route_expansions;
        if(t.status==='improved')improvements[t.action.kind]=(improvements[t.action.kind]??0)+1;
        replayed++;
      }
      assert.equal(digest(incumbent),e.best_sha256);assert.equal(digest(incumbent),digest(w));assert(checkRoot(data.root,data.graph,w,await probe.check(w)).factory_validated);
      assert.deepEqual(measure(w,catalogs),row.best_metrics);assert.deepEqual(e.best_metrics,row.best_metrics);assert(cmp(row.best_metrics.score,row.initial_metrics.score)<=0);
      assert.deepEqual(kinds,row.kinds);assert.deepEqual(statuses,row.statuses);assert.deepEqual(improvements,row.improvements);assert.equal(routeWork,row.route_expansions);
      rows.push({key,method:row.method,case:row.case,bank:row.bank,selected:row.selected,factory_validated:true,score:row.best_metrics.score,operations,
        episode_sha256:hash(path.join(row.directory,'episode.json.gz')),best_sha256:hash(path.join(row.directory,'best.json.gz'))});
      if(rows.length%10===0)console.log(JSON.stringify({episodes:rows.length,replayed_actions:replayed}));
    }
    assert.equal(expected.size,0);
    write(o.output,{schema_version:1,status:'passed',report_sha256:hash(path.join(o.screen,'report.json')),episodes:rows.length,replayed_actions:replayed,
      admission_attempts:admission.attempted,admission_accepted:accepted.length,rows,checkpoints:p.checkpoints,
      scope:'All frozen-transfer actions and incumbents: original request/graph/native validity, independent metrics, ordinary action membership, exact incumbent hashes and relative facility movement'});
    console.log(JSON.stringify({status:'passed',episodes:rows.length,replayed_actions:replayed}));
  }finally{probe.close();}
}
main(args(['screen','validator','output'])).catch(e=>{console.error(e.stack);process.exitCode=1;});
