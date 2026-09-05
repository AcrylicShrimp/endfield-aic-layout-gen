import { rotateFacility } from '../constructive-placement-only/model.mjs';

// Local diagnostic: preserve explicit direct/one-column route classes, not arbitrary waypoint paths.
export function orderInputs(catalogs,input) {
  const spec=structuredClone(input),nodes=new Map(spec.nodes.map(n=>[n.id,n])),decisions=[];
  const shape=id=>{const n=nodes.get(id),r=catalogs.recipes.recipes.find(r=>r.id===n.recipe),f=catalogs.facilities.facilities.find(f=>f.id===r.facility);return rotateFacility(f,n.rotation);};
  const ports=id=>shape(id).ports.map(p=>({...p,x:nodes.get(id).position[0]+p.position.x,y:nodes.get(id).position[1]+p.position.y}));
  const port=(id,name)=>{const p=ports(id).find(p=>p.id===name);if(!p)throw new Error(`Unknown port ${id}/${name}`);return p;};
  for(const target of new Set(spec.links.map(l=>l.target))){
    const incoming=spec.links.filter(l=>l.target===target).sort((a,b)=>port(a.source,a.source_port).y-port(b.source,b.source_port).y||a.id.localeCompare(b.id));
    const candidates=ports(target).filter(p=>p.direction==='input'&&p.transport==='belt'&&p.edge==='west').sort((a,b)=>a.y-b.y);
    if(candidates.length<incoming.length)throw new Error(`Insufficient distinct input ports at ${target}`);
    for(const [i,l]of incoming.entries()){decisions.push({kind:'input-port-order',link:l.id,source:l.source,before:l.target_port,after:candidates[i].id});l.target_port=candidates[i].id;}
  }
  for(const source of new Set(spec.links.map(l=>l.source))){
    const outgoing=spec.links.filter(l=>l.source===source);
    if(new Set(outgoing.map(l=>l.target)).size!==1)continue;
    const shifts=outgoing.map(l=>port(l.target,l.target_port).y-port(source,l.source_port).y);
    if(new Set(shifts).size!==1)continue;
    const dy=shifts[0];nodes.get(source).position[1]+=dy;
    decisions.push({kind:'align-exclusive-supplier',source,dy});
  }
  for(const l of spec.links){
    const a=port(l.source,l.source_port),b=port(l.target,l.target_port);
    if(!l.via.length){if(a.y!==b.y)throw new Error(`Direct link no longer aligned: ${l.id}`);}
    else if(l.via.length===2&&l.via[0][0]===l.via[1][0])l.via=[[l.via[0][0],a.y],[l.via[0][0],b.y]];
    else throw new Error(`Unsupported route class for port-order diagnostic: ${l.id}`);
  }
  // Old crossing coordinates are invalid after rebinding. Remaining conflicts need a new module
  // planning pass; the caller must independently validate this unaccepted no-module candidate.
  spec.crossing_bundle=null;
  return{spec,decisions,accepted:false};
}
