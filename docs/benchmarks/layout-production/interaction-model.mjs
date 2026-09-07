import { actionDomain,validateLevel,validateBudget,productionIdentity,screenBodies } from './capability-model.mjs';
import { emptyRectangles } from '../constructive-local-reembedding/groups.mjs';
import { extractChains,reembed } from '../constructive-local-reembedding/model.mjs';
import { compactInterface } from '../constructive-auto-interface/compact.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { prepareRoot,checkRoot } from '../layout-rl/coverage.mjs';
import { transform } from '../layout-rl/geometry.mjs';
import { score,compare } from '../layout-rl/score.mjs';
import { digest } from '../layout-rl/io.mjs';
import { density } from './metrics.mjs';

const staticCodes=new Set(['local-move-bounds','local-shared-terminal-move','local-shared-terminal-rotation','local-body-collision',
  'local-carried-collision','local-anchor-collision','local-terminal-collision','local-endpoint-body','local-internal-body-collision',
  'local-internal-anchor-collision','local-port-assignment','local-facility-rotation']);

export function validateInteraction(level) {
  if(typeof level?.compact_after!=='boolean'||!['compact','translate','rotate','port','void','contact'].includes(level.kind))throw new Error('Invalid interaction level');
  validateLevel({...level,kind:['compact','void','contact'].includes(level.kind)?'rotate':level.kind});
}

export function interactionDomain(w,graph,catalogs,level) {
  validateInteraction(level);
  if(level.kind==='compact')return {actions:[{compactOnly:true}],geometry_selection:null};
  if(!['void','contact'].includes(level.kind))return {actions:actionDomain(w,graph,catalogs,level),geometry_selection:null};
  const groupMap=new Map();
  for(const a of actionDomain(w,graph,catalogs,{...level,kind:'translate',step_limit:1})){
    const ids=a.moves[0].instances;groupMap.set(JSON.stringify(ids),ids);
  }
  const rectangles=level.kind==='void'?emptyRectangles(w,8):[],streams=[];
  for(const instances of groupMap.values()){
    const ids=new Set(instances),moving=w.placements.filter(p=>ids.has(p.instance));
    const x=Math.min(...moving.map(p=>p.x)),y=Math.min(...moving.map(p=>p.y));
    const width=Math.max(...moving.map(p=>p.x+p.width))-x,height=Math.max(...moving.map(p=>p.y+p.height))-y;
    const stream=[];
    const add=(tx,ty)=>{if(tx!==x||ty!==y)stream.push({moves:[{instances,dx:tx-x,dy:ty-y}]});};
    if(level.kind==='void'){
      for(const r of rectangles)if(width<=r.width&&height<=r.height)
        for(const tx of [r.x,r.x+r.width-width])for(const ty of [r.y,r.y+r.height-height])add(tx,ty);
    }else{
      const neighbors=new Set(graph.edges.filter(e=>ids.has(e.source)!==ids.has(e.target)).map(e=>ids.has(e.source)?e.target:e.source));
      for(const p of w.placements.filter(p=>neighbors.has(p.instance)&&!ids.has(p.instance)))for(const gap of [0,1]){
        for(const tx of [p.x-width-gap,p.x+p.width+gap])for(const ty of [p.y,p.y+p.height-height])add(tx,ty);
        for(const ty of [p.y-height-gap,p.y+p.height+gap])for(const tx of [p.x,p.x+p.width-width])add(tx,ty);
      }
    }
    streams.push(stream);
  }
  const actions=[],seen=new Set();
  for(let depth=0;streams.some(s=>s.length>depth);depth++)for(const s of streams)if(s[depth]){
    const k=JSON.stringify(s[depth]);if(seen.has(k))continue;seen.add(k);actions.push(s[depth]);
    if(actions.length>100000)throw new Error('Interaction domain exceeds 100000 actions');
  }
  return {actions,geometry_selection:{kind:level.kind,groups:groupMap.size,rectangles,scope:level.kind==='void'?'four corners of up to eight selected fitting voids':'internal-dependency contacts, four sides, two gaps and two alignments'}};
}

