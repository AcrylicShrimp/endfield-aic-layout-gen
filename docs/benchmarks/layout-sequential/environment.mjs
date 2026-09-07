import { actionDomain,screenBodies,productionIdentity } from '../layout-production/capability-model.mjs';
import { interactionDomain } from '../layout-production/interaction-model.mjs';
import { reembed,extractChains } from '../constructive-local-reembedding/model.mjs';
import { compactInterface } from '../constructive-auto-interface/compact.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { prepareRoot,checkRoot } from '../layout-rl/coverage.mjs';
import { transform } from '../layout-rl/geometry.mjs';
import { score,compare } from '../layout-rl/score.mjs';
import { observe,contextFor,tokenFields,actionFields as oldFields } from '../layout-rl/features.mjs';
import { digest } from '../layout-rl/io.mjs';
import { density,mechanicalWaste } from '../layout-production/metrics.mjs';

export { tokenFields };
export const actionFields=[...oldFields,'rotation_action','port_action','contact_action','compact_after','rotation_cos','rotation_sin','port_dx','port_dy'];
const staticCodes=new Set(['local-move-bounds','local-shared-terminal-move','local-shared-terminal-rotation','local-body-collision',
  'local-carried-collision','local-anchor-collision','local-terminal-collision','local-endpoint-body','local-internal-body-collision',
  'local-internal-anchor-collision','local-port-assignment','local-facility-rotation']);
const detail=(w,catalogs)=>({witness:w,labels:{},catalog_ports:project(w,catalogs),port_groups:{sources:[],targets:[]},embedding:'sequential-private'});

