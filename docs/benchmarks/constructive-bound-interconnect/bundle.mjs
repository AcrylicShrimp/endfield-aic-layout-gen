import { capacity, compare, parseRate } from '../constructive-interconnect/rates.mjs';

// Cross two orthogonal, commodity-disjoint lane bundles with a preplanned bridge matrix.
export function crossingBundle(horizontal, vertical, origin, catalogs) {
  if(!horizontal.length||!vertical.length||horizontal.length*vertical.length>1024)throw new Error('Invalid bundle size or diagnostic bridge budget');
  const lanes=[...horizontal,...vertical];
  if(lanes.some(l=>!l||!l.id||!l.rate||!catalogs.items.items.some(i=>i.id===l.item&&i.transport===l.transport)))throw new Error('Unknown or incompatible bundle channel');
  const transport=lanes[0].transport;
  if(new Set(lanes.map(l=>l.id)).size!==lanes.length||lanes.some(l=>l.transport!==transport)||horizontal.some(a=>vertical.some(b=>a.item===b.item)))throw new Error('Bundle channels must be unique, same-layer and commodity-disjoint across axes');
  const line=catalogs.transports.transports.find(t=>t.kind===transport);
  const def=catalogs.components.components.find(c=>c.kind==='bridge'&&c.transport===transport&&c.footprint.width===1&&c.footprint.height===1&&c.allowed_rotations.includes(0)&&
    ['west','north'].every(d=>c.input_directions.includes(d))&&['east','south'].every(d=>c.output_directions.includes(d)));
  if(!line||!def||lanes.some(l=>!parseRate(l.rate).n||compare(parseRate(l.rate),capacity(line.capacity))>0||compare(parseRate(l.rate),capacity(def.capacity))>0))throw new Error('Bundle capacity or catalog bridge unavailable');
  const [x,y]=origin,components=[];
  if(![x,y].every(Number.isSafeInteger)||x<1||y<1)throw new Error('Bundle origin needs nonnegative boundary clearance');
  for(const [row,a]of horizontal.entries())for(const [column,b]of vertical.entries())components.push({id:`bundle:${a.id}:${b.id}`,component:def.id,kind:'bridge',transport,rotation:0,position:{x:x+column,y:y+row},lanes:[a.id,b.id]});
  return {kind:'orthogonal-lane-bundle',core:{x,y,width:vertical.length,height:horizontal.length},
    reserved:{x:x-1,y:y-1,width:vertical.length+2,height:horizontal.length+2},components,
    channels:[...horizontal.map((l,i)=>({...l,input:{x:x-1,y:y+i},output:{x:x+vertical.length,y:y+i}})),
      ...vertical.map((l,i)=>({...l,input:{x:x+i,y:y-1},output:{x:x+i,y:y+horizontal.length}}))]};
}
