// Finite, unranked facility-pose domains. No routing or complete candidate validation here.
import {extractChains,extentOf} from '../../docs/benchmarks/constructive-local-reembedding/model.mjs';
import {movementPlan,movementIndex,bodyCells} from '../../docs/benchmarks/constructive-local-reembedding/movement.mjs';
import {detachablePairs,joinableChains} from '../../docs/benchmarks/constructive-local-reembedding/terminal-identity.mjs';
import {rigidBlockPose} from '../../docs/benchmarks/constructive-local-reembedding/block-rotation.mjs';
import {planFacilityRotations} from '../../docs/benchmarks/constructive-local-reembedding/facility-rotation.mjs';
import {emptyRectangles} from '../../docs/benchmarks/constructive-local-reembedding/groups.mjs';
const key=p=>`${p.x},${p.y}`;
export const LOCAL_STEPS=[[1,0],[-1,0],[0,1],[0,-1],[2,0],[-2,0],[0,2],[0,-2]];
const tag=(transport,p)=>`${transport}:${key(p)}`;
const fail=(code,message)=>{throw Object.assign(new Error(message),{code,severity:'error',stage:'factory-pose-vocabulary'});};
const diagnostic=(code,message,entity)=>({code,message,entity,severity:'info',stage:'factory-pose-vocabulary'});

export function poseAction(w,poses){
  const bodies=poses.map(q=>{const p=w.placements[q.member],swap=Math.abs(q.rotation-p.rotation)%180!==0;
    return {...q,width:swap?p.height:p.width,height:swap?p.width:p.height};});
  const x=Math.min(...bodies.map(p=>p.x)),y=Math.min(...bodies.map(p=>p.y));
  return {kind:'pose',poses:[...poses].sort((a,b)=>a.member-b.member),x,y,
    width:Math.max(...bodies.map(p=>p.x+p.width))-x,height:Math.max(...bodies.map(p=>p.y+p.height))-y,
    body_area:bodies.reduce((n,p)=>n+p.width*p.height,0),
    source_x:Math.min(...poses.map(q=>w.placements[q.member].x)),source_y:Math.min(...poses.map(q=>w.placements[q.member].y))};
}

export function poseOptions(w,catalogs,poses){
  if(!Array.isArray(poses)||!poses.length||poses.length>16||poses.some(q=>!q||!Number.isSafeInteger(q.member)||q.member<0||q.member>=w.placements.length||
    !Number.isSafeInteger(q.x)||!Number.isSafeInteger(q.y)||![0,90,180,270].includes(q.rotation))||new Set(poses.map(q=>q.member)).size!==poses.length)
    fail('factory.pose-parameters','Pose requires 1..16 distinct current placement indices, integer destinations and catalog rotations');
  const vectors=new Map();let changed=false;
  for(const q of poses){const p=w.placements[q.member],dx=q.x-p.x,dy=q.y-p.y,k=`${dx},${dy}`;
    if(!Number.isSafeInteger(dx)||!Number.isSafeInteger(dy))fail('factory.pose-parameters','Pose displacement exceeds safe integer range');
    changed ||= !!dx||!!dy||q.rotation!==p.rotation;
    if(!vectors.has(k))vectors.set(k,{instances:[],dx,dy});vectors.get(k).instances.push(p.instance);
  }
  if(!changed||vectors.size>4)fail('factory.pose-parameters','Pose must change geometry within four distinct translation vectors');
  const moves=[...vectors.values()],facilityRotations=poses.map(q=>({instance:w.placements[q.member].instance,rotation:q.rotation}));
  // This validates every requested catalog angle even if it matches the source angle.
  const facilityChanges=planFacilityRotations(w,catalogs,moves,facilityRotations);
  return {moves,facilityRotations,facilityChanges};
}

