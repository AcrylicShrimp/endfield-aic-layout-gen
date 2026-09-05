import { render } from '../constructive-placement-only/render.mjs';
export function zeroView(result) {
  const safe = JSON.stringify(result).replaceAll('<', '\\u003c').replaceAll('\u2028', '\\u2028').replaceAll('\u2029', '\\u2029');
  const extra = `<script id="zero-data" type="application/json">${safe}</script><script>
const zero=JSON.parse(document.getElementById('zero-data').textContent);
document.title='Zero-crossing interfaces · unresolved factory';
document.querySelector('h1').textContent='Zero-crossing interfaces · ordering before routing';
document.querySelector('.notice').textContent='FULL GRAPH GATE CLOSED · no factory routing performed · local geometry is not a complete factory';
document.querySelector('header small').textContent='Local certificates use independent lanes with zero shared grid cells. The whole candidate retains every facility and requirement, including unresolved crossings. No connections are deleted.';
const local=el('svg',{id:'local-view',role:'img','aria-label':'Local zero-crossing corridor'});svg.before(local);
const nav=document.createElement('div');nav.className='controls';document.querySelector('header').append(nav);
const chooser=document.createElement('select');chooser.id='corridor-select';chooser.style.maxWidth='100%';
for(const c of zero.corridors)chooser.append(new Option(c.id+' · '+c.edges.length+' requirements · '+c.status,c.id));nav.append(chooser);
const modes=['Certified lanes','Before ordering','Next pattern','Whole candidate'];let mode=modes[0];
for(const name of modes){const b=document.createElement('button');b.textContent=name;b.onclick=()=>{mode=name;showLocal();};nav.append(b);}
const detail=document.createElement('p');detail.id='zero-detail';nav.after(detail);
const ready=zero.corridors.filter(c=>c.certificate?.status==='local-zero-crossing-certificate').sort((a,b)=>b.original.inversion_pairs.length-a.original.inversion_pairs.length);
chooser.value=ready[0]?.id??zero.corridors[0]?.id;chooser.onchange=showLocal;
function schematic(sources,targets,edges){
 local.setAttribute('viewBox','-1 -1 16 '+(Math.max(sources.length,targets.length)*2+1));
 for(const e of edges){const a=sources.indexOf(e.source),b=targets.indexOf(e.target);local.append(el('line',{x1:3,y1:a*2+.5,x2:10,y2:b*2+.5,stroke:'#e38c96','stroke-width':.07}));}
 for(const [ids,x]of [[sources,0],[targets,10]])for(const [i,id]of ids.entries()){
  local.append(el('rect',{x,y:i*2,width:3,height:1,fill:'var(--block)',stroke:'var(--fg)','stroke-width':.03}));
  local.append(el('text',{x:x+1.5,y:i*2+.65,'text-anchor':'middle',fill:'var(--fg)','font-size':.5},zero.labels[id]));}
}
function showLocal(){
 const whole=mode==='Whole candidate';svg.style.display=whole?'block':'none';local.style.display=whole?'none':'block';
 document.querySelector('aside').style.display=whole?'block':'none';document.querySelectorAll('header > .controls').forEach(e=>{if(e!==nav)e.style.display=whole?'flex':'none';});
 local.replaceChildren();const c=zero.corridors.find(c=>c.id===chooser.value);
 if(whole){graph.checked=true;selected=null;draw();fitShown();detail.textContent='Unaccepted full candidate: all 59 facilities and every requirement retained. Several corridors remain unresolved; this drawing is NOT crossing-free.';return;}
 if(mode==='Next pattern'){
  const blocked=zero.corridors.find(c=>c.order.witness),w=blocked?.order.witness;
  if(!w){detail.textContent='No K2,2 witness in this report.';return;}
  schematic(w.sources,w.targets,blocked.edges.filter(e=>w.requirements.includes(e.id)));
  detail.textContent='FIXED-PAIR BASELINE: '+w.sources.map(id=>zero.labels[id]).join(' + ')+' each feed '+w.targets.map(id=>zero.labels[id]).join(' + ')+'. These four fixed links cannot be ordered. This is NOT recipe necessity: same-item flow reassignment can remove this example while preserving all terminal rates. See redistribution.json; physical routing remains unvalidated.';return;
 }
 if(mode==='Before ordering'){schematic(c.original.sources,c.original.targets,c.edges);detail.textContent='BEFORE: '+c.original.inversion_pairs.length+' strict logical order inversions. Schematic edges, NOT physical routes.';return;}
 const cert=c.certificate;
 if(cert?.status!=='local-zero-crossing-certificate'){schematic(c.original.sources,c.original.targets,c.edges);detail.textContent=c.order.message??cert?.message??'Unresolved';return;}
 local.setAttribute('viewBox','-1 -1 '+(cert.width+2)+' '+(cert.height+2));
 const colors=new Map([...new Set(cert.lanes.map(l=>l.item))].map((id,i)=>[id,'hsl('+(40+i*137.5%320)+' 65% 60%)']));
 for(const b of cert.blocks)local.append(el('rect',{x:b.x-.2,y:b.y-.2,width:b.width+.4,height:b.height+.4,fill:'none',stroke:'var(--line)','stroke-width':.05}));
 for(const lane of cert.lanes){local.append(el('polyline',{points:lane.cells.map(p=>(p.x+.5)+','+(p.y+.5)).join(' '),fill:'none',stroke:colors.get(lane.item),'stroke-width':.18}));}
 for(const p of cert.placements){local.append(el('rect',{x:p.x,y:p.y,width:p.width,height:p.height,fill:'var(--block)',stroke:'var(--fg)','stroke-width':.06}));
  local.append(el('text',{x:p.x+p.width/2,y:p.y+p.height/2+.3,'text-anchor':'middle',fill:'var(--fg)','font-size':.95},p.label));
  for(const port of p.catalog_ports){const a=port.position,b=port.connection;
   local.append(el('line',{x1:a.x+.5,y1:a.y+.5,x2:b.x+.5,y2:b.y+.5,stroke:port.direction==='input'?'var(--new)':'var(--port)','stroke-width':.1,'stroke-dasharray':port.transport==='pipe'?'.12 .1':''}));}
 }
 detail.textContent='AFTER: '+c.edges.length+' logical requirements → '+cert.lane_count+' full-demand lanes · ZERO shared/crossing grid cells · '+cert.width+' × '+cert.height+' local envelope. Concrete facing ports assigned for this isolated corridor only. Internal chain wiring and global composition are not certified.';
 document.getElementById('status').textContent=zero.summary.locally_certified+' / '+zero.summary.corridors+' forward corridors locally certified · full graph gate CLOSED';
}
showLocal();
</script>`;
  return render(result.composition.candidate).replace('</html>', extra + '</html>');
}
