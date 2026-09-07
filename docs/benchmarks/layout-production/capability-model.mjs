import { extractChains, extentOf, reembed } from '../constructive-local-reembedding/model.mjs';
import { dependencyCones } from '../constructive-local-reembedding/groups.mjs';
import { planFacilityRotations } from '../constructive-local-reembedding/facility-rotation.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { prepareRoot, checkRoot } from '../layout-rl/coverage.mjs';
import { transform } from '../layout-rl/geometry.mjs';
import { digest } from '../layout-rl/io.mjs';
import { score, compare } from '../layout-rl/score.mjs';
import { density } from './metrics.mjs';

const directions = [[1,0],[-1,0],[0,1],[0,-1]];
const sorted = xs => [...xs].sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b)));
const staticCodes = new Set(['local-move-bounds','local-shared-terminal-move','local-shared-terminal-rotation',
  'local-body-collision','local-carried-collision','local-anchor-collision','local-terminal-collision',
  'local-endpoint-body','local-internal-body-collision','local-internal-anchor-collision','local-port-assignment','local-facility-rotation']);

export function productionIdentity(w) {
  return digest({ placements: sorted(w.placements.map(({x,y,width,height,rotation,...p})=>p)),
    utilizations: Object.fromEntries(Object.entries(w.utilizations).sort()),
    networks: sorted(w.transport_networks.map(n=>({ id:n.id,item:n.item,transport:n.transport,requirements:sorted(n.requirement_ids),
      terminals:sorted(n.terminals.map(({position,endpoint,...t})=>({ ...t,endpoint:endpoint.kind==='facility'
        ? Object.fromEntries(Object.entries(endpoint).filter(([k])=>k!=='port')) : endpoint }))) }))) });
}

export function validateLevel(level) {
  if(!level || typeof level.id!=='string' || !/^[a-z0-9-]+$/.test(level.id) || !['translate','rotate','port','reroute'].includes(level.kind)
    || !Number.isSafeInteger(level.step_limit) || level.step_limit<0 || level.step_limit>8
    || !Number.isSafeInteger(level.max_members) || level.max_members<1 || level.max_members>16
    || !['internal','downstream'].includes(level.junction_policy)
    || level.kind==='translate' && level.step_limit===0) throw new Error('Invalid capability level');
}

export function actionDomain(w,graph,catalogs,level) {
  validateLevel(level);
  const streams=[], shifts=Array.from({length:level.step_limit},(_,i)=>directions.map(([x,y])=>({dx:x*(i+1),dy:y*(i+1)}))).flat();
  const ids=new Set(w.placements.map(p=>p.instance));
  if(level.kind==='translate'){
    const groups=new Map(), add=members=>{const a=[...new Set(members)].sort();if(a.length<=level.max_members)groups.set(JSON.stringify(a),a);};
    w.placements.forEach(p=>add([p.instance]));
    const edges=graph.edges.filter(e=>ids.has(e.source)&&ids.has(e.target)).map(e=>({...e,internal:true}));
    if(level.max_members>=2)edges.forEach(e=>add([e.source,e.target]));
    if(level.max_members>2)for(const g of dependencyCones({nodes:[...ids].map(id=>({id})),edges},{maxMembers:level.max_members,maxDepth:4}))add(g.instances);
    for(const instances of groups.values())streams.push(shifts.map(s=>({moves:[{instances,...s}]})));
  }else if(level.kind==='rotate'){
    for(const p of w.placements){
      const rotations=catalogs.facilities.facilities.find(f=>f.id===p.facility).allowed_rotations.filter(r=>r!==p.rotation);
      streams.push([{dx:0,dy:0},...shifts].flatMap(s=>rotations.map(rotation=>({moves:[{instances:[p.instance],...s}],facilityRotations:[{instance:p.instance,rotation}]}))));
    }
  }else if(level.kind==='port'){
    const ports=project(w,catalogs);
    for(const n of w.transport_networks)for(const t of n.terminals)if(t.endpoint.kind==='facility')
      streams.push(ports[t.endpoint.instance].filter(p=>p.id!==t.endpoint.port && p.direction===t.direction && p.transport===n.transport)
        .map(p=>({portAssignments:[{terminal:t.id,port:p.id}]})));
  }else{
    const chains=extractChains(w).chains;
    for(const c of chains)streams.push([{chainIds:[c.id],order:'near'}]);
    streams.push(['near','far'].map(order=>({chainIds:chains.map(c=>c.id),order})));
  }
  const actions=[],seen=new Set();
  for(let depth=0;streams.some(s=>s.length>depth);depth++)for(const stream of streams)if(stream[depth]){
    const a=stream[depth],k=JSON.stringify(a);if(seen.has(k))continue;seen.add(k);actions.push(a);
    if(actions.length>100000)throw new Error('Capability raw action domain exceeds 100000');
  }
  return actions;
}

