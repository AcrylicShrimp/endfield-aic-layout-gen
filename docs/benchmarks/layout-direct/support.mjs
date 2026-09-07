// Observation support is an execution guard, separate from complete factory validity.
export function observationSupport(w,catalogs,plan){
  for(const k of ['grid_cells','max_tokens'])if(!Number.isSafeInteger(plan?.[k])||plan[k]<1)throw new Error(`Invalid observation support guard: ${k}`);
  let minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;
  const add=p=>{minX=Math.min(minX,p.x);minY=Math.min(minY,p.y);maxX=Math.max(maxX,p.x);maxY=Math.max(maxY,p.y);};
  const body=(p,width,height)=>{add(p);add({x:p.x+width-1,y:p.y+height-1});};
  for(const p of w.placements)body(p,p.width,p.height);
  for(const n of w.transport_networks)for(const p of n.cells)add(p);
  for(const c of w.logistics_components){
    const spec=catalogs.components.components.find(v=>v.id===c.component),turn=c.rotation%180!==0;
    if(!spec||!spec.allowed_rotations.includes(c.rotation))throw new Error(`Unknown observation component/rotation: ${c.component}/${c.rotation}`);
    body(c.position,spec.footprint[turn?'height':'width'],spec.footprint[turn?'width':'height']);
  }
  const width=maxX-minX+1,height=maxY-minY+1,grid_cells=width*height,tokens=w.placements.length+w.transport_networks.length+w.logistics_components.length;
  if(![minX,minY,maxX,maxY,width,height,grid_cells].every(Number.isSafeInteger)||width<1||height<1)throw new Error('Invalid observation geometry extent');
  const diagnostics=[];
  for(const [code,actual,limit]of [['grid',grid_cells,plan.grid_cells],['token',tokens,plan.max_tokens]])if(actual>limit)diagnostics.push({
    code:`direct-${code}-guard`,severity:'warning',stage:'observation',message:`Complete factory needs ${actual} ${code} entries; execution guard is ${limit}`,entity:{actual,limit}});
  return {supported:diagnostics.length===0,bounds:{minX,minY,maxX,maxY,width,height},grid_cells,tokens,diagnostics};
}

export function sizeStratum(facilities){
  if(facilities===undefined||facilities===null)return 'unknown-flow';
  if(!Number.isSafeInteger(facilities)||facilities<1)throw new Error('Invalid solved facility count');
  return facilities<2?'single':facilities<=16?'small':facilities<=64?'medium':facilities<=128?'large':'above-envelope';
}

export function internalTopology(nodes,edges){
  const ids=new Set(nodes.map(n=>n.id)),next=new Map([...ids].map(id=>[id,new Set()])),previous=new Map([...ids].map(id=>[id,new Set()]));
  for(const e of edges)if(ids.has(e.source)&&ids.has(e.target)){next.get(e.source).add(e.target);previous.get(e.target).add(e.source);}
  const unseen=new Set(ids),weak=[];
  while(unseen.size){const queue=[unseen.values().next().value];unseen.delete(queue[0]);for(let i=0;i<queue.length;i++)for(const id of [...next.get(queue[i]),...previous.get(queue[i])])if(unseen.delete(id))queue.push(id);weak.push(queue.length);}
  let number=0;const indices=new Map(),low=new Map(),stack=[],active=new Set(),cyclic=[];
  const visit=id=>{
    indices.set(id,number);low.set(id,number++);stack.push(id);active.add(id);
    for(const other of next.get(id))if(!indices.has(other)){visit(other);low.set(id,Math.min(low.get(id),low.get(other)));}else if(active.has(other))low.set(id,Math.min(low.get(id),indices.get(other)));
    if(low.get(id)===indices.get(id)){const component=[];let other;do{other=stack.pop();active.delete(other);component.push(other);}while(other!==id);
      if(component.length>1||next.get(id).has(id))cyclic.push(component.length);}
  };
  for(const id of ids)if(!indices.has(id))visit(id);
  return {weak_components:weak.length,weak_component_sizes:weak.sort((a,b)=>b-a),cyclic_scc_sizes:cyclic.sort((a,b)=>b-a),
    branching_facilities:[...next.values()].filter(s=>s.size>1).length,merging_facilities:[...previous.values()].filter(s=>s.size>1).length};
}
