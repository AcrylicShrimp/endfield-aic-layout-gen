import {readFile} from 'node:fs/promises';
// Read-only diagnostic under the repository's current bridge model, not a complete layout solver.
// Each reported port is tested independently; paths do not reserve future traffic jointly.
if (process.argv.length !== 4) throw new Error('Usage: node boundary-access.mjs ASSEMBLY_REPORT COMPONENT_CATALOG');
const report = JSON.parse(await readFile(process.argv[2], 'utf8'));
const node = report.assembly.final_node;
const catalog = JSON.parse(await readFile(process.argv[3], 'utf8'));
const key = (x,y) => `${x},${y}`;
const layers = new Map();
const bridgeLayers = new Map();
for (const kind of ['belt','pipe']) {
  const blocked = new Set();
  for (const p of node.placements) for (let y=p.y;y<p.y+p.height;y++) for(let x=p.x;x<p.x+p.width;x++) blocked.add(key(x,y));
  for (const n of node.transport_networks.filter(n=>n.transport===kind)) for (const p of n.cells) blocked.add(key(p.x,p.y));
  const seen = new Set([key(-1,-1)]), queue=[[-1,-1]];
  for(let i=0;i<queue.length;i++) {
    const [x,y]=queue[i];
    for(const [nx,ny] of [[x-1,y],[x+1,y],[x,y-1],[x,y+1]]) {
      if(nx < -1 || ny < -1 || nx > node.bounds.width || ny > node.bounds.height) continue;
      const k=key(nx,ny);
      if(blocked.has(k)||seen.has(k)) continue;
      seen.add(k);queue.push([nx,ny]);
    }
  }
  layers.set(kind,seen);
  const bridge = catalog.components.find(c=>c.kind==='bridge' && c.transport===kind);
  const crossings = new Map();
  if (bridge) for (const n of node.transport_networks.filter(n=>n.transport===kind)) {
    for (const p of n.cells) {
      if (n.terminals.some(t=>t.position.x===p.x&&t.position.y===p.y) || node.logistics_components.some(c=>c.transport===kind&&c.position.x===p.x&&c.position.y===p.y)) continue;
      const incoming = n.segments.filter(s=>s.to.x===p.x&&s.to.y===p.y);
      const outgoing = n.segments.filter(s=>s.from.x===p.x&&s.from.y===p.y);
      if(incoming.length!==1||outgoing.length!==1) continue;
      const a=incoming[0],b=outgoing[0];
      const dx=p.x-a.from.x,dy=p.y-a.from.y;
      if(b.to.x-p.x!==dx || b.to.y-p.y!==dy) continue;
      if(a.rate.numerator/a.rate.denominator > bridge.capacity.quantity*1000/bridge.capacity.duration_ms) continue;
      crossings.set(key(p.x,p.y),{dx,dy});
    }
  }
  const bridged = new Map([[key(-1,-1),{previous:null,crossing:null}]]), frontier=[[-1,-1]];
  for(let i=0;i<frontier.length;i++) {
    const [x,y]=frontier[i];
    for(const [dx,dy] of [[-1,0],[1,0],[0,-1],[0,1]]) {
      let nx=x+dx,ny=y+dy,crossing=[];
      while(blocked.has(key(nx,ny))) {
        const straight=crossings.get(key(nx,ny));
        if(!straight || straight.dx*dx+straight.dy*dy!==0) break;
        crossing.push({x:nx,y:ny,component:bridge.id});
        nx+=dx;ny+=dy;
      }
      if(nx < -1 || ny < -1 || nx > node.bounds.width || ny > node.bounds.height || blocked.has(key(nx,ny))) continue;
      const k=key(nx,ny);
      if(bridged.has(k)) continue;
      bridged.set(k,{previous:key(x,y),crossing});frontier.push([nx,ny]);
    }
  }
  bridgeLayers.set(kind,bridged);
}
const bridgePath = (kind,p) => {
  const map=bridgeLayers.get(kind),result=[];
  let k=key(p.x,p.y);
  if(!map.has(k)) return null;
  while(map.get(k).previous!==null) { const entry=map.get(k); if(entry.crossing) result.push(...entry.crossing); k=entry.previous; }
  return result;
};
console.log(JSON.stringify(node.boundary_requirements.filter(b=>report.unresolved_facility_requirements.includes(b.requirement)).map(b=>({
  requirement:b.requirement,item:b.item,instance:b.inside_instance,direction:b.direction,rate:b.rate,
  ports:b.attachments.filter(a=>a.kind==='new-port').map(a=>({port:a.port.port,cell:a.port.connection,exterior_reachable:layers.get(b.transport).has(key(a.port.connection.x,a.port.connection.y)),bridge_crossings:bridgePath(b.transport,a.port.connection)})),
  network_attachments:b.attachments.filter(a=>a.kind==='existing-network').length
})),null,2));