export function screenBodies(w,catalogs,action) {
  const moves=action.moves??[],changes=planFacilityRotations(w,catalogs,moves,action.facilityRotations??[]);
  const shifts=new Map(moves.flatMap(m=>m.instances.map(id=>[id,m]))),bounds=extentOf(w);
  const ps=w.placements.map(p=>changes.get(p.instance)?.pose??({...p,x:p.x+(shifts.get(p.instance)?.dx??0),y:p.y+(shifts.get(p.instance)?.dy??0)}));
  if(ps.some(p=>p.x<bounds.minX||p.y<bounds.minY||p.x+p.width-1>bounds.maxX||p.y+p.height-1>bounds.maxY))
    return {code:'local-move-bounds',severity:'info',stage:'capability-body-screen',message:'Facility body leaves the current-extent neighborhood'};
  for(let i=0;i<ps.length;i++)for(let j=i+1;j<ps.length;j++){
    const a=ps[i],b=ps[j];if(a.x<b.x+b.width&&b.x<a.x+a.width&&a.y<b.y+b.height&&b.y<a.y+a.height)
      return {code:'local-body-collision',severity:'info',stage:'capability-body-screen',message:'Proposed facility bodies overlap',entity:[w.placements[i].instance,w.placements[j].instance]};
  }
  return null;
}

export function validateBudget(b) {
  for(const k of ['proposals','route_expansions','per_action_expansions','elapsed_ms','validator_ms'])
    if(!Number.isSafeInteger(b?.[k])||b[k]<1)throw new Error(`Invalid capability budget: ${k}`);
  if(b.proposals>100000||b.route_expansions>100000000||b.per_action_expansions>200000||b.elapsed_ms>300000||b.validator_ms>30000)
    throw new Error('Capability operational budget exceeds diagnostic guard');
}

