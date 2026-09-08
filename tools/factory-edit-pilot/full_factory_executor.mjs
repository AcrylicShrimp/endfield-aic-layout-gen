// Unranked local vocabulary and selected physical edits; acceptance belongs to Rust.
import fs from 'node:fs';
import readline from 'node:readline';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {extractChains,reembed,extentOf} from '../../docs/benchmarks/constructive-local-reembedding/model.mjs';
import {releaseBridgeDraft} from '../../docs/benchmarks/constructive-local-reembedding/components.mjs';
import {project} from '../../docs/benchmarks/constructive-factory-closure/model.mjs';

const key=p=>`${p.x},${p.y}`;
const equal=(a,b)=>BigInt(a.numerator)*BigInt(b.denominator)===BigInt(b.numerator)*BigInt(a.denominator);
const steps=[[1,0],[-1,0],[0,1],[0,-1],[2,0],[-2,0],[0,2],[0,-2]];
const fail=message=>{throw Object.assign(new Error(message),{code:'factory.edit-parameters'});};

export function strips(w,axis){
  const cross=axis==='x'?'y':'x',extent=axis==='x'?'width':'height',critical=new Set();
  for(const p of w.placements)for(let i=0;i<p[extent];i++)critical.add(p[axis]+i);
  for(const c of w.logistics_components)critical.add(c.position[axis]);
  for(const n of w.transport_networks){
    const ins=new Map(),outs=new Map();
    for(const s of n.segments)for(const [m,p]of [[ins,s.to],[outs,s.from]]){const k=key(p),a=m.get(k)??[];a.push(s);m.set(k,a);}
    for(const t of n.terminals)critical.add(t.position[axis]);
    for(const p of n.cells){
      const a=ins.get(key(p))??[],b=outs.get(key(p))??[];
      if(a.length!==1||b.length!==1||a[0].from[cross]!==p[cross]||b[0].to[cross]!==p[cross]
        ||Math.sign(p[axis]-a[0].from[axis])!==Math.sign(b[0].to[axis]-p[axis])||!equal(a[0].rate,b[0].rate))critical.add(p[axis]);
    }
  }
  // A used port arm is adjacent geometry too. Inserting between the body and its
  // connection would create an unsupported two-cell port arm, not a new belt cell.
  const portCuts=new Set(),placements=new Map(w.placements.map(p=>[p.instance,p]));
  for(const n of w.transport_networks)for(const t of n.terminals)if(t.endpoint.kind==='facility'){
    const p=placements.get(t.endpoint.instance),terminal=t.position[axis];
    const body=Math.max(p[axis],Math.min(p[axis]+p[extent]-1,terminal));
    for(let cut=Math.min(body,terminal)+1;cut<=Math.max(body,terminal);cut++)portCuts.add(cut);
  }
  const bounds=extentOf(w),low=axis==='x'?bounds.minX:bounds.minY,high=axis==='x'?bounds.maxX:bounds.maxY;
  return {remove:Array.from({length:high-low+1},(_,i)=>low+i).filter(i=>!critical.has(i)),
    insert:Array.from({length:high-low},(_,i)=>low+i+1).filter(i=>!portCuts.has(i)&&!w.placements.some(p=>p[axis]<i&&i<p[axis]+p[extent]))};
}

export function stripEdit(w,axis,position,insert){
  if(!['x','y'].includes(axis)||!Number.isSafeInteger(position)||!strips(w,axis)[insert?'insert':'remove'].includes(position))fail('Strip bisects a protected event or lies outside the explicit vocabulary');
  const move=p=>({...p,[axis]:p[axis]+(insert?(p[axis]>=position?1:0):(p[axis]>position?-1:0))});
  const result={...w,placements:w.placements.map(move),logistics_components:w.logistics_components.map(c=>({...c,position:move(c.position)})),
    transport_networks:w.transport_networks.map(n=>{
      const cells=n.cells.filter(p=>insert||p[axis]!==position).map(move),segments=[];
      const out=new Map(n.segments.map(s=>[key(s.from),s]));
      for(const s of n.segments){
        if(!insert&&s.from[axis]===position)continue;
        const from=move(s.from);let end=s.to;
        if(!insert&&end[axis]===position){const next=out.get(key(end));if(!next||!equal(next.rate,s.rate))fail('Missing straight continuation');end=next.to;}
        const to=move(end);
        if(insert&&Math.abs(to.x-from.x)+Math.abs(to.y-from.y)===2){
          const middle={x:(from.x+to.x)/2,y:(from.y+to.y)/2};cells.push(middle);
          segments.push({from,to:middle,rate:s.rate},{from:middle,to,rate:s.rate});
        }else segments.push({from,to,rate:s.rate});
      }
      const unique=new Map(cells.map(p=>[key(p),p]));
      if(unique.size!==cells.length)fail('Strip insertion produced duplicate same-network cells');
      return {...n,cells,segments,terminals:n.terminals.map(t=>({...t,position:move(t.position)}))};
    })};
  return result;
}