export function productionEdges(w){
  const indices=new Map(w.placements.map((p,i)=>[p.instance,i])),edges=new Map();
  for(const n of w.transport_networks){
    const out=new Map(),sinks=new Map(),bridges=new Set(w.logistics_components
      .filter(c=>c.kind==='bridge'&&c.transport===n.transport).map(c=>key(c.position)));
    // Validated bridges carry independent straight channels, even for one item.
    const vertex=(p,s)=>bridges.has(key(p))?`${key(p)}/${s.from.y===s.to.y?'h':'v'}`:key(p);
    for(const s of n.segments){const k=vertex(s.from,s),a=out.get(k)??[];a.push(vertex(s.to,s));out.set(k,a);}
    for(const t of n.terminals)if(t.direction==='input'&&t.endpoint.kind==='facility'){
      const k=key(t.position),a=sinks.get(k)??[];a.push(indices.get(t.endpoint.instance));sinks.set(k,a);
    }
    for(const t of n.terminals)if(t.direction==='output'&&t.endpoint.kind==='facility'){
      const source=indices.get(t.endpoint.instance),seen=new Set([key(t.position)]),queue=[key(t.position)];
      for(let i=0;i<queue.length;i++){
        for(const sink of sinks.get(queue[i])??[])if(source!==sink)edges.set(`${source}:${sink}`,[source,sink]);
        for(const next of out.get(queue[i])??[])if(!seen.has(next)){seen.add(next);queue.push(next);}
      }
    }
  }
  return [...edges.values()];
}

function components(count,edges){
  const adjacent=Array.from({length:count},()=>[]);for(const [a,b]of edges)adjacent[a].push(b);
  const index=Array(count).fill(-1),low=Array(count),stack=[],active=new Set(),result=[];let serial=0;
  function visit(v){
    index[v]=low[v]=serial++;stack.push(v);active.add(v);
    for(const next of adjacent[v]){if(index[next]===-1){visit(next);low[v]=Math.min(low[v],low[next]);}else if(active.has(next))low[v]=Math.min(low[v],index[next]);}
    if(low[v]===index[v]){const members=[];let next;do{next=stack.pop();active.delete(next);members.push(next);}while(next!==v);result.push(members);}
  }
  for(let i=0;i<count;i++)if(index[i]===-1)visit(i);
  return result;
}

export function poseGroups(w,includeConnected){
  const indices=new Map(w.placements.map((p,i)=>[p.instance,i])),parent=w.placements.map((_,i)=>i),frozen=new Set();
  const find=i=>parent[i]===i?i:parent[i]=find(parent[i]);
  const join=(a,b)=>{a=find(a);b=find(b);if(a!==b)parent[b]=a;};
  const slots=new Map(),pairs=detachablePairs(w);
  for(const n of w.transport_networks)for(const t of n.terminals){const k=tag(n.transport,t.position),a=slots.get(k)??[];a.push(t);slots.set(k,a);}
  for(const [k,ts]of slots)if(ts.length>1&&!pairs.has(k)){
    const members=ts.filter(t=>t.endpoint.kind==='facility').map(t=>indices.get(t.endpoint.instance));
    for(const i of members.slice(1))join(members[0],i);
    if(ts.some(t=>t.endpoint.kind==='external'))for(const i of members)frozen.add(i);
  }
  const closure=members=>{const roots=new Set(members.map(find));return parent.map((_,i)=>i).filter(i=>roots.has(find(i)));};
  const groups=[],seen=new Set(),omitted=[];
  const add=(members,source)=>{
    members=[...new Set(members)].sort((a,b)=>a-b);const k=members.join(',');if(seen.has(k))return;seen.add(k);
    if(members.length>16||members.some(i=>frozen.has(i))){omitted.push({members,source,code:members.length>16?'factory.group-member-budget':'factory.group-external-anchor'});return;}
    groups.push({members,source});
  };
  for(let i=0;i<w.placements.length;i++){
    const required=closure([i]);
    if(required.length===1)add(required,'singleton');
    else if(includeConnected)add(required,'terminal_closure');
    else omitted.push({members:[i],source:'singleton',code:'factory.group-required-closure'});
  }
  if(includeConnected){
    const edges=productionEdges(w);
    for(const scc of components(w.placements.length,edges))if(scc.length>1)add(closure(scc),'cycle');
    for(const pair of edges)add(closure(pair),'edge');
  }
  return {groups,omitted};
}

