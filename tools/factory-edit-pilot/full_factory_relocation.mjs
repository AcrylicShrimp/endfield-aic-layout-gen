// Finite, unranked relocation domains. No routing or complete candidate validation here.
import {extractChains,extentOf} from '../../docs/benchmarks/constructive-local-reembedding/model.mjs';
import {movementPlan,bodyCells} from '../../docs/benchmarks/constructive-local-reembedding/movement.mjs';
import {detachablePairs} from '../../docs/benchmarks/constructive-local-reembedding/terminal-identity.mjs';
const key=p=>`${p.x},${p.y}`;
export const LOCAL_STEPS=[[1,0],[-1,0],[0,1],[0,-1],[2,0],[-2,0],[0,2],[0,-2]];
const tag=(transport,p)=>`${transport}:${key(p)}`;
const fail=(code,message)=>{throw Object.assign(new Error(message),{code,severity:'error',stage:'factory-relocation-vocabulary'});};
const diagnostic=(code,message,entity)=>({code,message,entity,severity:'info',stage:'factory-relocation-vocabulary'});

export function productionEdges(w){
  const indices=new Map(w.placements.map((p,i)=>[p.instance,i])),edges=new Map();
  for(const n of w.transport_networks){
    const out=new Map(),sinks=new Map();
    for(const s of n.segments){const k=key(s.from),a=out.get(k)??[];a.push(key(s.to));out.set(k,a);}
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

export function relocationGroups(w,includeConnected){
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

export function preflightGroup(w,group,chains,anchors,bounds){
  const ids=group.members.map(i=>w.placements[i].instance),selected=new Set(ids);
  const plan=movementPlan(w,chains,anchors,[{instances:ids,dx:0,dy:0}],{junctionPolicy:'internal'});
  const bodies=w.placements.filter(p=>selected.has(p.instance)),stationary=w.placements.filter(p=>!selected.has(p.instance)).flatMap(bodyCells);
  const fixed={belt:new Set(),pipe:new Set()},moving={belt:new Map(),pipe:new Map()};
  const add=(tr,p)=>moving[tr].set(key(p),p);
  for(const tr of ['belt','pipe'])for(const k of anchors[tr])if(!plan.moved.has(`${tr}:${k}`))fixed[tr].add(k);
  for(const n of w.transport_networks)for(const t of n.terminals)if(t.endpoint.kind==='facility'&&selected.has(t.endpoint.instance))add(n.transport,t.position);
  for(const c of w.logistics_components)if(plan.components.includes(c.id))add(c.transport,c.position);
  for(const c of chains)if(plan.fixedChains.has(c.id))for(const p of plan.fixedChains.get(c.id))add(c.transport,p);
  const points=[...bodies.flatMap(p=>[p,{x:p.x+p.width-1,y:p.y+p.height-1}]),...Object.values(moving).flatMap(m=>[...m.values()])];
  const box=points.reduce((b,p)=>({minX:Math.min(b.minX,p.x),maxX:Math.max(b.maxX,p.x),minY:Math.min(b.minY,p.y),maxY:Math.max(b.maxY,p.y)}),{minX:Infinity,minY:Infinity,maxX:-Infinity,maxY:-Infinity});
  const anchorPoints=[...new Set([...fixed.belt,...fixed.pipe])].map(k=>{const [x,y]=k.split(',').map(Number);return{x,y};});
  const occupied=prefix(bounds,[...stationary,...anchorPoints]),body=prefix(bounds,stationary);
  const check=(dx,dy)=>{
    if(box.minX+dx<bounds.minX||box.maxX+dx>bounds.maxX||box.minY+dy<bounds.minY||box.maxY+dy>bounds.maxY)return 'bounds';
    if(bodies.some(p=>occupied(p.x+dx,p.y+dy,p.width,p.height)))return 'body_or_anchor';
    for(const tr of ['belt','pipe'])for(const p of moving[tr].values())if(body(p.x+dx,p.y+dy,1,1)||fixed[tr].has(`${p.x+dx},${p.y+dy}`))return 'carried_or_terminal';
    return null;
  };
  return {box,check,moving_points:Object.values(moving).reduce((n,m)=>n+m.size,0)};
}

export function relocationActions(w,{mode='groups',scans_per_group=256,candidates_per_group=32}={}){
  if(!['local','filtered-local','distant','groups'].includes(mode)||!Number.isSafeInteger(scans_per_group)||scans_per_group<8||scans_per_group>4096||
     !Number.isSafeInteger(candidates_per_group)||candidates_per_group<8||candidates_per_group>256)fail('factory.vocabulary-options','Invalid relocation mode or scan/candidate bounds');
  const began=performance.now(),bounds=extentOf(w),area=(bounds.maxX-bounds.minX+1)*(bounds.maxY-bounds.minY+1);
  if(!Number.isSafeInteger(area)||area>200000)fail('factory.vocabulary-area-budget','Current observation/enumeration area exceeds the explicit 200000-cell guard');
  const {chains,anchors}=extractChains(w),generated=mode==='local'?{groups:w.placements.map((_,i)=>({members:[i],source:'singleton'})),omitted:[]}:relocationGroups(w,mode==='groups');
  const groups=generated.groups.slice(0,512),actions=[],rejected={},diagnostics=[...generated.omitted];let examined=0,capped=0;
  const reject=code=>{rejected[code]=(rejected[code]??0)+1;};
  for(const group of groups){
    const bodies=group.members.map(i=>w.placements[i]),x=Math.min(...bodies.map(p=>p.x)),y=Math.min(...bodies.map(p=>p.y)),
      width=Math.max(...bodies.map(p=>p.x+p.width))-x,height=Math.max(...bodies.map(p=>p.y+p.height))-y,bodyArea=bodies.reduce((n,p)=>n+p.width*p.height,0);
    let preflight;
    if(mode!=='local')try{preflight=preflightGroup(w,group,chains,anchors,bounds);}catch(e){
      if(!e.code)throw e;diagnostics.push({...group,code:e.code});continue;
    }
    let retained=0;const seen=new Set();
    const examine=(dx,dy)=>{
      const k=`${dx},${dy}`;if(seen.has(k)||(!dx&&!dy))return;seen.add(k);examined++;
      const code=preflight?.check(dx,dy);if(code){reject(code);return;}
      actions.push({kind:'relocate',members:group.members,dx,dy,x:x+dx,y:y+dy,width,height,body_area:bodyArea,source_x:x,source_y:y});retained++;
    };
    for(const [dx,dy]of LOCAL_STEPS)examine(dx,dy);
    if(!['distant','groups'].includes(mode))continue;
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
    omitted:diagnostics.map(d=>({...d,...diagnostic(d.code,'Group omitted from the bounded retained-terminal relocation vocabulary',{members:d.members,source:d.source})})),examined,static_rejected:rejected,capped_groups:capped,actions:actions.length,scans_per_group,candidates_per_group,seconds:(performance.now()-began)/1000}};
}
