import { reembed,extractChains,extentOf } from '../constructive-local-reembedding/model.mjs';
import { compactInterface } from '../constructive-auto-interface/compact.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { prepareRoot,checkRoot } from '../layout-rl/coverage.mjs';
import { transform } from '../layout-rl/geometry.mjs';
import { score,compare } from '../layout-rl/score.mjs';
import { observe,contextFor } from '../layout-rl/features.mjs';
import { digest } from '../layout-rl/io.mjs';
import { productionIdentity } from '../layout-production/capability-model.mjs';
import { density,mechanicalWaste } from '../layout-production/metrics.mjs';

const normalRejections=new Set(['local-move-bounds','local-shared-terminal-move','local-shared-terminal-rotation','local-body-collision',
  'local-carried-collision','local-anchor-collision','local-terminal-collision','local-endpoint-body','local-internal-body-collision',
  'local-internal-anchor-collision','local-port-assignment','local-facility-rotation']);
const detail=(w,catalogs)=>({witness:w,labels:{},catalog_ports:project(w,catalogs),port_groups:{sources:[],targets:[]},embedding:'direct-private'});
export class DirectEnvironment {
  constructor(catalogs,probe,plan){
    for(const k of ['grid_cells','max_tokens','max_members','per_action_expansions','validator_ms'])if(!Number.isSafeInteger(plan?.[k])||plan[k]<1)throw new Error(`Invalid direct environment option: ${k}`);
    if(plan.max_members>16||plan.grid_cells>1000000||plan.per_action_expansions>200000)throw new Error('Direct environment guard exceeds execution support');
    this.catalogs=catalogs;this.probe=probe;this.plan=plan;
  }
  async reset(root,{steps=256,elapsed_ms=300000}={}){
    if(!Number.isSafeInteger(steps)||steps<1||steps>16384||!Number.isSafeInteger(elapsed_ms)||elapsed_ms<1||elapsed_ms>300000)throw new Error('Invalid direct episode budget');
    this.root={...root,graph:prepareRoot(root,this.catalogs)};
    if(!checkRoot(this.root,this.root.graph,root.witness,await this.probe.check(root.witness)).factory_validated)throw new Error('Direct reset requires a complete independently valid factory');
    this.current=transform(root.witness);this.best=this.current;this.currentScore=score(this.current,this.catalogs);this.bestScore=this.currentScore;this.initial=this.currentScore;
    this.identity=productionIdentity(this.current);this.limit=steps;this.elapsedLimit=elapsed_ms;this.started=performance.now();this.cache=null;this.trace=[];
    this.work={proposals:0,preprocess_ms:0,route_expansions:0,route_ms:0,compactions:0,compaction_ms:0,validations:0,validator_ms:0,accepted:0,worse_transitions:0,improvements:0};
    this.incumbents=[{proposal:0,score:this.initial.tuple,witness_sha256:digest(this.best)}];return this.observation();
  }
  expired(){return performance.now()-this.started>=this.elapsedLimit;}
  get done(){return this.work.proposals>=this.limit||this.expired();}
  observation(){
    const start=performance.now(),state=digest(this.current);
    if(this.cache?.state!==state){
      const w=this.current,b=extentOf(w),width=b.maxX-b.minX+1,height=b.maxY-b.minY+1,area=width*height;
      if(area>this.plan.grid_cells)throw new Error(`Direct observation grid guard exceeded: ${area}`);
      const grid=Array.from({length:10},()=>Array(area).fill(0)),at=p=>(p.y-b.minY)*width+p.x-b.minX;
      for(const p of w.placements)for(let y=0;y<p.height;y++)for(let x=0;x<p.width;x++)grid[0][at({x:p.x+x,y:p.y+y})]=1;
      for(const n of w.transport_networks){
        const channel=n.transport==='belt'?1:2;
        for(const p of n.cells)grid[channel][at(p)]=1;
        for(const s of n.segments){const horizontal=s.from.y===s.to.y,c=n.transport==='belt'?(horizontal?6:7):(horizontal?8:9);
          grid[c][at(s.from)]=Math.sign(horizontal?s.to.x-s.from.x:s.to.y-s.from.y);grid[c][at(s.to)]=grid[c][at(s.from)];}
        for(const t of n.terminals)grid[t.direction==='input'?4:5][at(t.position)]=1;
      }
      for(const c of w.logistics_components){
        const spec=this.catalogs.components.components.find(v=>v.id===c.component),turn=c.rotation%180!==0;
        const cw=spec.footprint[turn?'height':'width'],ch=spec.footprint[turn?'width':'height'];
        for(let dy=0;dy<ch;dy++)for(let dx=0;dx<cw;dx++)grid[3][at({x:c.position.x+dx,y:c.position.y+dy})]=1;
      }
      const graph=observe(w,this.root.graph,[{kind:'compact'}],this.currentScore,this.bestScore,this.initial,this.limit-this.work.proposals,this.limit,
        {max_tokens:this.plan.max_tokens,translation_steps:[1]}),ports=project(w,this.catalogs);
      this.cache={state,bounds:{...b,width,height},grid,tokens:graph.tokens,adjacency:graph.adjacency,
        facilities:w.placements.map(p=>({...p,ports:ports[p.instance],rotations:this.catalogs.facilities.facilities.find(f=>f.id===p.facility).allowed_rotations})),
        terminals:w.transport_networks.flatMap(n=>n.terminals.map(t=>({...t,network:n.id,transport:n.transport}))),
        edges:this.root.graph.edges.filter(e=>e.internal).map(({source,target})=>({source,target})),
        networks:w.transport_networks,logistics_components:w.logistics_components,
        chains:extractChains(w).chains.map(c=>({id:c.id,network:w.transport_networks[c.network].id,transport:c.transport,start:c.cells[0],end:c.cells.at(-1),cells:c.cells.length})),
        utilizations:w.utilizations,max_members:this.plan.max_members};
      this.work.preprocess_ms+=performance.now()-start;
    }
    return {schema_version:1,root:this.root.id,done:this.done,used:this.work.proposals,remaining:Math.max(0,this.limit-this.work.proposals),
      initial_score:this.initial.tuple,current_score:this.currentScore.tuple,best_score:this.bestScore.tuple,
      context:contextFor(this.currentScore,this.bestScore,this.initial,this.limit-this.work.proposals,this.limit),state:this.cache};
  }
  params(action){
    if(!action||!['relocate','pose','port','reroute','compact'].includes(action.kind)||typeof action.compact_after!=='boolean')throw new Error('Malformed direct action DTO');
    const ids=new Set(this.current.placements.map(p=>p.instance));
    if(action.kind==='relocate'||action.kind==='pose'){
      if(!Array.isArray(action.instances)||!action.instances.length||action.instances.length>this.plan.max_members||new Set(action.instances).size!==action.instances.length
        ||action.instances.some(id=>!ids.has(id))||!Number.isSafeInteger(action.dx)||!Number.isSafeInteger(action.dy))throw new Error('Malformed direct movement DTO');
      const seen=new Set([action.instances[0]]);let changed=true;
      while(changed){changed=false;for(const e of this.root.graph.edges.filter(e=>e.internal))if(action.instances.includes(e.source)&&action.instances.includes(e.target)&&seen.has(e.source)!==seen.has(e.target)){seen.add(e.source);seen.add(e.target);changed=true;}}
      if(seen.size!==action.instances.length)throw new Error('Direct movement group must be connected');
      const params={moves:[{instances:action.instances,dx:action.dx,dy:action.dy}]};
      if(action.kind==='pose'){
        const p=this.current.placements.find(p=>p.instance===action.instances[0]);
        if(action.instances.length!==1||!this.catalogs.facilities.facilities.find(f=>f.id===p.facility).allowed_rotations.includes(action.rotation))throw new Error('Malformed direct pose DTO');
        params.facilityRotations=[{instance:p.instance,rotation:action.rotation}];
      }
      return params;
    }
    if(action.kind==='port')return {portAssignments:[{terminal:action.terminal,port:action.port}]};
    if(action.kind==='reroute'){
      if(!Number.isSafeInteger(action.chain)||!['near','far'].includes(action.order))throw new Error('Malformed direct reroute DTO');
      return {chainIds:[action.chain],order:action.order};
    }
    return {};
  }
  async checked(candidate){
    if(productionIdentity(candidate)!==this.identity)throw new Error('Direct mutation changed original production identity');
    if(this.expired())return {status:'late-result'};
    const start=performance.now();this.work.validations++;let report;
    try{report=await this.probe.check(candidate,this.plan.validator_ms);}finally{this.work.validator_ms+=performance.now()-start;}
    let coverage;try{coverage=checkRoot(this.root,this.root.graph,candidate,report);}catch(e){if(e.code!=='chain-factory-coverage-invalid')throw e;coverage={factory_validated:false};}
    return {status:this.expired()?'late-result':coverage.factory_validated?'valid':'validation-rejected',coverage,validation_sha256:digest({...report,elapsed_ms:0})};
  }
  retain(w,row){
    const s=score(w,this.catalogs);
    if(compare(s.tuple,this.bestScore.tuple)<0){this.best=w;this.bestScore=s;this.work.improvements++;row.new_incumbent=true;
      this.incumbents.push({proposal:this.work.proposals,score:s.tuple,witness_sha256:digest(w)});}
    return s;
  }
  async step(action){
    if(this.done)return {...this.observation(),transition:{performed:false,status:'episode-ended'}};
    const params=this.params(action),before=this.current,began=performance.now(),row={action,proposal:++this.work.proposals,parent_sha256:digest(before),before:this.currentScore.tuple,
      accepted:false,status:'pending',route_expansions:0,phases:[]};let candidate=null;
    try{
      if(action.kind==='compact')candidate=before;
      else{
        const chains=extractChains(before).chains,d=c=>Math.abs(c.cells[0].x-c.cells.at(-1).x)+Math.abs(c.cells[0].y-c.cells.at(-1).y);
        const routePriority=[...chains].sort((a,b)=>Number(a.transport!=='pipe')-Number(b.transport!=='pipe')||(action.order==='far'?-1:1)*(d(a)-d(b))||a.id-b.id).map(c=>c.id);
        const start=performance.now();let result;
        try{result=reembed(detail(before,this.catalogs),this.catalogs,{...params,endpointPolicy:'terminal-flexible',junctionPolicy:'internal',routePriority,
          workBudget:this.plan.per_action_expansions,totalWorkBudget:this.plan.per_action_expansions,onRouteResult:r=>{row.route_expansions+=r.expanded;this.work.route_expansions+=r.expanded;}});}
        finally{this.work.route_ms+=performance.now()-start;}
        row.status=result.status;row.diagnostics=result.diagnostics;
        if(result.status==='candidate')candidate=transform(result.detail.witness);else if(result.status==='unchanged')candidate=before;
      }
      if(candidate){
        let check=await this.checked(candidate);row.phases.push({stage:'intermediate',...check});
        if(check.status==='valid'){
          this.retain(candidate,row);
          if(action.compact_after||action.kind==='compact'){
            const start=performance.now();this.work.compactions++;
            try{candidate=transform(compactInterface(detail(candidate,this.catalogs),this.catalogs).witness);}finally{this.work.compaction_ms+=performance.now()-start;}
            check=await this.checked(candidate);row.phases.push({stage:'compaction',...check});
          }
          if(check.status==='valid'){
            const s=this.retain(candidate,row);if(compare(s.tuple,this.currentScore.tuple)>0)this.work.worse_transitions++;
            this.current=candidate;this.currentScore=s;this.work.accepted++;row.accepted=true;row.status='accepted';row.score=s.tuple;row.witness_sha256=digest(candidate);
          }else row.status=check.status;
        }else row.status=check.status;
      }
    }catch(e){if(!normalRejections.has(e.code))throw e;row.status=e.code;row.message=e.message;}
    if(row.route_expansions>this.plan.per_action_expansions||digest(before)!==row.parent_sha256)throw new Error('Direct work/immutability invariant failed');
    row.elapsed_ms=performance.now()-began;row.best=this.bestScore.tuple;this.trace.push(row);return {...this.observation(),transition:row};
  }
  summary({trace=false,witnesses=false}={}){return {root:this.root.id,factory_validated:true,initial_score:this.initial,best_score:this.bestScore,current_score:this.currentScore,
    work:this.work,incumbents:this.incumbents,elapsed_ms:performance.now()-this.started,termination:this.expired()?'elapsed-budget':this.done?'proposal-budget':'caller-ended',
    density:density(this.best,this.catalogs),waste:mechanicalWaste({trace:this.trace,work:this.work}),best_witness_sha256:digest(this.best),
    ...(trace?{trace:this.trace}:{}),...(witnesses?{best_witness:this.best,current_witness:this.current}:{})};}
}