function prefix(bounds,points){
  const width=bounds.maxX-bounds.minX+1,height=bounds.maxY-bounds.minY+1,stride=width+1,data=new Int32Array(stride*(height+1));
  for(const p of points)if(p.x>=bounds.minX&&p.x<=bounds.maxX&&p.y>=bounds.minY&&p.y<=bounds.maxY)data[(p.y-bounds.minY+1)*stride+p.x-bounds.minX+1]=1;
  for(let y=1;y<=height;y++)for(let x=1;x<=width;x++)data[y*stride+x]+=data[y*stride+x-1]+data[(y-1)*stride+x]-data[(y-1)*stride+x-1];
  return (x,y,w,h)=>{x-=bounds.minX;y-=bounds.minY;if(x<0||y<0||x+w>width||y+h>height)return 1;
    return data[(y+h)*stride+x+w]-data[y*stride+x+w]-data[(y+h)*stride+x]+data[y*stride+x];};
}

export function preflightGroup(w,group,chains,anchors,bounds,joinable=joinableChains(w,chains),index=movementIndex(w,chains)){
  const ids=group.members.map(i=>w.placements[i].instance),selected=new Set(ids);
  const plan=movementPlan(w,chains,anchors,[{instances:ids,dx:0,dy:0}],{junctionPolicy:'internal',joinable,index});
  const bodies=w.placements.filter(p=>selected.has(p.instance)),stationary=w.placements.filter(p=>!selected.has(p.instance)).flatMap(bodyCells);
  const fixed={belt:new Set(),pipe:new Set()},moving={belt:new Map(),pipe:new Map()};
  const add=(tr,p)=>moving[tr].set(key(p),p);
  for(const tr of ['belt','pipe'])for(const k of anchors[tr])if(!plan.moved.has(`${tr}:${k}`))fixed[tr].add(k);
  for(const n of w.transport_networks)for(const t of n.terminals)if(t.endpoint.kind==='facility'&&selected.has(t.endpoint.instance))add(n.transport,t.position);
  for(const c of w.logistics_components)if(plan.components.includes(c.id))add(c.transport,c.position);
  for(const c of chains)if(plan.fixedChains.has(c.id))for(const p of plan.fixedChains.get(c.id))add(c.transport,p);
  const joinVectors=new Map(),joinOrigins=new Map();
  for(const c of joinable){
    const a=c.startTerminals[0],b=c.endTerminals[0],ownA=selected.has(a.endpoint.instance),ownB=selected.has(b.endpoint.instance);
    if(ownA===ownB)continue;
    const [own,other]=ownA?[a,b]:[b,a],dx=other.position.x-own.position.x,dy=other.position.y-own.position.y;
    joinVectors.set(`${dx},${dy}`,[dx,dy]);
    joinOrigins.set(`${c.transport}:${key(own.position)}:${dx},${dy}`,key(other.position));
  }
  const points=[...bodies.flatMap(p=>[p,{x:p.x+p.width-1,y:p.y+p.height-1}]),...Object.values(moving).flatMap(m=>[...m.values()])];
  const box=points.reduce((b,p)=>({minX:Math.min(b.minX,p.x),maxX:Math.max(b.maxX,p.x),minY:Math.min(b.minY,p.y),maxY:Math.max(b.maxY,p.y)}),{minX:Infinity,minY:Infinity,maxX:-Infinity,maxY:-Infinity});
  const anchorPoints=[...new Set([...fixed.belt,...fixed.pipe])].map(k=>{const [x,y]=k.split(',').map(Number);return{x,y};});
  const occupied=prefix(bounds,[...stationary,...anchorPoints]),body=prefix(bounds,stationary);
  const check=(dx,dy)=>{
    if(box.minX+dx<bounds.minX||box.maxX+dx>bounds.maxX||box.minY+dy<bounds.minY||box.maxY+dy>bounds.maxY)return 'bounds';
    if(bodies.some(p=>occupied(p.x+dx,p.y+dy,p.width,p.height)))return 'body_or_anchor';
    for(const tr of ['belt','pipe'])for(const p of moving[tr].values())if(body(p.x+dx,p.y+dy,1,1)||(fixed[tr].has(`${p.x+dx},${p.y+dy}`)&&joinOrigins.get(`${tr}:${key(p)}:${dx},${dy}`)!==`${p.x+dx},${p.y+dy}`))return 'carried_or_terminal';
    return null;
  };
  return {box,check,join_vectors:[...joinVectors.values()],moving_points:Object.values(moving).reduce((n,m)=>n+m.size,0)};
}