export async function probeLevel(root,catalogs,probe,level,budget) {
  validateBudget(budget);validateLevel(level);
  const graph=prepareRoot(root,catalogs),w=root.witness,rootHash=digest(w),identity=productionIdentity(w);
  const rootValidation=await probe.check(w,budget.validator_ms);
  if(!checkRoot(root,graph,w,rootValidation).factory_validated)throw new Error(`Invalid capability root: ${root.id}`);
  const started=performance.now(),domain=actionDomain(w,graph,catalogs,level),generation_ms=performance.now()-started;
  const initial=score(w,catalogs);let best=w,bestScore=initial;
  const detail={witness:w,labels:{},catalog_ports:project(w,catalogs),port_groups:{sources:[],targets:[]},embedding:'capability-private'};
  const chains=extractChains(w).chains,trace=[],seen=new Set([digest(transform(w))]);
  const work={generation_ms,screen_ms:0,kernel_ms:0,validation_ms:0,proposals:0,kernel_calls:0,route_calls:0,route_expansions:0,validations:0};
  const expired=()=>performance.now()-started>=budget.elapsed_ms;
  for(const [index,action]of domain.entries()){
    if(expired()||work.proposals>=budget.proposals||work.route_expansions>=budget.route_expansions)break;
    const began=performance.now(),row={index,action,status:'pending',route_expansions:0,routes:[]};work.proposals++;
    let candidate=null;
    try{
      const screening=performance.now();let failure;
      try{failure=screenBodies(w,catalogs,action);}finally{work.screen_ms+=performance.now()-screening;}
      if(failure){row.status='static-rejected';row.diagnostics=[failure];}
      else if(expired())row.status='late-result';
      else{
        const repairStart=performance.now();work.kernel_calls++;
        const distance=c=>Math.abs(c.cells[0].x-c.cells.at(-1).x)+Math.abs(c.cells[0].y-c.cells.at(-1).y);
        const priority=[...chains].sort((a,b)=>Number(a.transport!=='pipe')-Number(b.transport!=='pipe')
          || (action.order==='far'?-1:1)*(distance(a)-distance(b)) || a.id-b.id).map(c=>c.id);
        const allowance=Math.min(budget.per_action_expansions,budget.route_expansions-work.route_expansions);
        let result;
        try{result=reembed(detail,catalogs,{...action,endpointPolicy:'terminal-flexible',junctionPolicy:level.junction_policy,
          routePriority:priority,workBudget:allowance,totalWorkBudget:allowance,onRouteResult:r=>{row.route_expansions+=r.expanded;work.route_expansions+=r.expanded;work.route_calls++;row.routes.push(r);}});}
        finally{work.kernel_ms+=performance.now()-repairStart;}
        if(result.status==='rejected'){row.status='repair-failed';row.diagnostics=result.diagnostics;}
        else if(result.status==='unchanged')row.status='unchanged';
        else if(result.status==='candidate')candidate=transform(result.detail.witness);
        else throw new Error(`Unknown kernel result: ${result.status}`);
      }
    }catch(e){
      if(!staticCodes.has(e.code))throw e;
      row.status='static-rejected';row.diagnostics=[{code:e.code,severity:e.severity??'info',stage:e.stage??'capability-kernel',message:e.message,entity:e.entity??null}];
    }
    if(row.route_expansions>budget.per_action_expansions||work.route_expansions>budget.route_expansions)throw new Error('Capability routing budget invariant failed');
    if(candidate){
      if(productionIdentity(candidate)!==identity)throw new Error('Capability action changed original production identity');
      if(expired())row.status='late-result';
      else{
        const validationStart=performance.now();work.validations++;
        let validation;
        try{validation=await probe.check(candidate,budget.validator_ms);}finally{work.validation_ms+=performance.now()-validationStart;}
        const coverage=checkRoot(root,graph,candidate,validation);
        row.validation_sha256=digest({...validation,elapsed_ms:0});row.factory_validated=coverage.factory_validated===true;
        if(expired())row.status='late-result';
        else if(!row.factory_validated){row.status='validation-rejected';row.validation=validation;row.coverage=coverage;}
        else{
          const value=score(candidate,catalogs),id=digest(candidate);row.score=value.tuple;row.witness_sha256=id;
          row.status=seen.has(id)?'duplicate-valid':compare(value.tuple,initial.tuple)<0?'improved':'valid-no-gain';
          seen.add(id);
          if(compare(value.tuple,bestScore.tuple)<0){best=candidate;bestScore=value;row.new_incumbent=true;}
        }
      }
    }
    row.elapsed_ms=performance.now()-began;trace.push(row);
  }
  if(digest(w)!==rootHash)throw new Error('Capability kernel mutated the frozen root');
  const counts={},diagnostics={};for(const row of trace){counts[row.status]=(counts[row.status]??0)+1;for(const d of row.diagnostics??[])diagnostics[d.code]=(diagnostics[d.code]??0)+1;}
  return {schema_version:1,root:root.id,level,budget,initial:initial.tuple,best:bestScore.tuple,best_witness:best,
    best_density:density(best,catalogs),root_sha256:rootHash,root_validation_sha256:digest({...rootValidation,elapsed_ms:0}),factory_validated:true,
    domain_size:domain.length,visited:trace.length,domain_fraction:domain.length?trace.length/domain.length:1,
    enumeration_complete:trace.length===domain.length,unique_valid_states:seen.size-1,counts,diagnostics,work,trace,
    elapsed_ms:performance.now()-started,termination:trace.length===domain.length?'domain-exhausted':expired()?'elapsed-budget':work.proposals>=budget.proposals?'proposal-budget':'route-budget',
    no_opportunity_proven:false,global_infeasibility_proven:false};
}
