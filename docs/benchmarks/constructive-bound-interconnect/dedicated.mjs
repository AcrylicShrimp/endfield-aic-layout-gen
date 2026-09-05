import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
import { rate, add, multiply, parseRate, dto, compare, capacity } from '../constructive-interconnect/rates.mjs';
import { crossingBundle } from './bundle.mjs';
const key=p=>`${p.x},${p.y}`,pair=p=>[p.x,p.y],delta={north:[0,-1],east:[1,0],south:[0,1],west:[-1,0]};

export function buildDedicated(catalogs,spec) {
  if(spec.schema_version!==1)throw new Error('Unsupported explicit binding fixture');
  const nodes=new Map(spec.nodes.map(n=>[n.id,n])),recipes=new Map(catalogs.recipes.recipes.map(r=>[r.id,r]));
  if(nodes.size!==spec.nodes.length||new Set(spec.links.map(l=>l.id)).size!==spec.links.length)throw new Error('Duplicate node/link ID');
  const selected=new Map(spec.nodes.map(n=>{const r=recipes.get(n.recipe);if(!r)throw new Error(`Unknown recipe ${n.recipe}`);return[n.id,r];}));
  const links=spec.links.map(l=>{
    const source=selected.get(l.source),target=selected.get(l.target),u=nodes.get(l.target)?.utilization;
    if(!source||!target||source.outputs.length!==1||!u||u==='derived')throw new Error('Explicit binding requires a single-output source and fixed consumer utilization');
    const item=source.outputs[0].item,a=target.inputs.find(a=>a.item===item),transport=catalogs.items.items.find(i=>i.id===item)?.transport;
    if(!a||!transport)throw new Error(`Incompatible recipe link ${l.id}`);
    const value=multiply(multiply(rate(BigInt(a.quantity)*1000n,target.duration_ms),parseRate(u)),parseRate(l.target_fraction));
    return{...l,item,transport,rate:dto(value)};
  });
  const utilizations={};
  for(const n of spec.nodes){let u;
    if(n.utilization==='derived'){
      const r=selected.get(n.id);if(r.outputs.length!==1)throw new Error('Derived producer must have one output');
      const total=links.filter(l=>l.source===n.id).reduce((s,l)=>add(s,parseRate(l.rate)),rate(0)),full=rate(BigInt(r.outputs[0].quantity)*1000n,r.duration_ms);
      u=rate(total.n*full.d,total.d*full.n);
    }else u=parseRate(n.utilization);
    if(!u.n||compare(u,rate(1))>0)throw new Error(`Utilization overflow ${n.id}`);utilizations[n.id]=dto(u);
  }
  const bundleSpec=spec.crossing_bundle,byLink=new Map(links.map(l=>[l.id,l]));
  const module=bundleSpec?crossingBundle(bundleSpec.horizontal_links.map(id=>byLink.get(id)),bundleSpec.vertical_links.map(id=>byLink.get(id)),bundleSpec.origin,catalogs):null;
  const witness={schema_version:1,placements:[],utilizations,transport_networks:[],logistics_components:(module?.components??[]).map(({lanes,...c})=>c)},ports={},networks=new Map(),usedPorts=new Set();
  for(const n of spec.nodes){const r=selected.get(n.id),f=catalogs.facilities.facilities.find(f=>f.id===r.facility);
    if(!f?.allowed_rotations.includes(n.rotation))throw new Error(`Invalid facility rotation ${n.id}`);
    const shape=rotateFacility(f,n.rotation),[x,y]=n.position;
    witness.placements.push({instance:n.id,recipe:r.id,facility:r.facility,x,y,width:shape.width,height:shape.height,rotation:n.rotation});
    ports[n.id]=shape.ports.map(p=>({...p,position:{x:x+p.position.x,y:y+p.position.y},connection:{x:x+p.position.x+delta[p.edge][0],y:y+p.position.y+delta[p.edge][1]}}));
  }
  const network=item=>{if(!networks.has(item)){const transport=catalogs.items.items.find(i=>i.id===item)?.transport;if(!transport)throw new Error(`Unknown item ${item}`);
    networks.set(item,{id:`network:${item}`,item,transport,requirement_ids:[],cells:new Map(),segments:new Map(),terminals:[],component_ids:[]});}return networks.get(item);};
  const terminal=(id,portId,item,value)=>{const p=ports[id].find(p=>p.id===portId),n=network(item),tag=id+'/'+portId;
    if(!p||p.transport!==n.transport||usedPorts.has(tag))throw new Error(`Missing, incompatible or reused port ${tag}`);usedPorts.add(tag);
    n.terminals.push({id:tag,node:id,direction:p.direction,endpoint:{kind:'facility',instance:id,port:portId},position:p.connection,rate:dto(value)});return p.connection;};
  const path=(item,value,points)=>{const n=network(item),cells=polyline(points.map(p=>Array.isArray(p)?p:pair(p)));
    for(const p of cells)n.cells.set(key(p),p);
    for(let i=1;i<cells.length;i++){const from=cells[i-1],to=cells[i],id=key(from)+'>'+key(to),old=n.segments.get(id);n.segments.set(id,{from,to,rate:dto(add(old?parseRate(old.rate):rate(0),value))});}return cells;};
  const routes=new Map();
  for(const l of links){const value=parseRate(l.rate),from=terminal(l.source,l.source_port,l.item,value),to=terminal(l.target,l.target_port,l.item,value);
    network(l.item).requirement_ids.push(l.id);routes.set(l.id,path(l.item,value,[from,...l.via,to]));}
  // Verify that the explicit physical bindings actually traverse each preplanned channel straight.
  for(const channel of module?.channels??[]){const cells=routes.get(channel.id),a=cells.findIndex(p=>key(p)===key(channel.input)),b=cells.findIndex(p=>key(p)===key(channel.output));
    const expected=polyline([pair(channel.input),pair(channel.output)]);
    if(a<0||b<a||JSON.stringify(cells.slice(a,b+1))!==JSON.stringify(expected))throw new Error(`Binding misses bundle channel ${channel.id}`);}
  for(const c of module?.components??[])for(const id of c.lanes){const n=network(byLink.get(id).item);if(!n.component_ids.includes(c.id))n.component_ids.push(c.id);}
  for(const n of spec.nodes){const r=selected.get(n.id),u=parseRate(utilizations[n.id]);
    for(const direction of ['input','output'])for(const a of r[direction+'s']){
      const required=multiply(rate(BigInt(a.quantity)*1000n,r.duration_ms),u),net=network(a.item);
      const routed=net.terminals.filter(t=>t.node===n.id&&t.direction===direction).reduce((s,t)=>add(s,parseRate(t.rate)),rate(0));
      const remaining=rate(required.n*routed.d-routed.n*required.d,required.d*routed.d);if(!remaining.n)continue;
      const cap=capacity(catalogs.transports.transports.find(t=>t.kind===net.transport).capacity);
      if(compare(remaining,cap)>0)throw new Error(`External binding needs more than one lane at ${n.id}`);
      const port=ports[n.id].find(p=>p.direction===direction&&p.transport===net.transport&&p.edge===(direction==='input'?'west':'east')&&!usedPorts.has(n.id+'/'+p.id));
      if(!port)throw new Error(`No external port ${n.id}`);
      const connection=terminal(n.id,port.id,a.item,remaining),outside={x:direction==='input'?0:spec.external_east,y:connection.y},id=`external:${n.id}:${port.id}`;
      net.requirement_ids.push(id);net.terminals.push({id,node:id,direction:direction==='input'?'output':'input',endpoint:{kind:'external',node:id,side:direction==='input'?'west':'east'},position:outside,rate:dto(remaining)});
      path(a.item,remaining,direction==='input'?[outside,connection]:[connection,outside]);
    }
  }
  witness.transport_networks=[...networks.values()].filter(n=>n.cells.size).map(n=>({...n,cells:[...n.cells.values()],segments:[...n.segments.values()]}));
  return{witness,module,catalog_ports:ports,case:'dedicated-full-rate',external_connected:true,bindings:links};
}