export function translationActions(w,{mode='groups',scans_per_group=256,candidates_per_group=32}={}){
  if(!['local','filtered-local','distant','groups'].includes(mode)||!Number.isSafeInteger(scans_per_group)||scans_per_group<8||scans_per_group>4096||
     !Number.isSafeInteger(candidates_per_group)||candidates_per_group<8||candidates_per_group>256)fail('factory.vocabulary-options','Invalid relocation mode or scan/candidate bounds');
  const began=performance.now(),bounds=extentOf(w),area=(bounds.maxX-bounds.minX+1)*(bounds.maxY-bounds.minY+1);
  if(!Number.isSafeInteger(area)||area>200000)fail('factory.vocabulary-area-budget','Current observation/enumeration area exceeds the explicit 200000-cell guard');
  const {chains,anchors}=extractChains(w),index=movementIndex(w,chains),joinable=index.joinable,generated=mode==='local'?{groups:w.placements.map((_,i)=>({members:[i],source:'singleton'})),omitted:[]}:poseGroups(w,mode==='groups');
  const groups=generated.groups.slice(0,512),actions=[],rejected={},diagnostics=[...generated.omitted];let examined=0,capped=0,joinExamined=0;
  const reject=code=>{rejected[code]=(rejected[code]??0)+1;};
  for(const group of groups){
    let preflight;
    if(mode!=='local')try{preflight=preflightGroup(w,group,chains,anchors,bounds,joinable,index);}catch(e){
      if(!e.code)throw e;diagnostics.push({...group,code:e.code});continue;
    }
    let retained=0;const seen=new Set();
    const examine=(dx,dy)=>{
      const k=`${dx},${dy}`;if(seen.has(k)||(!dx&&!dy))return;seen.add(k);examined++;
      const code=preflight?.check(dx,dy);if(code){reject(code);return;}
      actions.push(poseAction(w,group.members.map(member=>{const p=w.placements[member];return {member,x:p.x+dx,y:p.y+dy,rotation:p.rotation};})));retained++;
    };
    for(const [dx,dy]of LOCAL_STEPS)examine(dx,dy);
    if(!['distant','groups'].includes(mode))continue;
    for(const [dx,dy]of preflight.join_vectors){if(retained>=candidates_per_group)break;joinExamined++;examine(dx,dy);}
    const b=preflight.box,loX=bounds.minX-b.minX,loY=bounds.minY-b.minY,nx=bounds.maxX-b.maxX-loX+1,ny=bounds.maxY-b.maxY-loY+1,total=nx*ny;
    if(total<=0)continue;
    const gcd=(a,b)=>b?gcd(b,a%b):a;let stride=Math.max(1,Math.floor(total*.61803398875));while(gcd(stride,total)!==1)stride++;
    let i=0;
    for(;i<total&&i<scans_per_group&&retained<candidates_per_group;i++){
      const position=(i*stride)%total;examine(loX+position%nx,loY+Math.floor(position/nx));
    }
    if(i<total)capped++;
  }
  return {actions,report:{mode,groups:groups.length,groups_generated:generated.groups.length,groups_budget_omitted:Math.max(0,generated.groups.length-groups.length),
    omitted:diagnostics.map(d=>({...d,...diagnostic(d.code,'Group omitted from the bounded retained-terminal relocation vocabulary',{members:d.members,source:d.source})})),examined,static_rejected:rejected,capped_groups:capped,join_vectors_examined:joinExamined,actions:actions.length,scans_per_group,candidates_per_group,seconds:(performance.now()-began)/1000}};
}