export class SequentialEnvironment {
  constructor(roots,catalogs,probe,plan){
    this.roots=new Map(roots.map(r=>[r.id,{...r,graph:prepareRoot(r,catalogs)}]));this.catalogs=catalogs;this.probe=probe;this.plan=plan;this.validRoots=new Set();
    for(const k of ['per_action_expansions','validator_ms','max_actions','max_tokens'])if(!Number.isSafeInteger(plan[k])||plan[k]<1)throw new Error(`Invalid environment option: ${k}`);
    if(!Array.isArray(plan.levels)||!plan.levels.length)throw new Error('Missing sequential action levels');
  }
  async reset(id,{steps=64,elapsed_ms=300000}={}){
    if(!Number.isSafeInteger(steps)||steps<1||steps>16384||!Number.isSafeInteger(elapsed_ms)||elapsed_ms<1||elapsed_ms>300000)throw new Error('Invalid sequential episode budget');
    this.root=this.roots.get(id);if(!this.root)throw new Error(`Unknown root: ${id}`);
    if(!this.validRoots.has(id)){
      const report=await this.probe.check(this.root.witness,this.plan.validator_ms);
      if(!checkRoot(this.root,this.root.graph,this.root.witness,report).factory_validated)throw new Error(`Invalid root: ${id}`);
      this.validRoots.add(id);
    }
    this.current=this.root.witness;this.best=this.current;this.currentScore=score(this.current,this.catalogs);this.bestScore=this.currentScore;
    this.initial=this.currentScore;this.identity=productionIdentity(this.current);this.limit=steps;this.elapsedLimit=elapsed_ms;this.started=performance.now();
    this.trace=[];this.seenPairs=new Set();this.cache=null;this.domainEnded=false;
    this.work={proposals:0,raw_generated:0,static_rejected:0,preprocess_ms:0,feature_ms:0,route_expansions:0,route_ms:0,compactions:0,compaction_ms:0,
      validations:0,validator_ms:0,accepted:0,worse_transitions:0,improvements:0};
    this.incumbents=[{proposal:0,score:this.initial.tuple,witness_sha256:digest(this.best)}];
    return this.observation();
  }
  expired(){return performance.now()-this.started>=this.elapsedLimit;}
  get done(){return this.work.proposals>=this.limit||this.expired()||this.domainEnded;}
  observation(){
    if(!this.root)throw new Error('Reset before observation');
    const began=performance.now(),state=digest(this.current);
    if(this.cache?.state!==state){
      const ports=project(this.current,this.catalogs),terminals=new Map(this.current.transport_networks.flatMap(n=>n.terminals.map(t=>[t.id,{...t,transport:n.transport}])));
      const occupiedPorts=new Set([...terminals.values()].filter(t=>t.endpoint.kind==='facility').map(t=>JSON.stringify([t.endpoint.instance,t.endpoint.port])));
      const raw=[],unique=new Set();
      for(const level of this.plan.levels){
        const domain=level.kind==='compact'?[{compactOnly:true}]:level.kind==='contact'
          ?interactionDomain(this.current,this.root.graph,this.catalogs,{...level,compact_after:false}).actions
          :actionDomain(this.current,this.root.graph,this.catalogs,level);
        for(const params of domain)for(const compact_after of level.kind==='compact'?[false]:this.plan.compaction_variants){
          const a={kind:level.kind,params,compact_after},key=JSON.stringify(a);if(unique.has(key))continue;unique.add(key);raw.push(a);
        }
      }
      this.work.raw_generated+=raw.length;
      const actions=[],fake=[],extras=[];
      for(const a of raw){
        let reject=false;
        if(!a.params.compactOnly)try{reject=screenBodies(this.current,this.catalogs,a.params)!==null;}catch(e){if(!staticCodes.has(e.code))throw e;reject=true;}
        let portDelta=[0,0],portTerminal=null;
        if(a.kind==='port'){
          const p=a.params.portAssignments[0],t=terminals.get(p.terminal);portTerminal=t;
          const q=ports[t.endpoint.instance].find(q=>q.id===p.port),position=q.connection;
          reject ||= occupiedPorts.has(JSON.stringify([t.endpoint.instance,p.port])) || this.current.placements.some(p=>position.x>=p.x&&position.x<p.x+p.width&&position.y>=p.y&&position.y<p.y+p.height);
          portDelta=[(position.x-t.position.x)/Math.max(this.initial.width,1),(position.y-t.position.y)/Math.max(this.initial.height,1)];
        }
        if(reject){this.work.static_rejected++;continue;}
        const move=a.params.moves?.[0],rotation=a.params.facilityRotations?.[0];
        const delta=rotation?rotation.rotation-this.current.placements.find(p=>p.instance===rotation.instance).rotation:0;
        actions.push(a);fake.push(a.params.compactOnly?{kind:'compact'}:{kind:'translate',instances:move?.instances??[portTerminal.endpoint.instance],dx:move?.dx??0,dy:move?.dy??0});
        extras.push([Number(a.kind==='rotate'),Number(a.kind==='port'),Number(a.kind==='contact'),Number(a.compact_after),Math.round(Math.cos(delta*Math.PI/180)),Math.round(Math.sin(delta*Math.PI/180)),...portDelta]);
      }
      if(actions.length>this.plan.max_actions)throw new Error(`Sequential action guard exceeded: ${actions.length} > ${this.plan.max_actions}`);
      this.work.preprocess_ms+=performance.now()-began;
      const featureStart=performance.now();
      const features=observe(this.current,this.root.graph,fake,this.currentScore,this.bestScore,this.initial,this.limit-this.work.proposals,this.limit,{max_tokens:this.plan.max_tokens,translation_steps:[1,2,4,8]});
      features.actions=features.actions.map((a,i)=>[...a,...extras[i]]);
      this.work.feature_ms+=performance.now()-featureStart;
      this.cache={state,actions,features,keys:actions.map(a=>`${state}:${digest(a)}`)};
    }
    this.actions=this.cache.actions;
    const mask=this.cache.keys.map(k=>!this.seenPairs.has(k));this.domainEnded=!mask.some(Boolean);
    const context=contextFor(this.currentScore,this.bestScore,this.initial,this.limit-this.work.proposals,this.limit);
    const features={...this.cache.features,actions:this.cache.features.actions.map(a=>[...a.slice(0,15),...context,...a.slice(oldFields.length)]),action_mask:mask};
    // A terminal padding action is only an encoding placeholder, never an executed proposal.
    if(this.done){features.actions=[Array(actionFields.length).fill(0)];features.action_mask=[true];}
    return {root:this.root.id,features,current_score:this.currentScore.tuple,best_score:this.bestScore.tuple,initial_score:this.initial.tuple,
      used:this.work.proposals,remaining:Math.max(0,this.limit-this.work.proposals),done:this.done,factory_validated:true};
  }
  async checked(candidate){
    if(productionIdentity(candidate)!==this.identity)throw new Error('Sequential action changed original production identity');
    if(this.expired())return {status:'late-result'};
    const began=performance.now();this.work.validations++;let report;
    try{report=await this.probe.check(candidate,this.plan.validator_ms);}finally{this.work.validator_ms+=performance.now()-began;}
    let coverage;
    try{coverage=checkRoot(this.root,this.root.graph,candidate,report);}catch(e){if(e.code!=='chain-factory-coverage-invalid')throw e;coverage={factory_validated:false,diagnostics:[{code:e.code,message:e.message}]};}
    return {status:this.expired()?'late-result':coverage.factory_validated?'valid':'validation-rejected',
      validation_sha256:digest({...report,elapsed_ms:0}),coverage};
  }
  retain(candidate,row){
    const value=score(candidate,this.catalogs);
    if(compare(value.tuple,this.bestScore.tuple)<0){this.best=candidate;this.bestScore=value;this.work.improvements++;
      this.incumbents.push({proposal:this.work.proposals,score:value.tuple,witness_sha256:digest(candidate)});row.new_incumbent=true;}
    return value;
  }
  async step(index){
    const observation=this.observation();if(observation.done)return {...observation,transition:{performed:false,accepted:false,status:'episode-ended'}};
    if(!Number.isSafeInteger(index)||!this.actions[index]||!observation.features.action_mask[index])throw new Error('Unknown or masked sequential action');
    const a=this.actions[index],before=this.current,beforeScore=this.currentScore,began=performance.now();this.seenPairs.add(this.cache.keys[index]);this.work.proposals++;
    const row={proposal:this.work.proposals,action:a,parent_sha256:digest(before),before:beforeScore.tuple,accepted:false,status:'pending',route_expansions:0,phases:[]};
    let candidate=null;
    try{
      if(a.params.compactOnly)candidate=transform(before);
      else{
        const chains=extractChains(before).chains,d=c=>Math.abs(c.cells[0].x-c.cells.at(-1).x)+Math.abs(c.cells[0].y-c.cells.at(-1).y);
        const routePriority=[...chains].sort((a,b)=>Number(a.transport!=='pipe')-Number(b.transport!=='pipe')||d(a)-d(b)||a.id-b.id).map(c=>c.id);
        const start=performance.now();let result;
        try{result=reembed(detail(before,this.catalogs),this.catalogs,{...a.params,endpointPolicy:'terminal-flexible',junctionPolicy:'internal',routePriority,
          workBudget:this.plan.per_action_expansions,totalWorkBudget:this.plan.per_action_expansions,onRouteResult:r=>{row.route_expansions+=r.expanded;this.work.route_expansions+=r.expanded;}});}
        finally{this.work.route_ms+=performance.now()-start;}
        row.status=result.status;row.diagnostics=result.diagnostics;
        if(result.status==='candidate')candidate=transform(result.detail.witness);else if(result.status==='unchanged')candidate=transform(before);
      }
      if(candidate){
        let validation=await this.checked(candidate);row.phases.push({stage:'intermediate',...validation});
        if(validation.status==='valid'){
          this.retain(candidate,row);
          if(a.compact_after||a.params.compactOnly){
            const start=performance.now();this.work.compactions++;
            try{candidate=transform(compactInterface(detail(candidate,this.catalogs),this.catalogs).witness);}finally{this.work.compaction_ms+=performance.now()-start;}
            validation=await this.checked(candidate);row.phases.push({stage:'compaction',...validation});
          }
          if(validation.status==='valid'){
            const value=this.retain(candidate,row);row.accepted=true;row.status='accepted';row.score=value.tuple;row.witness_sha256=digest(candidate);this.work.accepted++;
            if(compare(value.tuple,beforeScore.tuple)>0)this.work.worse_transitions++;
            this.current=candidate;this.currentScore=value;
          }else row.status=validation.status;
        }else row.status=validation.status;
      }
    }catch(e){if(!staticCodes.has(e.code))throw e;row.status=e.code;row.message=e.message;}
    if(row.route_expansions>this.plan.per_action_expansions||digest(before)!==row.parent_sha256)throw new Error('Sequential work/immutability invariant failed');
    row.elapsed_ms=performance.now()-began;row.best=this.bestScore.tuple;this.trace.push(row);
    return {...this.observation(),transition:row};
  }
  summary({witnesses=false,trace=false}={}){
    return {root:this.root.id,factory_validated:true,initial_score:this.initial,current_score:this.currentScore,best_score:this.bestScore,work:this.work,
      elapsed_ms:performance.now()-this.started,termination:this.expired()?'elapsed-budget':this.domainEnded?'offered-domain-exhausted':this.work.proposals>=this.limit?'proposal-budget':'caller-ended',
      incumbents:this.incumbents,best_witness_sha256:digest(this.best),density:density(this.best,this.catalogs),waste:mechanicalWaste({work:this.work,trace:this.trace}),
      ...(witnesses?{best_witness:this.best,current_witness:this.current}:{}),...(trace?{trace:this.trace}:{})};
  }
}
