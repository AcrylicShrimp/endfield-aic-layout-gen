// Usage: node run.mjs BASELINE_BINARY CANDIDATE_BINARY NEW_OUTPUT_DIRECTORY
// AIC_PLACEMENT_ROUNDS defaults to 1; set to 3+ for repeated release measurements.
// AIC_PLACEMENT_VARIANTS selects comma-separated baseline,zero,nearby-1,nearby-2,nearby-4,nearby-8.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
const root = fileURLToPath(new URL('../../../', import.meta.url));
if (process.argv.length !== 5) throw new Error('Expected baseline, candidate, new output directory');
const [baseline, candidate, output] = process.argv.slice(2).map(p => path.resolve(p));
const rounds = Number(process.env.AIC_PLACEMENT_ROUNDS ?? 1);
if (!Number.isSafeInteger(rounds) || rounds < 1 || rounds > 10) throw new Error('Expected 1..10 rounds');
const variants = {baseline: [baseline, null], zero: [candidate, 0], 'nearby-1': [candidate, 1], 'nearby-2': [candidate, 2], 'nearby-4': [candidate, 4], 'nearby-8': [candidate, 8]};
const placementBudget=Number(process.env.AIC_PLACEMENT_BUDGET??256);
const routeBudget=Number(process.env.AIC_PLACEMENT_ROUTE_BUDGET??4096);
if(!Number.isSafeInteger(placementBudget)||placementBudget<1||placementBudget>4096||!Number.isSafeInteger(routeBudget)||routeBudget<1||routeBudget>65536)throw new Error('Invalid experiment budget');
const names = (process.env.AIC_PLACEMENT_VARIANTS ?? 'baseline,zero,nearby-1,nearby-4').split(',');
if (names.some(n => !variants[n])) throw new Error('Unknown placement variant');
const hash = value => createHash('sha256').update(value).digest('hex');
const digest = value => hash(JSON.stringify(value));
const files = ['data/game/normalized/recipes.json','data/game/normalized/facilities.json',
  'data/game/normalized/items.json','data/game/normalized/transports.json','data/game/normalized/logistics-components.json',
  'data/examples/source-plan.game-heavy-xiranite-forge.request.json','data/examples/source-plan.game-seed-cycle-small.request.json'];
const summary = {schema_version: 1, baseline_revision: '0046d39',
  source_parent_revision: execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim(),
  platform: {os:os.type(),release:os.release(),arch:os.arch(),cpus:os.cpus().length},
  rustc:execFileSync('rustc',['--version'],{encoding:'utf8'}).trim(),
  build:'release; no compiler/test overlap during measured runs',
  budgets:{steps:256,placements:placementBudget,route_searches:routeBudget},
  binary_sha256:{baseline:hash(fs.readFileSync(baseline)),candidate:hash(fs.readFileSync(candidate))},
  input_sha256:Object.fromEntries(files.map(f=>[f,hash(fs.readFileSync(path.join(root,f)))])), runs:[]};