export function compoundGroups(w){
  const groups=new Map(),add=(members,source)=>{
    members=[...new Set(members)].sort((a,b)=>a-b);
    if(members.length>=2&&members.length<=4&&!groups.has(members.join(',')))groups.set(members.join(','),{members,source});
  };
  for(const g of poseGroups(w,true).groups)add(g.members,g.source);
  const adjacent=w.placements.map(()=>new Set());
  for(const [a,b]of productionEdges(w)){adjacent[a].add(b);adjacent[b].add(a);}
  for(const axis of ['x','y']){
    const cross=axis==='x'?'y':'x',size=axis==='x'?'width':'height',crossSize=axis==='x'?'height':'width',lines=new Map();
    for(const [i,p]of w.placements.entries()){const k=`${p[cross]},${p[crossSize]}`,line=lines.get(k)??[];line.push(i);lines.set(k,line);}
    for(const line of lines.values()){
      line.sort((a,b)=>w.placements[a][axis]-w.placements[b][axis]||a-b);
      for(let i=0;i<line.length;i++)for(let j=i+1;j<Math.min(i+4,line.length);j++){
        const a=w.placements[line[j-1]],b=w.placements[line[j]];if(a[axis]+a[size]!==b[axis])break;
        const members=line.slice(i,j+1),selected=new Set(members),seen=new Set([members[0]]),queue=[members[0]];
        for(let k=0;k<queue.length;k++)for(const next of adjacent[queue[k]])if(selected.has(next)&&!seen.has(next)){seen.add(next);queue.push(next);}
        if(seen.size===members.length)add(members,`aligned-${axis}`);
      }
    }
  }
  return [...groups.values()];
}

function* origins(box,bounds,voids,limit){
  const seen=new Set();let count=0;
  function* offer(x,y){const k=`${x},${y}`;if(seen.has(k)||count>=limit)return;seen.add(k);count++;yield{x,y};}
  yield* offer(box.x,box.y);
  for(const [dx,dy]of LOCAL_STEPS)yield* offer(box.x+dx,box.y+dy);
  for(const r of voids)if(r.width>=box.width&&r.height>=box.height)
    for(const y of [r.y,r.y+1,r.y+r.height-box.height])for(const x of [r.x,r.x+1,r.x+r.width-box.width])yield* offer(x,y);
  const nx=bounds.maxX-bounds.minX-box.width+2,ny=bounds.maxY-bounds.minY-box.height+2,total=nx*ny;
  if(nx<=0||ny<=0)return;
  const gcd=(a,b)=>b?gcd(b,a%b):a;let stride=Math.max(1,Math.floor(total*.61803398875));while(gcd(stride,total)!==1)stride++;
  for(let i=0;i<total&&count<limit;i++){const p=(i*stride)%total;yield* offer(bounds.minX+p%nx,bounds.minY+Math.floor(p/nx));}
}

