export function render(detail, validation, catalogs) {
  const data = JSON.stringify({detail,validation,catalogs}).replaceAll('<','\\u003c');
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Recipe-bound interconnect</title>
<style>:root{color-scheme:light dark;--bg:#f6f8fb;--fg:#193341;--grid:#d8e1e9;--facility:#dbe9f1;--a:#946006;--b:#06787e;--other:#697c92;--component:#7251a8;--bad:#c92e45}
@media(prefers-color-scheme:dark){:root{--bg:#07131d;--fg:#d7eaf2;--grid:#203746;--facility:#183442;--a:#efb55b;--b:#62d6d1;--other:#93a4bb;--component:#c2a0ff;--bad:#ff6f85}}
*{box-sizing:border-box}body{margin:0;padding:24px;background:var(--bg);color:var(--fg);font:16px system-ui}h1{font-size:24px;margin:0 0 10px}p{line-height:1.5;max-width:1100px;overflow-wrap:anywhere}svg{display:block;width:100%;max-height:820px}text{fill:var(--fg);font-family:ui-monospace,monospace}.legend{display:flex;flex-wrap:wrap;gap:15px}.legend span{overflow-wrap:anywhere}button{font:inherit;padding:8px 12px;cursor:pointer}nav{display:flex;gap:10px;flex-wrap:wrap}@media(max-width:480px){body{padding:12px}h1{font-size:20px}}</style>
<h1>Recipe-bound interconnect · actual catalog facilities</h1><p id="status"></p><nav><button id="normal">Validated geometry</button><button id="overload">Same geometry at four times the rate</button></nav><p id="explanation" aria-live="polite"></p><div class="legend"><span id="itemA" style="color:var(--a)"></span><span id="itemB" style="color:var(--b)"></span><span>Muted: raw inputs and final output</span><span>S: splitter · M: converger · X: bridge</span></div>
<svg id="diagram" role="img" aria-label="Facility ports, typed networks and preplanned logistics components"></svg><p id="scope"></p>
<script type="application/json" id="data">${data}</script><script>
const {detail,validation,catalogs}=JSON.parse(document.getElementById('data').textContent),w=detail.witness,svg=document.getElementById('diagram');let overload=false;
const el=(name,attrs={},text='')=>{const e=document.createElementNS('http://www.w3.org/2000/svg',name);for(const[k,v]of Object.entries(attrs))e.setAttribute(k,v);e.textContent=text;return e;};
const value=r=>r.numerator/r.denominator,rateText=r=>r.denominator===1?String(r.numerator):r.numerator+'/'+r.denominator;
const sources=w.transport_networks.filter(n=>n.terminals.some(t=>t.endpoint.kind==='facility'&&t.direction==='output')&&n.terminals.some(t=>t.endpoint.kind==='facility'&&t.direction==='input')).map(n=>n.item);
document.getElementById('itemA').textContent='A: '+sources[0];document.getElementById('itemB').textContent='B: '+sources[1];
const color=item=>item===sources[0]?'var(--a)':item===sources[1]?'var(--b)':'var(--other)';
function draw(){svg.replaceChildren();const points=w.transport_networks.flatMap(n=>n.cells),width=Math.max(...w.placements.map(p=>p.x+p.width),...points.map(p=>p.x+1)),height=Math.max(...w.placements.map(p=>p.y+p.height),...points.map(p=>p.y+1));
const s=Math.min(33,(svg.clientWidth-20)/(width+1)),x=10,y=10,xy=p=>({x:x+(p.x+.5)*s,y:y+(p.y+.5)*s});svg.setAttribute('viewBox','0 0 '+svg.clientWidth+' '+(height*s+25));
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
document.getElementById('status').textContent=w.placements.length+' facilities · '+w.logistics_components.length+' components · '+width+' × '+height+' used bounds · '+(overload?'SCALED CAPACITY VIOLATION':'independent validator '+(validation.covered_geometry_validated?'PASS':'REJECTED'));
document.getElementById('explanation').textContent=overload?'RED: single-lane capacity exceeded when rates are multiplied by four. This is the retained geometry, not proof that the recipe request is impossible.':'Actual unit-grid paths. Independent validation checks recipe rates, facility ports, component arms, capacity and reachability.';
document.getElementById('scope').textContent='Pending recipe terminals: '+validation.recipe_terminals.filter(t=>value(t.pending_rate)>0).length+'. '+(detail.block?'Actual graph cycle: internal return flow is closed; outgoing graph products remain pending. ':'')+'This local fixture is not the complete heavy-xiranite production graph.';}
document.getElementById('normal').onclick=()=>{overload=false;draw();};document.getElementById('overload').onclick=()=>{overload=true;draw();};new ResizeObserver(draw).observe(svg);draw();
</script></html>`;
}
