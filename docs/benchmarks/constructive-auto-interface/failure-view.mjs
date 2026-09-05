// This is a logical diagnostic, not a generated physical layout or routing witness.
export function renderFailure(failure) {
  const data = JSON.stringify(failure).replaceAll('<', '\\u003c');
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Automatic interface: port pressure</title><style>
:root{color-scheme:light dark;--bg:#f5f8fa;--fg:#123442;--box:#dbe9f0;--bad:#b42646;--a:#95640c;--b:#147b83}
@media(prefers-color-scheme:dark){:root{--bg:#07131d;--fg:#d7eaf2;--box:#183442;--bad:#ff8299;--a:#efb55b;--b:#62d6d1}}
*{box-sizing:border-box}body{margin:0;padding:24px;background:var(--bg);color:var(--fg);font:16px system-ui}h1{font-size:24px}p{max-width:1050px;line-height:1.5;overflow-wrap:anywhere}svg{display:block;width:100%;max-width:1120px}text{fill:var(--fg);font-family:ui-monospace,monospace}.bad{fill:var(--bad)}
</style><h1>Next limit: exclusive port-per-pair assignment</h1><p id="status"></p>
<p>Logical demand only — dashed edges are NOT belt routes. There is no physical candidate at this failure stage.</p>
<svg id="diagram" role="img" aria-label="Logical supplier demands and catalog-facing port counts"></svg>
<p>Exclusive port-per-pair binding is the restriction being tested. Capacity-aware split/merge trees can share ports among compatible demands, subject to aggregate capacity; crossing modules alone cannot do that.</p>
<p>No global impossibility is claimed. No facility placement or valid factory is shown.</p>
<script type="application/json" id="data">${data}</script><script>
const failure=JSON.parse(document.getElementById('data').textContent),e=failure.evidence,svg=document.getElementById('diagram');
svg.setAttribute('viewBox','0 0 1120 '+(Math.max(e.producer_order.length*270,e.consumer_order.length*135)+40));
document.getElementById('status').textContent=failure.code+' · '+e.order_trials+' bank-order trials · geometry not constructed';
const el=(name,attrs={},text='')=>{const n=document.createElementNS('http://www.w3.org/2000/svg',name);for(const[k,v]of Object.entries(attrs))n.setAttribute(k,v);n.textContent=text;return n;};
const rate=r=>r.numerator+'/'+r.denominator+' per second',color=id=>e.producer_order.indexOf(id)%2?'var(--b)':'var(--a)';
const sourceY=id=>65+e.producer_order.indexOf(id)*270,targetY=id=>25+e.consumer_order.indexOf(id)*135;
for(const l of e.lanes){const y=targetY(l.target)+30+e.producer_order.indexOf(l.source)*32;svg.append(el('line',{x1:340,y1:sourceY(l.source)+70,x2:780,y2:y,stroke:color(l.source),'stroke-width':2,'stroke-dasharray':'8 6'}));}
for(const id of e.producer_order){const n=e.instances.find(n=>n.id===id),p={count:e.lanes.filter(l=>l.source===id).length,available_ports:n.facing_output_ports},y=sourceY(id);
svg.append(el('rect',{x:10,y,width:330,height:190,rx:8,fill:'var(--box)',stroke:color(id)}));
for(const [i,t]of [id+' · '+n.facility.split('-')[0],'Supply: '+rate(n.outputs[0].rate),p.count+' pair lanes / '+p.available_ports+' ports','Line capacity: '+rate(e.lane_capacity)].entries())svg.append(el('text',{x:25,y:y+30+i*35,'font-size':i===0?22:16,class:i===2&&p.count>p.available_ports?'bad':''},t));
for(let i=0;i<p.available_ports;i++)svg.append(el('circle',{cx:40+i*35,cy:y+166,r:7,fill:color(id)}));}
for(const id of e.consumer_order){const n=e.instances.find(n=>n.id===id),y=targetY(id);svg.append(el('rect',{x:780,y,width:320,height:113,rx:8,fill:'var(--box)',stroke:'var(--fg)'}));
svg.append(el('text',{x:800,y:y+25,'font-size':20},id+' · '+n.facility.split('-')[0]));
for(const [i,l]of e.lanes.filter(l=>l.target===id).entries())svg.append(el('text',{x:800,y:y+55+i*27,'font-size':16},l.source+': '+rate(l.rate)));}
</script></html>`;
}
