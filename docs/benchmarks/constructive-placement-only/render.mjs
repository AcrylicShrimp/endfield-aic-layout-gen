// Standalone project diagnostic, with graph overlays explicitly separated from physical routing.
export function render(report) {
  const data = JSON.stringify(report).replaceAll('<', '\\u003c').replaceAll('\u2028', '\\u2028').replaceAll('\u2029', '\\u2029');
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Placement only · full factory geometry</title>
<style>:root{color-scheme:light dark;--bg:light-dark(#fafafa,#0c1720);--fg:light-dark(#152b3b,#d6e9f6);--line:light-dark(#c7d2da,#314e61);--block:light-dark(#e5edf2,#173447);--selected:light-dark(#166bae,#82c8fa);--new:light-dark(#16815f,#61ddba);--port:light-dark(#813eab,#d7a7f1);--pipe:light-dark(#077c99,#6bd5ee)}*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.45 system-ui}header,aside{padding:14px 18px}h1{font-size:20px;margin:0}p{margin:6px 0}.controls{display:flex;gap:10px;align-items:center;flex-wrap:wrap;margin-top:10px}button,input{font:inherit}button{border:1px solid var(--line);border-radius:4px;padding:7px 11px;color:inherit;background:transparent;cursor:pointer}button:disabled{opacity:.4;cursor:default}input[type=range]{width:min(300px,45vw)}label{display:flex;gap:6px;align-items:center}svg{display:block;width:100%;height:64vh;min-height:380px;border-block:1px solid var(--line);touch-action:none}.grid{stroke:var(--line);stroke-width:.04;fill:none}.facility{fill:var(--block);stroke:var(--fg);stroke-width:.07;cursor:pointer}.facility.selected{stroke:var(--selected);stroke-width:.22}.facility.new{stroke:var(--new);stroke-width:.15}.facility text{fill:var(--fg);stroke:none;font-family:ui-monospace,monospace;text-anchor:middle;dominant-baseline:middle}.logical{fill:none;stroke:var(--port);stroke-width:.12;stroke-dasharray:.35 .2;pointer-events:none}.logical.pipe{stroke:var(--pipe)}.ports line{stroke:var(--port);stroke-width:.12}.ports .pipe{stroke:var(--pipe)}.ports circle{fill:var(--bg);stroke:var(--port);stroke-width:.1}.ports .output{fill:var(--port)}.ports{pointer-events:none}aside{overflow-wrap:anywhere}#selected-details{white-space:pre-wrap}#status{font-variant-numeric:tabular-nums}small{font-size:13px}.notice{color:var(--port)}@media(max-width:480px){header,aside{padding:10px}button{min-height:42px}svg{height:58vh;min-height:320px}.controls{gap:8px}}</style>
<header><h1>${report.block_experiment ? 'Straight-chain blocks · two-pass placement experiment' : 'Placement only · belt flow → east · pipe supply ↑ north'}</h1><p class="notice">0 physical routes · no assigned ports · NOT A VALIDATED FACTORY</p><p id="status" aria-live="polite"></p>
<div class="controls"><button id="previous" type="button">Previous</button><label>Placed <input id="stage" type="range" min="1" max="${report.placements.length}" value="${report.placements.length}"></label><button id="next" type="button">Next</button><button id="reset" type="button">Fit shown</button></div>
<div class="controls"><label><input id="show-ports" type="checkbox" checked>Catalog ports (unassigned)</label><label><input id="show-graph" type="checkbox">Logical edges (NOT routes)</label><button id="clear" type="button">Clear selection</button></div>
<p><span style="color:var(--new)">IN → green</span> · <span style="color:var(--port)">OUT → purple</span> · solid lead: belt · dashed blue lead: pipe. Arrows show material direction.</p>
<small>${report.block_experiment ? 'Same-color members form one rigid chain candidate; outlined boxes are block envelopes. One empty cell inside chains; wider, degree-aware channels between blocks. Dashed arrows are logical dependencies, NOT routes. F labels match the frozen baseline.' : "One empty cell between facilities. Exclusive belt suppliers are packed together around their consumer's input ports. Pipe-rich branches prefer northern rows."} Wheel to zoom, drag to pan, select a facility for its inputs/outputs. No ports are assigned.</small></header>
<svg id="layout" role="img" aria-label="Full placement-only factory layout with no physical routes"></svg><aside><strong id="selected-title">Select a facility</strong><div id="selected-details">Inspect placement independently of routing. Catalog port positions are optional; none are assigned.</div></aside>
<script id="data" type="application/json">${data}</script><script>
const report=JSON.parse(document.getElementById('data').textContent);
const svg=document.getElementById('layout'), stage=document.getElementById('stage');
const ports=document.getElementById('show-ports'), graph=document.getElementById('show-graph');
const byId=new Map(report.placements.map(p=>[p.instance,p]));
const blockData=report.block_experiment;
if(blockData)graph.checked=true;
let selected=null, view=[-1,-1,report.bounds.width+2,report.bounds.height+2], drag=null;
const el=(tag,attrs={},text)=>{const e=document.createElementNS('http://www.w3.org/2000/svg',tag);for(const [k,v]of Object.entries(attrs))e.setAttribute(k,String(v));if(text!==undefined)e.textContent=text;return e;};
function applyView(){svg.setAttribute('viewBox',view.join(' '));const scale=Math.min(svg.clientWidth/view[2],svg.clientHeight/view[3]);const size=Math.max(1.15,11/Math.max(.01,scale));svg.querySelectorAll('text').forEach(t=>t.setAttribute('font-size',size));}
function fitShown(){const shown=report.placements.slice(0,Number(stage.value)),x=Math.min(...shown.map(p=>p.x))-1,y=Math.min(...shown.map(p=>p.y))-1;view=[x,y,Math.max(...shown.map(p=>p.x+p.width))+1-x,Math.max(...shown.map(p=>p.y+p.height))+1-y];applyView();}
function draw(fit=false){
 const count=Number(stage.value),visible=report.placements.slice(0,count),ids=new Set(visible.map(p=>p.instance));
 if(selected&&!ids.has(selected))selected=null;
 svg.replaceChildren();
 const defs=el('defs'),pattern=el('pattern',{id:'grid',width:1,height:1,patternUnits:'userSpaceOnUse'});
 pattern.append(el('path',{d:'M1 0H0V1',class:'grid'}));defs.append(pattern);
 const arrow=el('marker',{id:'logical-arrow',viewBox:'0 0 10 10',refX:9,refY:5,markerWidth:6,markerHeight:6,orient:'auto'});arrow.append(el('path',{d:'M0 0L10 5L0 10Z',fill:'var(--port)'}));defs.append(arrow);svg.append(defs);
 svg.append(el('rect',{x:-1,y:-1,width:report.bounds.width+2,height:report.bounds.height+2,fill:'url(#grid)'}));
 if(blockData)for(const [i,b]of blockData.blocks.entries()){
  if(!b.members.every(id=>ids.has(id)))continue;
  const box=el('rect',{x:b.x-.3,y:b.y-.3,width:b.width+.6,height:b.height+.6,fill:'none',stroke:'hsl('+(i*137.5%360)+' 55% 60%)','stroke-width':b.members.length>1?.16:.06,'stroke-dasharray':b.members.length>1?'':'.3 .2','pointer-events':'none'});
  box.append(el('title',{},b.id+': '+b.members.map(id=>byId.get(id).label).join(' → ')));svg.append(box);
 }
 if(graph.checked)for(const edge of report.internal_requirements){
  if(!ids.has(edge.source)||!ids.has(edge.target)||selected&&edge.source!==selected&&edge.target!==selected)continue;
  const a=byId.get(edge.source),b=byId.get(edge.target);
  if(blockData&&a.block===b.block)continue;
  const line=el('path',{class:'logical '+edge.transport,'marker-end':'url(#logical-arrow)',d:'M'+(a.x+a.width/2)+' '+(a.y+a.height/2)+' L'+(b.x+b.width/2)+' '+(b.y+b.height/2)});
  svg.append(line);
 }
 for(const p of visible){
  const g=el('g',{class:'facility'+(p.instance===selected?' selected':'')+(p.placement_index===count-1?' new':''),'data-instance':p.instance});
  if(blockData){const i=blockData.blocks.findIndex(b=>b.id===p.block);g.style.fill='light-dark(hsl('+(i*137.5%360)+' 40% 87%),hsl('+(i*137.5%360)+' 30% 23%))';}
  g.append(el('rect',{x:p.x,y:p.y,width:p.width,height:p.height}));
  g.append(el('text',{x:p.x+p.width/2,y:p.y+p.height/2},p.label));
  g.append(el('title',{},p.label+' · '+p.facility+' · '+p.rotation+'°'));svg.append(g);
  if(ports.checked){const pg=el('g',{class:'ports'});for(const port of p.catalog_ports){
   const a=port.position,b=port.connection,mx=(a.x+b.x)/2+.5,my=(a.y+b.y)/2+.5;
   const sign=port.direction==='output'?1:-1,dx=(b.x-a.x)*sign,dy=(b.y-a.y)*sign;
   pg.append(el('line',{x1:a.x+.5,y1:a.y+.5,x2:b.x+.5,y2:b.y+.5,class:port.transport,'stroke-dasharray':port.transport==='pipe'?'.12 .1':''}));
   const color=port.direction==='input'?'var(--new)':'var(--port)';
   const arrow=el('path',{d:'M'+(mx+dx*.27)+' '+(my+dy*.27)+' L'+(mx-dx*.19-dy*.2)+' '+(my-dy*.19+dx*.2)+' L'+(mx-dx*.19+dy*.2)+' '+(my-dy*.19-dx*.2)+' Z',fill:color,'data-port':port.id,'data-direction':port.direction,'data-transport':port.transport});
   arrow.append(el('title',{},port.id+' · '+port.direction+' '+port.transport+' · '+port.edge+' · unassigned'));pg.append(arrow);
  }svg.append(pg);}
 }
 document.getElementById('status').textContent=count+' / '+report.required_facilities+' facilities shown · final bounds '+report.bounds.width+' × '+report.bounds.height+' · 0 belt / 0 pipe tiles';
 document.getElementById('previous').disabled=count===1;document.getElementById('next').disabled=count===report.placements.length;
 const p=byId.get(selected);
 document.getElementById('selected-title').textContent=p?p.label+' · '+p.facility:'Select a facility';
 if(p&&!blockData){const d=report.placement_decisions.find(d=>d.instance===p.instance);
 const names=direction=>report.internal_requirements.filter(e=>e[direction]==p.instance).map(e=>byId.get(e[direction==='target'?'source':'target']).label+' ['+e.transport+', '+e.item+']').join(', ')||'none';
 document.getElementById('selected-details').textContent='Position ('+p.x+', '+p.y+'), '+p.width+' × '+p.height+', rotation '+p.rotation+'° · row shift '+d.row_shift+' cells\\nBranch pipe outputs: '+p.branch_pipe_outputs+' · row-choice north-pipe violations: '+d.row_choice.north_pipe_violations+' · branch inversions: '+d.row_choice.branch_priority_inversions+'\\nInputs: '+names('target')+'\\nOutputs: '+names('source')+'\\nCatalog ports (all unassigned):\\n'+p.catalog_ports.map(port=>port.id+' · '+port.direction+' '+port.transport+' · '+port.edge).join('\\n')+'\\nInstance: '+p.instance+'\\nNo port assignments or physical route feasibility checks.';
 if(d.supplier_group)document.getElementById('selected-details').textContent+='\\nSupplier group: '+d.supplier_group.map(id=>byId.get(id).label).join(', ')+'\\nAlignment: '+d.alignment_basis+' · group port distance sum: '+d.row_choice.port_distance_sum+' · max: '+d.row_choice.port_distance_max+' · interface center error: '+d.row_choice.interface_center_distance;
 }else document.getElementById('selected-details').textContent='Inspect placement independently of routing. Graph edges are abstract requirements, not belts or pipes.';
 if(blockData){document.getElementById('status').textContent+=' · '+blockData.summary.chains+' chains / '+blockData.summary.blocks+' blocks · '+report.placement_ms.toFixed(1)+' ms placement';
  if(p){const b=blockData.blocks.find(b=>b.id===p.block);document.getElementById('selected-details').textContent=b.id+' · '+b.members.map(id=>byId.get(id).label).join(' → ')+'\\nBlock '+b.width+' × '+b.height+' · boundary channel allowance '+b.channel+' cells (heuristic, not routing proof)\\n'+b.boundaries.map(e=>byId.get(e.instance).label+' '+e.direction+' '+e.transport+' ↔ '+(byId.get(e.other)?.label??'external')+' · '+e.item+' · '+e.rate.numerator+'/'+e.rate.denominator).join('\\n')+'\\nNo assigned ports or validated routes.';}
 }
 if(fit)fitShown();else applyView();
}
stage.addEventListener('input',()=>draw(true));ports.addEventListener('change',()=>draw());graph.addEventListener('change',()=>draw());
document.getElementById('previous').onclick=()=>{stage.value=Number(stage.value)-1;draw(true);};
document.getElementById('next').onclick=()=>{stage.value=Number(stage.value)+1;draw(true);};
document.getElementById('clear').onclick=()=>{selected=null;draw();};
document.getElementById('reset').onclick=fitShown;
const point=e=>new DOMPoint(e.clientX,e.clientY).matrixTransform(svg.getScreenCTM().inverse());
svg.addEventListener('wheel',e=>{e.preventDefault();const at=point(e),factor=e.deltaY>0?1.15:1/1.15;
 if(view[2]*factor<8||view[2]*factor>report.bounds.width*8)return;
 const px=(at.x-view[0])/view[2],py=(at.y-view[1])/view[3];
 view=[view[0]+view[2]*px*(1-factor),view[1]+view[3]*py*(1-factor),view[2]*factor,view[3]*factor];applyView();},{passive:false});
svg.addEventListener('pointerdown',e=>{drag={x:e.clientX,y:e.clientY,inverse:svg.getScreenCTM().inverse(),point:point(e),view:view.slice(),moved:false,target:e.target.closest('[data-instance]')?.dataset.instance};svg.setPointerCapture(e.pointerId);});
svg.addEventListener('pointermove',e=>{if(!drag)return;const at=new DOMPoint(e.clientX,e.clientY).matrixTransform(drag.inverse);if(Math.abs(e.clientX-drag.x)+Math.abs(e.clientY-drag.y)>4)drag.moved=true;view=[drag.view[0]-(at.x-drag.point.x),drag.view[1]-(at.y-drag.point.y),drag.view[2],drag.view[3]];applyView();});
svg.addEventListener('pointerup',e=>{if(drag&&!drag.moved){selected=drag.target??null;draw();}drag=null;svg.releasePointerCapture(e.pointerId);});
svg.addEventListener('pointercancel',()=>{drag=null;});window.addEventListener('resize',applyView);draw();
</script></html>`;
}
