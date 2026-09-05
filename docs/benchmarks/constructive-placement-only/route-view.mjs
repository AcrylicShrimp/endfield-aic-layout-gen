// Reuse the placement inspector; physical witness and failed logical requirements are separate layers.
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import { render } from './render.mjs';
const [placementFile, routingFile, output] = process.argv.slice(2);
if (process.argv.length !== 5) throw new Error('Usage: node route-view.mjs FROZEN_PLACEMENT ROUTING_REPORT NEW_HTML');
const bytes = fs.readFileSync(placementFile), placement = JSON.parse(bytes);
const routing = JSON.parse(fs.readFileSync(routingFile));
if (routing.schema_version !== 2 || routing.kind !== 'fixed-placement-routing-probe' || !routing.covered_routes_validated
    || !routing.coordinates_frozen || routing.factory_validated
    || routing.provenance.placement_sha256 !== createHash('sha256').update(bytes).digest('hex'))
  throw new Error('Expected a validated covered-route witness for this exact frozen placement, not a factory-success claim');
const safe = JSON.stringify(routing).replaceAll('<', '\\u003c').replaceAll('\u2028', '\\u2028').replaceAll('\u2029', '\\u2029');
const extra = `<script id="routing-data" type="application/json">${safe}</script><script>
const routing=JSON.parse(document.getElementById('routing-data').textContent),shift=routing.search_translation;
const requirements=new Map([...report.internal_requirements,...report.external_requirements].map(e=>[e.id,e]));
document.title='Fixed placement · physical routing probe';
document.querySelector('h1').textContent='Fixed placement · placement order → nearest connections → nearest ports';
document.querySelector('.notice').textContent='Covered routes validated · external connections pending · NOT A COMPLETE FACTORY';
document.querySelector('.controls').style.display='none';stage.value=report.placements.length;
document.querySelector('label:has(#show-ports)').lastChild.textContent=' Catalog ports (some assigned)';
document.querySelector('header small').textContent='Frozen facility coordinates. Gold: physical belts. Blue: physical pipes. Red dashed: selected FAILED logical requirement, not a route. Select a failure to inspect; wheel to zoom, drag to pan.';
const selector=document.createElement('select');selector.id='failure';selector.setAttribute('aria-label','Unresolved internal requirement');
selector.style.maxWidth='100%';selector.style.minWidth='0';
selector.append(new Option('Whole routed layout',''));
for(const id of routing.unresolved_internal){const e=requirements.get(id);selector.append(new Option(byId.get(e.source).label+' → '+byId.get(e.target).label+' · '+e.transport+' · '+e.item,id));}
const control=document.createElement('label');control.textContent='Inspect failure ';control.append(selector);document.querySelector('header').append(control);
control.style.display='block';
const failures=document.createElement('p');failures.id='failure-detail';document.querySelector('header').append(failures);
const originalDraw=draw;
draw=function(){originalDraw();svg.querySelectorAll('[data-port] title').forEach(t=>t.textContent=t.textContent.replace('unassigned','catalog candidate'));const group=el('g',{'pointer-events':'none'});
 for(const n of routing.final_node.transport_networks){const color=n.transport==='belt'?'light-dark(#9b650c,#e9be6c)':'light-dark(#047c96,#68d9ed)';
  for(const p of n.cells)group.append(el('rect',{x:p.x-shift+.18,y:p.y-shift+.18,width:.64,height:.64,fill:color,opacity:.7}));
  for(const s of n.segments)group.append(el('line',{x1:s.from.x-shift+.5,y1:s.from.y-shift+.5,x2:s.to.x-shift+.5,y2:s.to.y-shift+.5,stroke:color,'stroke-width':.18}));
 }
 svg.append(group);const edge=requirements.get(selector.value);
 if(edge){const a=byId.get(edge.source),b=byId.get(edge.target);svg.append(el('path',{d:'M'+(a.x+a.width/2)+' '+(a.y+a.height/2)+' L'+(b.x+b.width/2)+' '+(b.y+b.height/2),fill:'none',stroke:'light-dark(#b72d32,#ff8790)','stroke-width':.14,'stroke-dasharray':'.3 .25'}));
  const attempt=routing.attempts.findLast(t=>t.requirement===edge.id);failures.textContent='Unresolved '+a.label+' → '+b.label+': '+attempt.port_pairs+' port pairs, '+attempt.astar_searches+' A* calls, '+attempt.witness_rejections+' invalid-witness rejections, '+attempt.future_domain_rejections+' future-domain rejections. Bounded search failure, not infeasibility.';
  failures.textContent+=' Placement index '+attempt.placement_index+', nearest port distance '+attempt.nearest_port_distance+'.';
  const affected=attempt.rejected_future_requirements.map(id=>requirements.get(id)).filter(Boolean);
  if(affected.length)failures.textContent+=' Rejected candidates would remove the last attachment option for: '+affected.map(e=>(byId.get(e.source)?.label??'external')+' → '+(byId.get(e.target)?.label??'external')).join(', ')+'.';
 }else failures.textContent='Choose an unresolved requirement to inspect its endpoints and final attempt. No facility relocation was performed.';
 const count=routing.required_internal-routing.unresolved_internal.length;
 document.getElementById('status').textContent=report.placements.length+' facilities · '+count+' / '+routing.required_internal+' internal requirements connected · '+routing.unresolved_external.length+' external requirements pending · '+routing.elapsed_ms.toFixed(1)+' ms routing probe';
 if(selected)document.getElementById('selected-details').textContent=document.getElementById('selected-details').textContent.replace('No port assignments or physical route feasibility checks.','Physical routes shown; unresolved requirements and external wiring remain.').replace('Catalog ports (all unassigned):','Catalog port candidates:');
};
function fitRouting(){const edge=requirements.get(selector.value);
 const points=edge?[byId.get(edge.source),byId.get(edge.target)].flatMap(p=>[{x:p.x-3,y:p.y-3},{x:p.x+p.width+3,y:p.y+p.height+3}]):
 [...report.placements.flatMap(p=>[{x:p.x-1,y:p.y-1},{x:p.x+p.width+1,y:p.y+p.height+1}]),...routing.final_node.transport_networks.flatMap(n=>n.cells.map(p=>({x:p.x-shift,y:p.y-shift})))];
 const x=Math.min(...points.map(p=>p.x))-1,y=Math.min(...points.map(p=>p.y))-1;view=[x,y,Math.max(...points.map(p=>p.x))+2-x,Math.max(...points.map(p=>p.y))+2-y];applyView();}
selector.onchange=()=>{const e=requirements.get(selector.value);selected=e?.source??null;draw();fitRouting();};
const fit=document.createElement('button');fit.type='button';fit.textContent='Fit view';fit.onclick=fitRouting;control.after(fit);
draw();fitRouting();
</script>`;
fs.writeFileSync(output, render(placement).replace('</html>', extra + '</html>'), { flag: 'wx' });
console.log(JSON.stringify({ visualization_output: output, connected: routing.required_internal-routing.unresolved_internal.length, unresolved: routing.unresolved_internal.length }));