export function describe(w,catalogs){
  const began=performance.now(),ports=project(w,catalogs),chains=extractChains(w).chains;
  const actions=[];
  for(const p of w.placements){
    for(const [dx,dy]of steps)actions.push({kind:'move',instance:p.instance,dx,dy,x:p.x,y:p.y});
    for(const rotation of catalogs.facilities.facilities.find(f=>f.id===p.facility).allowed_rotations)
      if(rotation!==p.rotation)actions.push({kind:'rotate',instance:p.instance,rotation,x:p.x,y:p.y});
  }
  const used=new Set(w.transport_networks.flatMap(n=>n.terminals.filter(t=>t.endpoint.kind==='facility').map(t=>JSON.stringify([t.endpoint.instance,t.endpoint.port]))));
  for(const n of w.transport_networks)for(const t of n.terminals)if(t.endpoint.kind==='facility'){
    for(const p of ports[t.endpoint.instance])if(p.direction===t.direction&&p.transport===n.transport&&!used.has(JSON.stringify([t.endpoint.instance,p.id])))
      actions.push({kind:'port',terminal:t.id,instance:t.endpoint.instance,port:p.id,x:p.connection.x,y:p.connection.y,transport:n.transport});
  }
  for(const c of chains)actions.push({kind:'reroute',chain:c.id,network:c.network,x:c.cells[0].x,y:c.cells[0].y,transport:c.transport});
  for(const c of w.logistics_components){
    for(const [dx,dy]of steps)actions.push({kind:'component',component:c.id,x:c.position.x+dx,y:c.position.y+dy,dx,dy,rotation:c.rotation,transport:c.transport});
    if(c.kind==='bridge')actions.push({kind:'release_bridge',component:c.id,x:c.position.x,y:c.position.y,transport:c.transport});
  }
  for(const axis of ['x','y']){
    const options=strips(w,axis);
    for(const position of options.insert)actions.push({kind:'insert',axis,position,x:axis==='x'?position:0,y:axis==='y'?position:0});
    for(const position of options.remove)actions.push({kind:'contract',axis,position,x:axis==='x'?position:0,y:axis==='y'?position:0});
  }
  actions.push({kind:'restore_best',x:0,y:0});
  return {schema_version:1,ports,chains,actions,seconds:(performance.now()-began)/1000};
}

export function execute(w,catalogs,action,work=5000){
  const began=performance.now();let expanded=0;
  if(!Number.isSafeInteger(work)||work<1||work>200000)fail('Invalid route work budget');
  try{
    if(['insert','contract'].includes(action.kind))return {status:'candidate',witness:stripEdit(w,action.axis,action.position,action.kind==='insert'),route_expansions:0,seconds:(performance.now()-began)/1000};
    const options={endpointPolicy:'terminal-flexible',junctionPolicy:'internal',maxNewCrossings:2,
      workBudget:work,totalWorkBudget:work,onRouteResult:r=>{expanded+=r.expanded;}};
    switch(action.kind){
      case 'move':options.moves=[{instances:[action.instance],dx:action.dx,dy:action.dy}];break;
      case 'rotate':options.moves=[{instances:[action.instance],dx:0,dy:0}];options.facilityRotations=[{instance:action.instance,rotation:action.rotation}];break;
      case 'port':options.portAssignments=[{terminal:action.terminal,port:action.port}];break;
      case 'reroute':options.chainIds=[action.chain];break;
      case 'component':options.componentPoses=[{id:action.component,x:action.x,y:action.y,rotation:action.rotation}];break;
      case 'release_bridge':options.releaseBridges=[action.component];break;
      default:fail('Unknown executor action');
    }
    const draft=releaseBridgeDraft(w,catalogs,options.releaseBridges??[]);
    const chains=extractChains(draft.witness,draft.released).chains;
    options.routePriority=[...chains].sort((a,b)=>Number(a.transport!=='pipe')-Number(b.transport!=='pipe')||a.id-b.id).map(c=>c.id);
    const result=reembed({witness:w,labels:{}},catalogs,options);
    return {status:result.status,witness:result.status==='candidate'?result.detail.witness:result.status==='unchanged'?w:null,
      diagnostics:result.diagnostics??[],route_expansions:expanded,seconds:(performance.now()-began)/1000};
  }catch(e){return {status:'executor_rejected',diagnostics:[{code:e.code??'factory.executor-error',message:e.message}],route_expansions:expanded,seconds:(performance.now()-began)/1000};}
}

async function main(){
  const i=process.argv.indexOf('--catalogs');if(i<0||!process.argv[i+1])fail('Expected --catalogs PATH');
  const paths=JSON.parse(fs.readFileSync(process.argv[i+1],'utf8'));
  const catalogs=Object.fromEntries(Object.entries(paths).map(([k,p])=>[k,JSON.parse(fs.readFileSync(p,'utf8'))]));
  for await(const line of readline.createInterface({input:process.stdin,crlfDelay:Infinity})){
    try{const r=JSON.parse(line);if(r.schema_version!==1)fail('Unsupported executor request');
      const result=r.op==='describe'?describe(r.witness,catalogs):r.op==='execute'?execute(r.witness,catalogs,r.action,r.work):fail('Unknown executor operation');
      process.stdout.write(JSON.stringify({id:r.id,...result})+'\n');
    }catch(e){process.stdout.write(JSON.stringify({status:'error',message:e.message})+'\n');}
  }
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url))main().catch(e=>{console.error(e.message);process.exitCode=1;});
