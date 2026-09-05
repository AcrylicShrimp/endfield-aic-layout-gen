import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { permutationModule, polyline } from '../constructive-interconnect/geometry.mjs';
import { rate, add, multiply, parseRate, dto, compare, capacity } from '../constructive-interconnect/rates.mjs';
const key = p => `${p.x},${p.y}`, point = a => ({ x: a[0], y: a[1] }), pair = p => [p.x, p.y];
const delta = { north: [0,-1], east: [1,0], south: [0,1], west: [-1,0] };

export function buildFixture(catalogs, spec, merge, external) {
  if (spec.schema_version !== 1) throw new Error('Unsupported fixture schema');
  const recipe = id => { const r = catalogs.recipes.recipes.find(r => r.id === id); if (!r) throw new Error(`Unknown recipe ${id}`); return r; };
  const sources = spec.source_recipes.map(recipe), consumer = recipe(spec.consumer_recipe);
  if (sources.length !== 2 || sources.some(r => r.outputs.length !== 1)) throw new Error('Fixture requires two single-output recipes');
  const commodity = sources.map(r => r.outputs[0].item), consumerRate = commodity.map(item => {
    const a = consumer.inputs.find(a => a.item === item); if (!a) throw new Error(`Consumer cannot use ${item}`);
    return multiply(rate(BigInt(a.quantity) * 1000n, consumer.duration_ms), parseRate(spec.consumer_utilization));
  });
  if (commodity[0] === commodity[1]) throw new Error('Fixture must exercise different items');
  const sourceRate = consumerRate.map(r => multiply(r, rate(2)));
  const sourceUtil = sources.map((r, i) => {
    const full = rate(BigInt(r.outputs[0].quantity) * 1000n, r.duration_ms), total = sourceRate[i];
    return rate(total.n * full.d, total.d * full.n * BigInt(i === 0 && merge ? 2 : 1));
  });
  const witness = { schema_version: 1, placements: [], utilizations: {}, transport_networks: [], logistics_components: [] }, ports = new Map(), networks = new Map();
  const place = (id, r, u) => {
    const f = catalogs.facilities.facilities.find(f => f.id === r.facility);
    if (!f?.allowed_rotations.includes(spec.rotation) || compare(u, rate(1)) > 0) throw new Error(`Invalid facility/utilization ${id}`);
    const shape = rotateFacility(f, spec.rotation), [x,y] = spec.placements[id];
    witness.placements.push({ instance:id, recipe:r.id, facility:r.facility, x,y,width:shape.width,height:shape.height,rotation:spec.rotation });
    witness.utilizations[id] = dto(u);
    ports.set(id, shape.ports.map(p => ({...p, position:{x:x+p.position.x,y:y+p.position.y},
      connection:{x:x+p.position.x+delta[p.edge][0],y:y+p.position.y+delta[p.edge][1]} })));
  };
  place('A0', sources[0], sourceUtil[0]); if(merge)place('A1', sources[0], sourceUtil[0]);
  place('B', sources[1], sourceUtil[1]); for(const id of ['C0','C1'])place(id, consumer, parseRate(spec.consumer_utilization));
  const network = item => {
    if (!networks.has(item)) {
      const transport = catalogs.items.items.find(i => i.id === item)?.transport;
      if (transport !== 'belt') throw new Error(`Fixture only supports belt terminals: ${item}`);
      networks.set(item,{id:`network:${item}`,item,transport,requirement_ids:[`covered:${item}`],cells:new Map(),segments:new Map(),terminals:[],component_ids:[]});
    }
    return networks.get(item);
  };
  const terminal = (instance, portId, item, value) => {
    const p = ports.get(instance).find(p => p.id === portId); if(!p)throw new Error(`Missing port ${instance}/${portId}`);
    const n = network(item);
    n.terminals.push({id:`terminal:${instance}:${portId}`,node:instance,direction:p.direction,endpoint:{kind:'facility',instance,port:portId},position:p.connection,rate:dto(value)});
    return p.connection;
  };
  const addPath = (item,value,cells) => {
    const n=network(item);
    for(const p of cells)n.cells.set(key(p),p);
    for(let i=1;i<cells.length;i++) {
      const from=cells[i-1],to=cells[i],id=key(from)+'>'+key(to),old=n.segments.get(id);
      n.segments.set(id,{from,to,rate:dto(add(old?parseRate(old.rate):rate(0),value))});
    }
  };
  const route = (item,value,...points) => addPath(item,value,polyline(points.map(p => Array.isArray(p)?p:pair(p))));
  const a0=terminal('A0',spec.source_output_port,commodity[0],multiply(sourceRate[0],rate(1,merge?2:1)));
  const b=terminal('B',spec.source_output_port,commodity[1],sourceRate[1]);
  const demands = ['C0','C1'].map(id => commodity.map((item,i) => terminal(id,spec.consumer_input_ports[i],item,consumerRate[i])));
  const pts=Object.fromEntries(Object.entries(spec.points).map(([k,v])=>[k,point(v)]));
  const component = (id,kind,position,items) => {
    const def=catalogs.components.components.find(c=>c.kind===kind&&c.transport==='belt'&&c.footprint.width===1&&c.footprint.height===1&&c.allowed_rotations.includes(270));
    if(!def)throw new Error(`No catalog ${kind}`);
    witness.logistics_components.push({id,component:def.id,kind,transport:'belt',position,rotation:270});
    for(const item of items)network(item).component_ids.push(id);
  };
  component('splitA','splitter',pts.splitA,[commodity[0]]);component('splitB','splitter',pts.splitB,[commodity[1]]);
  if(merge) {
    component('mergeA','converger',pts.merge,[commodity[0]]);
    const a1=terminal('A1',spec.source_output_port,commodity[0],multiply(sourceRate[0],rate(1,2)));
    route(commodity[0],multiply(sourceRate[0],rate(1,2)),a0,pts.merge);
    route(commodity[0],multiply(sourceRate[0],rate(1,2)),a1,...spec.routes.A1_merge,pts.merge);
    route(commodity[0],sourceRate[0],pts.merge,pts.splitA);
  } else route(commodity[0],sourceRate[0],a0,pts.splitA);
  route(commodity[1],sourceRate[1],b,pts.splitB);
  const module=permutationModule(commodity.map((item,i)=>({id:String(i),item,transport:'belt',rate:dto(consumerRate[i])})),['1','0'],catalogs);
  const modulePaths=module.lanes.map(l=>l.cells.map(p=>({x:p.x+pts.module.x,y:p.y+pts.module.y})));
  for(const c of module.components) {
    witness.logistics_components.push({...c,id:`cross:${c.id}`,position:{x:c.position.x+pts.module.x,y:c.position.y+pts.module.y}});
    for(const item of commodity)network(item).component_ids.push(`cross:${c.id}`);
  }
  route(commodity[0],consumerRate[0],pts.splitA,demands[0][0]);
  route(commodity[0],consumerRate[0],pts.splitA,...spec.routes.splitA_module,modulePaths[0][0]);
  addPath(commodity[0],consumerRate[0],modulePaths[0]);
  route(commodity[0],consumerRate[0],modulePaths[0].at(-1),...spec.routes.module_C1,demands[1][0]);
  route(commodity[1],consumerRate[1],pts.splitB,...spec.routes.splitB_module,modulePaths[1][0]);
  addPath(commodity[1],consumerRate[1],modulePaths[1]);
  route(commodity[1],consumerRate[1],modulePaths[1].at(-1),...spec.routes.module_C0,demands[0][1]);
  route(commodity[1],consumerRate[1],pts.splitB,...spec.routes.splitB_C1,demands[1][1]);
  if(external) for(const p of witness.placements) {
    const r=recipe(p.recipe),u=parseRate(witness.utilizations[p.instance]);
    for(const direction of ['input','output'])for(const amount of r[direction+'s']) {
      const item=amount.item,n=network(item),required=multiply(rate(BigInt(amount.quantity)*1000n,r.duration_ms),u);
      const used=n.terminals.filter(t=>t.node===p.instance&&t.direction===direction).reduce((sum,t)=>add(sum,parseRate(t.rate)),rate(0));
      const remaining=rate(required.n*used.d-used.n*required.d,required.d*used.d);
      if(!remaining.n)continue;
      const selected=ports.get(p.instance).find(port=>port.direction===direction&&port.transport===n.transport&&port.edge===(direction==='input'?'west':'east')&&
        ![...networks.values()].some(n=>n.terminals.some(t=>t.endpoint.kind==='facility'&&t.endpoint.instance===p.instance&&t.endpoint.port===port.id)));
      if(!selected||compare(remaining,capacity(catalogs.transports.transports.find(t=>t.kind===n.transport).capacity))>0)throw new Error('External binding needs an additional capacity lane');
      const connection=terminal(p.instance,selected.id,item,remaining),outside={x:direction==='input'?0:spec.external_east,y:connection.y},id=`external:${p.instance}:${selected.id}`;
      n.terminals.push({id,node:id,direction:direction==='input'?'output':'input',endpoint:{kind:'external',node:id,side:direction==='input'?'west':'east'},position:outside,rate:dto(remaining)});
      if(direction==='input')route(item,remaining,outside,connection);else route(item,remaining,connection,outside);
    }
  }
  witness.transport_networks=[...networks.values()].filter(n=>n.cells.size).map(n=>({...n,cells:[...n.cells.values()],segments:[...n.segments.values()]}));
  return { witness, module:{...module,origin:pts.module}, catalog_ports:Object.fromEntries(ports),
    case:merge?'merge-split-cross':'split-cross',external_connected:external };
}