export function poseActions(w,catalogs,options={}){
  const {pose_domain='compound',compound_scans=256,compound_candidates=8,compound_preflights=512}=options;
  if(!['basic','compound'].includes(pose_domain)||!Number.isSafeInteger(compound_scans)||compound_scans<8||compound_scans>4096||
    !Number.isSafeInteger(compound_candidates)||compound_candidates<1||compound_candidates>32||
    !Number.isSafeInteger(compound_preflights)||compound_preflights<1||compound_preflights>4096)
    fail('factory.vocabulary-options','Invalid pose domain or compound scan/candidate/preflight bounds');
  const began=performance.now(),translation=translationActions(w,options),actions=[...translation.actions],bounds=extentOf(w),
    {chains,anchors}=extractChains(w),index=movementIndex(w,chains),joinable=index.joinable,rejected={},streams=[];
  const report={pose_domain,single_rotations_examined:0,single_rotations:0,compound_groups:0,compound_groups_omitted:0,
    compound_examined:0,compound_preflights:0,compound_actions:0,compound_streams:0,compound_streams_capped:0,
    compound_preflight_limit:compound_preflights,compound_scans,compound_candidates,static_rejected:rejected};
  const reject=code=>{rejected[code]=(rejected[code]??0)+1;};
  const inside=p=>p.x>=bounds.minX&&p.x<=bounds.maxX&&p.y>=bounds.minY&&p.y<=bounds.maxY;
  const preflight=poses=>{
    try{
      const o=poseOptions(w,catalogs,poses),plan=movementPlan(w,chains,anchors,o.moves,{facilityChanges:o.facilityChanges,joinable,index});
      if(w.transport_networks.some(n=>n.terminals.some(t=>!inside(plan.shiftTerminal(t))))||
        w.logistics_components.some(c=>!inside(plan.shiftAnchor(c.transport,c.position)))||
        [...plan.fixedChains.values()].some(path=>path.some(p=>!inside(p)))){reject('factory.pose-carried-bounds');return false;}
      return true;
    }catch(e){if(!e.code)throw e;reject(e.code);return false;}
  };
  for(const [member,p]of w.placements.entries()){
    const stationary=prefix(bounds,w.placements.filter((_,i)=>i!==member).flatMap(bodyCells));
    for(const rotation of catalogs.facilities.facilities.find(f=>f.id===p.facility).allowed_rotations)if(rotation!==p.rotation){
      report.single_rotations_examined++;
      const poses=[{member,x:p.x,y:p.y,rotation}],a=poseAction(w,poses);
      if(stationary(a.x,a.y,a.width,a.height)){reject('factory.pose-body-or-bounds');continue;}
      if(preflight(poses)){actions.push(a);report.single_rotations++;}
    }
  }
  if(pose_domain==='compound'){
    const all=compoundGroups(w),groups=all.slice(0,128),voids=emptyRectangles(w);
    report.compound_groups=groups.length;report.compound_groups_omitted=all.length-groups.length;
    for(const group of groups){
      const selected=new Set(group.members),stationary=prefix(bounds,w.placements.filter((_,i)=>!selected.has(i)).flatMap(bodyCells)),
        ps=group.members.map(i=>w.placements[i]),origin={x:Math.min(...ps.map(p=>p.x)),y:Math.min(...ps.map(p=>p.y))};
      for(const turns of [1,2,3]){
        try{
          const r=rigidBlockPose(w,catalogs,ps.map(p=>p.instance),turns,origin);
          streams.push({group,stationary,template:r.placements,origin,origins:origins(r.envelope,bounds,voids,compound_scans),examined:0,retained:0,done:false});
        }catch(e){if(!e.code)throw e;reject(e.code);}
      }
    }
    report.compound_streams=streams.length;
    // Each stream consumes at most one expensive movement preflight per round.
    // Body-only rejections are cheap and cannot starve another group's preflight.
    let active=true;
    while(active&&report.compound_preflights<compound_preflights){
      active=false;
      for(const s of streams){
        if(s.done||s.retained>=compound_candidates)continue;
        active=true;
        while(true){
          const next=s.origins.next();if(next.done){s.done=true;break;}
          s.examined++;report.compound_examined++;
          const dx=next.value.x-s.origin.x,dy=next.value.y-s.origin.y;
          if(s.template.some(p=>s.stationary(p.x+dx,p.y+dy,p.width,p.height))){reject('factory.pose-body-or-bounds');continue;}
          const poses=s.template.map((p,i)=>({member:s.group.members[i],x:p.x+dx,y:p.y+dy,rotation:p.rotation}));
          report.compound_preflights++;
          if(preflight(poses)){actions.push(poseAction(w,poses));report.compound_actions++;s.retained++;}
          break;
        }
        if(report.compound_preflights>=compound_preflights)break;
      }
    }
    report.compound_streams_capped=streams.filter(s=>!s.done||s.examined>=compound_scans).length;
  }
  const seen=new Set(),unique=actions.filter(a=>{const k=JSON.stringify(a.poses);if(seen.has(k))return false;seen.add(k);return true;});
  return {actions:unique,report:{...report,translation:translation.report,duplicates:actions.length-unique.length,actions:unique.length,seconds:(performance.now()-began)/1000}};
}
