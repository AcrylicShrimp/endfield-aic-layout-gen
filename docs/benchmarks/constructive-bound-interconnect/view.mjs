export function usedBounds(w) {
  const points = [...w.placements.flatMap(p => [p, { x: p.x + p.width - 1, y: p.y + p.height - 1 }]),
    ...w.transport_networks.flatMap(n => n.cells), ...w.logistics_components.map(c => c.position)];
  const x = Math.min(...points.map(p => p.x)), y = Math.min(...points.map(p => p.y));
  return { x, y, width: Math.max(...points.map(p => p.x)) - x + 1, height: Math.max(...points.map(p => p.y)) - y + 1 };
}
export function render(detail, validation, catalogs) {
  const data = JSON.stringify({detail,validation,catalogs,used_bounds:usedBounds(detail.witness)}).replaceAll('<','\\u003c');
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Recipe-bound interconnect</title>
<style>:root{color-scheme:light dark;--bg:#f6f8fb;--fg:#193341;--grid:#d8e1e9;--facility:#dbe9f1;--a:#946006;--b:#06787e;--other:#697c92;--component:#7251a8;--bad:#c92e45;--item-lightness:38%}
@media(prefers-color-scheme:dark){:root{--bg:#07131d;--fg:#d7eaf2;--grid:#203746;--facility:#183442;--a:#efb55b;--b:#62d6d1;--other:#93a4bb;--component:#c2a0ff;--bad:#ff6f85;--item-lightness:70%}}
*{box-sizing:border-box}body{margin:0;padding:24px;background:var(--bg);color:var(--fg);font:16px system-ui}h1{font-size:24px;margin:0 0 10px}p{line-height:1.5;max-width:1100px;overflow-wrap:anywhere}svg{display:block;width:100%;max-height:820px}text{fill:var(--fg);font-family:ui-monospace,monospace}.legend{display:flex;flex-wrap:wrap;gap:15px}.legend span{overflow-wrap:anywhere}button{font:inherit;padding:8px 12px;cursor:pointer}nav{display:flex;gap:10px;flex-wrap:wrap}@media(max-width:480px){body{padding:12px}h1{font-size:20px}}</style>
<h1>Recipe-bound interconnect · actual catalog facilities</h1><p id="status"></p><nav><button id="normal">Validated geometry</button><button id="overload">Same geometry at four times the rate</button><button id="fit">Fit diagram</button><span>Wheel to zoom · drag to pan</span></nav><p id="explanation" aria-live="polite"></p><div class="legend"><span id="itemA" style="color:var(--a)"></span><span id="itemB" style="color:var(--b)"></span><span>Muted: boundary-only commodities</span><span>S: splitter · M: converger · X: bridge</span></div>
<svg id="diagram" role="img" aria-label="Facility ports, typed networks and preplanned logistics components"></svg><p id="scope"></p>
<script type="application/json" id="data">${data}</script><script>
const {detail,validation,catalogs,used_bounds}=JSON.parse(document.getElementById('data').textContent),w=detail.witness,svg=document.getElementById('diagram');let overload=false,view,fullView,layout,pan;
const el=(name,attrs={},text='')=>{const e=document.createElementNS('http://www.w3.org/2000/svg',name);for(const[k,v]of Object.entries(attrs))e.setAttribute(k,v);e.textContent=text;return e;};
const value=r=>r.numerator/r.denominator,rateText=r=>r.denominator===1?String(r.numerator):r.numerator+'/'+r.denominator;
const sources=w.transport_networks.filter(n=>n.terminals.some(t=>t.endpoint.kind==='facility'&&t.direction==='output')&&n.terminals.some(t=>t.endpoint.kind==='facility'&&t.direction==='input')).map(n=>n.item);
document.getElementById('itemA').textContent='A: '+sources[0];document.getElementById('itemB').textContent='B: '+sources[1];
const color=item=>{const i=sources.indexOf(item);return i===0?'var(--a)':i===1?'var(--b)':i<0?'var(--other)':'hsl('+((i*137.508)%360)+' 65% var(--item-lightness))';};
for(const item of sources.slice(2)){const e=document.createElement('span');e.textContent=item;e.style.color=color(item);document.querySelector('.legend').append(e);}
function applyView(){svg.setAttribute('viewBox',view.join(' '));}
function focusLabels(labels){const ps=w.placements.filter(p=>labels.includes(detail.labels?.[p.instance]??p.instance));if(!ps.length)return false;
const left=Math.min(...ps.map(p=>p.x))-2,top=Math.min(...ps.map(p=>p.y))-2,right=Math.max(...ps.map(p=>p.x+p.width))+2,bottom=Math.max(...ps.map(p=>p.y+p.height))+2;
view=[layout.x+left*layout.s,layout.y+top*layout.s,(right-left)*layout.s,(bottom-top)*layout.s];applyView();return true;}
function draw(){svg.replaceChildren();const points=[...w.transport_networks.flatMap(n=>n.cells),...w.logistics_components.map(c=>c.position)],width=Math.max(...w.placements.map(p=>p.x+p.width),...points.map(p=>p.x+1)),height=Math.max(...w.placements.map(p=>p.y+p.height),...points.map(p=>p.y+1));
const s=Math.min(33,(svg.clientWidth-20)/(width+1)),x=10,y=10,xy=p=>({x:x+(p.x+.5)*s,y:y+(p.y+.5)*s});layout={s,x,y};view=[0,0,svg.clientWidth,height*s+25];fullView=[...view];applyView();
for(let i=0;i<=width;i++)svg.append(el('line',{x1:x+i*s,y1:y,x2:x+i*s,y2:y+height*s,stroke:'var(--grid)','stroke-width':.5}));
for(let i=0;i<=height;i++)svg.append(el('line',{x1:x,y1:y+i*s,x2:x+width*s,y2:y+i*s,stroke:'var(--grid)','stroke-width':.5}));
for(const n of w.transport_networks){const cap=catalogs.transports.transports.find(t=>t.kind===n.transport).capacity,limit=cap.quantity*1000/cap.duration_ms;
for(const segment of n.segments){const a=xy(segment.from),b=xy(segment.to),bad=value(segment.rate)*(overload?4:1)>limit;svg.append(el('line',{x1:a.x,y1:a.y,x2:b.x,y2:b.y,stroke:bad?'var(--bad)':color(n.item),'stroke-width':Math.max(1.5,s*.16)}));}
for(const t of n.terminals)if(t.endpoint.kind==='external'){const a=xy(t.position);svg.append(el('circle',{cx:a.x,cy:a.y,r:Math.max(2,s*.17),fill:color(n.item)}));}}
for(const p of w.placements){svg.append(el('rect',{x:x+p.x*s,y:y+p.y*s,width:p.width*s,height:p.height*s,fill:'var(--facility)',stroke:'var(--fg)','stroke-width':1}));
svg.append(el('text',{x:x+(p.x+p.width/2)*s,y:y+(p.y+p.height/2)*s,'text-anchor':'middle','font-size':Math.max(11,Math.min(22,s*.8))},detail.labels?.[p.instance]??p.instance));
if(s>16)svg.append(el('text',{x:x+(p.x+p.width/2)*s,y:y+(p.y+p.height/2)*s+18,'text-anchor':'middle','font-size':11},p.facility.split('-')[0]));
for(const q of detail.catalog_ports[p.instance]){const a=xy(q.position),b=xy(q.connection);svg.append(el('line',{x1:a.x,y1:a.y,x2:b.x,y2:b.y,stroke:q.direction==='input'?'var(--b)':'var(--a)','stroke-width':Math.max(1,s*.06)}));}}
for(const c of w.logistics_components){const a=xy(c.position),r=Math.max(5,s*.35);svg.append(el('rect',{x:a.x-r,y:a.y-r,width:2*r,height:2*r,fill:'var(--bg)',stroke:'var(--component)','stroke-width':2}));svg.append(el('text',{x:a.x,y:a.y+4,'text-anchor':'middle','font-size':12},c.kind==='bridge'?'X':c.kind==='splitter'?'S':'M'));}
if(detail.move_diagnostic){const m=detail.move_diagnostic;
for(const p of w.placements.filter(p=>m.instances.includes(p.instance))){
svg.append(el('rect',{x:x+(p.x+m.dx)*s,y:y+(p.y+m.dy)*s,width:p.width*s,height:p.height*s,fill:'var(--bad)','fill-opacity':.12,stroke:'var(--bad)','stroke-width':2,'stroke-dasharray':'5 3'}));
svg.append(el('text',{x:x+(p.x+m.dx+p.width/2)*s,y:y+(p.y+m.dy+p.height/2)*s,'text-anchor':'middle','font-size':Math.max(10,s*.6),style:'fill:var(--bad)'},(detail.labels?.[p.instance]??p.instance)+' proposed'));}
for(const p of m.failed_endpoints??[]){const a=xy(p);svg.append(el('circle',{cx:a.x,cy:a.y,r:s*.35,fill:'none',stroke:'var(--bad)','stroke-width':3}));}}
document.getElementById('status').textContent=w.placements.length+' facilities · '+w.logistics_components.length+' components · '+used_bounds.width+' × '+used_bounds.height+' used bounds · '+(overload?'SCALED CAPACITY VIOLATION':'independent validator '+(validation.covered_geometry_validated?'PASS':'REJECTED')+(detail.accepted===false?' · planner REJECTED':''));
document.getElementById('explanation').textContent=overload?'RED: single-lane capacity exceeded when rates are multiplied by four. This is the retained geometry, not proof that the recipe request is impossible.':'Actual unit-grid paths. Independent validation checks recipe rates, facility ports, component arms, capacity and reachability.';
if(detail.move_diagnostic&&!overload){document.getElementById('status').textContent+=' · proposed move REJECTED';document.getElementById('explanation').textContent='RED DASHED: proposed facility positions, not committed geometry. RED RINGS: failed route endpoints. '+detail.move_diagnostic.code+'. The solid geometry is the unchanged validated factory.';}
document.getElementById('scope').textContent='Pending recipe terminals: '+validation.recipe_terminals.filter(t=>value(t.pending_rate)>0).length+'. '+(detail.case==='actual-graph-cycle-block'?'Actual graph cycle: internal return flow is closed; outgoing graph products remain pending. ':detail.case==='actual-graph-block-composition'?'Actual composed graph: new frontier connections are validated; block interiors are preserved; remaining graph cuts stay pending. ':detail.case==='actual-graph-factory-closure'?'Actual graph closure: retained blocks, mixed-transport head and original boundary obligations. ':'')+(detail.factory_validated&&validation.covered_geometry_validated&&validation.recipe_coverage_complete?'Complete original graph and external margins independently checked. No minimum-area claim.':'This is not the complete heavy-xiranite production graph.');}
document.getElementById('fit').onclick=()=>{view=[...fullView];applyView();};
svg.style.touchAction='none';svg.style.cursor='grab';
svg.addEventListener('wheel',e=>{e.preventDefault();const p=new DOMPoint(e.clientX,e.clientY).matrixTransform(svg.getScreenCTM().inverse()),k=e.deltaY>0?1.15:1/1.15;
if(view[2]*k<10||view[2]*k>fullView[2]*8)return;view=[p.x+(view[0]-p.x)*k,p.y+(view[1]-p.y)*k,view[2]*k,view[3]*k];applyView();},{passive:false});
svg.onpointerdown=e=>{pan={id:e.pointerId,x:e.clientX,y:e.clientY,matrix:svg.getScreenCTM().inverse(),view:[...view]};svg.setPointerCapture(e.pointerId);};
svg.onpointermove=e=>{if(!pan||pan.id!==e.pointerId)return;const a=new DOMPoint(pan.x,pan.y).matrixTransform(pan.matrix),b=new DOMPoint(e.clientX,e.clientY).matrixTransform(pan.matrix);view=[pan.view[0]+a.x-b.x,pan.view[1]+a.y-b.y,pan.view[2],pan.view[3]];applyView();};
svg.onpointerup=svg.onpointercancel=()=>{pan=null;};
document.getElementById('normal').onclick=()=>{overload=false;draw();};document.getElementById('overload').onclick=()=>{overload=true;draw();};new ResizeObserver(draw).observe(svg);draw();
</script></html>`;
}
