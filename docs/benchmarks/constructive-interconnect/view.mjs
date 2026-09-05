// Standalone repository diagnostic, not an in-conversation mockup.
export function render(report) {
  const safe = JSON.stringify(report).replaceAll('<', '\\u003c');
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Planned lane permutation</title><style>
:root{color-scheme:light dark;--bg:#f5f7fa;--fg:#152a36;--grid:#d9e1e8;--a:#996109;--b:#087e83;--module:#6f55aa}
@media(prefers-color-scheme:dark){:root{--bg:#07131d;--fg:#d3e8f1;--grid:#233a49;--a:#efb55b;--b:#62d6d1;--module:#b597f0}}
*{box-sizing:border-box}body{margin:0;padding:24px;background:var(--bg);color:var(--fg);font:16px system-ui}h1{font-size:24px;margin:0 0 10px}p{max-width:1000px;line-height:1.5;overflow-wrap:anywhere}button{font:inherit;padding:8px 12px;cursor:pointer}nav{display:flex;gap:8px;flex-wrap:wrap}svg{display:block;width:100%;max-height:760px}text{fill:var(--fg);font-family:ui-monospace,monospace}.key{display:flex;gap:16px;flex-wrap:wrap}.key span{overflow-wrap:anywhere}.key i{display:inline-block;width:20px;border-top:3px solid;vertical-align:middle}#status{font-weight:600} @media(max-width:480px){body{padding:12px}h1{font-size:20px}}
</style><h1>Planned lane permutation · typed channels stay separate</h1>
<p>Solid lines: actual unit-grid paths. Dashed boxes: reserved adjacent-swap modules. Squares: catalog bridges with independent perpendicular channels. Fanout/fanin and facility binding are not built here.</p>
<nav aria-label="Recipe interface case"><button id="minimum">Two consumers · small recipe</button><button id="maximum">Two consumers · catalog maximum belt input</button></nav>
<p id="status" aria-live="polite"></p><div class="key" id="legend"></div><svg id="diagram" role="img" aria-label="Planned grid routes and crossing modules"></svg><p id="detail"></p>
<script type="application/json" id="data">${safe}</script><script>
const report=JSON.parse(document.getElementById('data').textContent),svg=document.getElementById('diagram');
let selected='minimum';
const el=(name,attrs={},content='')=>{const e=document.createElementNS('http://www.w3.org/2000/svg',name);for(const[k,v]of Object.entries(attrs))e.setAttribute(k,v);e.textContent=content;return e;};
function draw(){const f=report.examples[selected],g=f.geometry;svg.replaceChildren();
const items=[...new Set(g.lanes.map(l=>l.item))],color=item=>'var(--'+(items.indexOf(item)%2?'b':'a')+')';
const scale=Math.min(38,(svg.clientWidth-100)/(g.width+2),680/(g.height+2)),left=50,top=22;
svg.setAttribute('viewBox','0 0 '+svg.clientWidth+' '+(g.height*scale+50));
const xy=p=>({x:left+(p.x+.5)*scale,y:top+(p.y+.5)*scale});
for(let x=0;x<=g.width;x++)svg.append(el('line',{x1:left+x*scale,y1:top,x2:left+x*scale,y2:top+g.height*scale,stroke:'var(--grid)','stroke-width':.5}));
for(let y=0;y<=g.height;y++)svg.append(el('line',{x1:left,y1:top+y*scale,x2:left+g.width*scale,y2:top+y*scale,stroke:'var(--grid)','stroke-width':.5}));
for(const m of g.modules)svg.append(el('rect',{x:left+m.x*scale,y:top+m.y*scale,width:m.width*scale,height:m.height*scale,fill:'none',stroke:'var(--module)','stroke-width':1,'stroke-dasharray':'5 4'}));
for(const l of g.lanes){svg.append(el('polyline',{points:l.cells.map(p=>{const a=xy(p);return a.x+','+a.y;}).join(' '),fill:'none',stroke:color(l.item),'stroke-width':Math.max(1.3,Math.min(4,scale*.14))}));
for(const [p,anchor,dx]of [[l.cells[0],'end',-12],[l.cells.at(-1),'start',12]]){const a=xy(p);svg.append(el('text',{x:a.x+dx,y:a.y+4,'text-anchor':anchor,'font-size':11},l.id));}
for(const p of [l.cells[0],l.cells.at(-1)]){const a=xy(p);svg.append(el('circle',{cx:a.x,cy:a.y,r:Math.max(2,scale*.13),fill:color(l.item)}));}}
for(const c of g.components){const a=xy(c.position),r=Math.max(3,scale*.25);svg.append(el('rect',{x:a.x-r,y:a.y-r,width:2*r,height:2*r,fill:'var(--bg)',stroke:'var(--module)','stroke-width':1.5}));
svg.append(el('line',{x1:a.x-r,y1:a.y,x2:a.x+r,y2:a.y,stroke:'var(--fg)','stroke-width':1}));
svg.append(el('path',{d:'M'+a.x+','+(a.y-r)+' L'+a.x+','+(a.y+r),stroke:'var(--fg)','stroke-width':1,'stroke-dasharray':'1 2'}));}
document.getElementById('status').textContent=g.lanes.length+' lanes · '+g.modules.length+' planned bridges · '+g.width+' × '+g.height+' reserved envelope · geometry checks PASS';
document.getElementById('detail').textContent=f.recipe+' × '+f.consumer_instances+' consumers. Source side grouped by item; destination side grouped by consumer. Rates preserved per lane. NOT a complete factory or a full N:M network.';
const legend=document.getElementById('legend');legend.replaceChildren();for(const item of items){const span=document.createElement('span'),mark=document.createElement('i');mark.style.borderColor=color(item);span.append(mark,' '+item);legend.append(span);}}
document.getElementById('minimum').onclick=()=>{selected='minimum';draw();};document.getElementById('maximum').onclick=()=>{selected='maximum';draw();};new ResizeObserver(draw).observe(svg);draw();
</script></html>`;
}