export async function interactionLevel(root,catalogs,probe,level,budget) {
  validateInteraction(level);validateBudget(budget);
  const graph=prepareRoot(root,catalogs),w=root.witness,originalHash=digest(w),identity=productionIdentity(w);
  const validation=await probe.check(w,budget.validator_ms);
  if(!checkRoot(root,graph,w,validation).factory_validated)throw new Error(`Invalid interaction root: ${root.id}`);
  const start=performance.now(),domain=interactionDomain(w,graph,catalogs,level),generation_ms=performance.now()-start;
  const makeDetail=w=>({witness:w,labels:{},catalog_ports:project(w,catalogs),port_groups:{sources:[],targets:[]},embedding:'interaction-private'});
  const detail=makeDetail(w),chains=extractChains(w).chains;
  const distance=c=>Math.abs(c.cells[0].x-c.cells.at(-1).x)+Math.abs(c.cells[0].y-c.cells.at(-1).y);
  const priority=[...chains].sort((a,b)=>Number(a.transport!=='pipe')-Number(b.transport!=='pipe')||distance(a)-distance(b)||a.id-b.id).map(c=>c.id);
  let best=w,bestScore=score(w,catalogs);const initial=bestScore.tuple,trace=[];
  const seen=new Set([digest(transform(w))]);
  const work={generation_ms,screen_ms:0,kernel_ms:0,compaction_ms:0,validation_ms:0,proposals:0,kernel_calls:0,route_calls:0,route_expansions:0,compactions:0,validations:0};
  const expired=()=>performance.now()-start>=budget.elapsed_ms;
  const check=async(candidate,stage)=>{
    if(productionIdentity(candidate)!==identity)throw new Error(`Interaction changed production identity at ${stage}`);
    if(expired())return {stage,status:'late-result'};
    const began=performance.now();work.validations++;let report;
    try{report=await probe.check(candidate,budget.validator_ms);}finally{work.validation_ms+=performance.now()-began;}
    let coverage;
    try{coverage=checkRoot(root,graph,candidate,report);}
    catch(e){
      if(e.code!=='chain-factory-coverage-invalid')throw e;
      coverage={factory_validated:false,diagnostics:[{code:e.code,severity:e.severity,stage:e.stage,message:e.message,entity:e.entity??null}]};
    }
    const valid=coverage.factory_validated===true;
    return {stage,status:expired()?'late-result':valid?'valid':'validation-rejected',factory_validated:valid,
      validation_sha256:digest({...report,elapsed_ms:0}),...(valid?{score:score(candidate,catalogs).tuple,witness_sha256:digest(candidate)}:{validation:report,coverage})};
  };
  const retain=(candidate,result)=>{
    if(result.status==='valid'&&compare(result.score,bestScore.tuple)<0){best=candidate;bestScore=score(candidate,catalogs);result.new_incumbent=true;}
  };
  for(const [index,action]of domain.actions.entries()){
    if(expired()||work.proposals>=budget.proposals||work.route_expansions>=budget.route_expansions)break;
    const began=performance.now(),row={index,action,status:'pending',routes:[],route_expansions:0,phases:[]};work.proposals++;
    let candidate=null;
    if(action.compactOnly)candidate=transform(w);
    else try{
      const screening=performance.now();let failure;
      try{failure=screenBodies(w,catalogs,action);}finally{work.screen_ms+=performance.now()-screening;}
      if(failure){row.status='static-rejected';row.diagnostics=[failure];}
      else if(expired())row.status='late-result';
      else{
        const began=performance.now(),allowance=Math.min(budget.per_action_expansions,budget.route_expansions-work.route_expansions);work.kernel_calls++;
        let result;
        try{result=reembed(detail,catalogs,{...action,endpointPolicy:'terminal-flexible',junctionPolicy:level.junction_policy,
          routePriority:priority,totalWorkBudget:allowance,workBudget:allowance,onRouteResult:r=>{row.routes.push(r);row.route_expansions+=r.expanded;work.route_calls++;work.route_expansions+=r.expanded;}});}
        finally{work.kernel_ms+=performance.now()-began;}
        if(result.status==='rejected'){row.status='repair-failed';row.diagnostics=result.diagnostics;}
        else if(result.status==='unchanged')candidate=transform(w);
        else if(result.status==='candidate')candidate=transform(result.detail.witness);
        else throw new Error(`Unexpected interaction kernel result: ${result.status}`);
      }
    }catch(e){
      if(!staticCodes.has(e.code))throw e;
      row.status='static-rejected';row.diagnostics=[{code:e.code,severity:e.severity??'info',stage:e.stage??'interaction-kernel',message:e.message,entity:e.entity??null}];
    }
    if(work.route_expansions>budget.route_expansions||row.route_expansions>budget.per_action_expansions)throw new Error('Interaction expansion accounting invariant failed');
    if(candidate){
      const intermediate=await check(candidate,'intermediate');row.phases.push(intermediate);retain(candidate,intermediate);
      if(intermediate.status!=='valid')row.status=intermediate.status;
      else{
        let last=intermediate;
        if(level.compact_after||action.compactOnly){
          if(expired()){last={stage:'compaction',status:'late-result'};row.phases.push(last);}
          else{
            const began=performance.now();work.compactions++;let result;
            try{result=compactInterface(makeDetail(candidate),catalogs);}finally{work.compaction_ms+=performance.now()-began;}
            const compacted=transform(result.witness);last=await check(compacted,'compaction');
            last.removed_columns=result.compaction.removed_columns;last.removed_rows=result.compaction.removed_rows;
            row.phases.push(last);retain(compacted,last);
            if(last.status==='valid'){
              row.compaction_improved=compare(last.score,intermediate.score)<0;
              row.compaction_enabled_root_improvement=compare(intermediate.score,initial)>=0&&compare(last.score,initial)<0;
              candidate=compacted;
            }
          }
        }
        if(last.status!=='valid')row.status=last.status;
        else{
          row.score=last.score;row.witness_sha256=last.witness_sha256;
          row.status=seen.has(last.witness_sha256)?'duplicate-valid':compare(last.score,initial)<0?'improved':'valid-no-gain';
          seen.add(last.witness_sha256);
        }
      }
    }
    row.elapsed_ms=performance.now()-began;trace.push(row);
  }
  if(digest(w)!==originalHash)throw new Error('Interaction mutated the original root');
  const counts={},diagnostics={};for(const r of trace){counts[r.status]=(counts[r.status]??0)+1;for(const d of r.diagnostics??[])diagnostics[d.code]=(diagnostics[d.code]??0)+1;}
  return {schema_version:1,root:root.id,level,budget,initial,best:bestScore.tuple,best_witness:best,best_density:density(best,catalogs),
    factory_validated:true,root_sha256:originalHash,geometry_selection:domain.geometry_selection,domain_size:domain.actions.length,visited:trace.length,
    enumeration_complete:trace.length===domain.actions.length,counts,diagnostics,work,trace,unique_final_states:seen.size-1,
    compaction_improvements:trace.filter(r=>r.compaction_improved).length,compaction_enabled_root_improvements:trace.filter(r=>r.compaction_enabled_root_improvement).length,
    elapsed_ms:performance.now()-start,termination:trace.length===domain.actions.length?'domain-exhausted':expired()?'elapsed-budget':work.proposals>=budget.proposals?'proposal-budget':'route-budget',
    global_infeasibility_proven:false};
}