fs.mkdirSync(output);
const references = new Map();
function score(n) {
  if (!n) return null;
  const cells = new Set(n.transport_networks.flatMap(n=>n.cells.map(c=>`${n.transport}:${c.x}:${c.y}`)));
  let turns=0;
  for(const net of n.transport_networks) {
    const arms=new Map();
    const at=p=>{const k=`${p.x}:${p.y}`;if(!arms.has(k))arms.set(k,{ins:[],outs:[]});return arms.get(k);};
    for(const s of net.segments){const d=[s.to.x-s.from.x,s.to.y-s.from.y];at(s.from).outs.push(d);at(s.to).ins.push(d);}
    for(const c of net.cells){const a=at(c);for(const i of a.ins)for(const o of a.outs)if(i[0]!==o[0]||i[1]!==o[1])turns++;}
  }
  return [n.bounds.width*n.bounds.height,cells.size,turns];
}
for(const [workload,sourcePlan] of [['heavy-xiranite',files[5]],['seed-cycle',files[6]]]) {
  for(let round=0;round<rounds;round++) {
    const shift=round%names.length;
    for(const variant of [...names.slice(shift),...names.slice(0,shift)]) {
      const [binary,nearby]=variants[variant]; const prefix=path.join(output,`${workload}-${round}-${variant}`);
      const args=['layouts','construct-production','--recipes',files[0],'--source-plan',sourcePlan,
        '--facility-catalog',files[1],'--item-catalog',files[2],'--transport-catalog',files[3],
        '--logistics-component-catalog',files[4],'--target-instance','facility-instance:recipe-occurrence:/target:0',
        '--max-steps','256','--max-placement-attempts',String(placementBudget),'--max-route-searches',String(routeBudget),
        '--report-output',prefix+'.json','--visualization-output',prefix+'.html',
        ...(nearby===null?[]:['--nearby-candidates',String(nearby)])];
      const err=fs.openSync(prefix+'.stderr','wx');let stdoutBytes=0;const chunks=[];const start=performance.now();
      let code;try{code=await new Promise((resolve,reject)=>{const child=spawn('/usr/bin/time',['-lp',binary,...args],{cwd:root,stdio:['ignore','pipe',err]});
        child.stdout.on('data',c=>{stdoutBytes+=c.length;if(stdoutBytes<=4096)chunks.push(c);});
        child.on('error',reject);child.on('close',resolve);});}finally{fs.closeSync(err);}
      const wall=(performance.now()-start)/1000;
      if(stdoutBytes>4096)throw new Error('Oversized stdout');
      const stdout=Buffer.concat(chunks).toString('utf8');const completion=JSON.parse(stdout);
      const report=JSON.parse(fs.readFileSync(prefix+'.json','utf8')); const n=report.final_node;
      if(stdout.split('\n').length!==2||completion.kind!=='constructive-production-summary'||completion.success!==report.success||completion.report_output!==prefix+'.json'||!([0,1].includes(code)))throw new Error('Completion contract');
      const witness=digest({final:n,steps:report.steps.map(s=>({node:s.node,statistics:s.statistics,attempts:s.placement_attempts})),diagnostics:report.diagnostics,rejections:report.rejected_reasons,failed:report.failed_statistics});
      const key=workload+':'+(variant==='zero'?'baseline':variant);
      if(references.has(key)&&references.get(key)!==witness)throw new Error('Non-deterministic or changed zero-mode witness '+key);
      references.set(key,witness);
      const stderr=fs.readFileSync(prefix+'.stderr','utf8');
      const decisions=report.placement_decisions??[];
      const record={workload,round,variant,nearby_candidates:nearby,exit_code:code,complete:report.complete,status:report.status,
        wall_seconds:wall,planner_seconds:report.elapsed_ms/1000,
        user_seconds:Number(stderr.match(/^user (\S+)/m)?.[1]),system_seconds:Number(stderr.match(/^sys (\S+)/m)?.[1]),
        rss_bytes:Number(stderr.match(/^\s*(\d+)\s+maximum resident set size/m)?.[1]),
        score:score(n),bounds:n?.bounds,facilities:n?.member_instances.length,internal_requirements:n?.internal_requirements.length,external_requirements:n?.external_requirements.length,
        steps:report.steps.length,astar_calls:report.steps.reduce((s,r)=>s+r.statistics.astar_searches,report.failed_statistics.astar_searches),
        placement_attempts:report.steps.reduce((s,r)=>s+r.placement_attempts,report.failed_placement_attempts),
        selection:{calls:decisions.length,attempts:decisions.reduce((s,d)=>s+d.attempted,0),valid:decisions.reduce((s,d)=>s+d.valid,0),
          better_than_first:decisions.filter(d=>d.first_score&&d.selected_score&&JSON.stringify(d.first_score)!==JSON.stringify(d.selected_score)).length,
          no_valid:decisions.filter(d=>d.valid===0).length},
        last_decisions:decisions.slice(-4),diagnostics:report.diagnostics,rejected_reasons:report.rejected_reasons,
        failed_groups:report.attempted_groups_at_failure,unresolved_internal:report.unresolved_internal_requirements,
        unresolved_external:report.unresolved_external_requirements,
        obstruction:report.last_obstruction&&{connection:report.last_obstruction.connection,blocked:report.last_obstruction.blocked_requirements},
        witness_sha256:witness,final_node_sha256:digest(n),stdout_bytes:stdoutBytes,
        report_bytes:fs.statSync(prefix+'.json').size,html_bytes:fs.statSync(prefix+'.html').size};
      summary.runs.push(record);
      fs.writeFileSync(path.join(output,'summary.json'),JSON.stringify(summary,null,2)+'\n');
      console.log(JSON.stringify({workload,round,variant,complete:record.complete,facilities:record.facilities,score:record.score,wall,planner:record.planner_seconds,astar:record.astar_calls}));
    }
  }
}
