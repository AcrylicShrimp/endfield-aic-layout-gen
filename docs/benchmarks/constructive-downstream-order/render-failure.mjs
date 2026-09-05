// Render exact retained geometry into a standalone diagnostic, without rerunning construction.
import fs from 'node:fs';
import { reservationConflicts } from './reservations.mjs';
const [coarsePath, nearbyPath, output] = process.argv.slice(2);
if (process.argv.length !== 5) throw new Error('Usage: node render-failure.mjs COARSE_REPORT NEARBY_REPORT NEW_OUTPUT_HTML');
const escape = s => String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const coarse = JSON.parse(fs.readFileSync(coarsePath));
const nearby = JSON.parse(fs.readFileSync(nearbyPath));
// These IDs select the measured regression, not facility/recipe data or production rules.
const cases = [
  { report: coarse, edge: 'wiring-edge:21c5b34ce05a1a6145f4451f9000d03a9184f3301407bb9ecb356b9339b77043',
    title: 'Coarse: shared input connection cell', known: true, link: 'closed-coarse-partial.html' },
  { report: nearby, edge: 'wiring-edge:05e089285ba40fa7965113fad2d6e3c58db56ee47bade60deef31023d6c6a1e8',
    title: 'Nearby: failed input, cause unresolved', known: false, link: 'closed-nearby-4-partial.html' },
];
function panel(data, index) {
  const n = data.report.final_node;
  const b = n.boundary_requirements.find(b => b.requirement === data.edge);
  if (!b || b.attachments.length !== 1 || b.attachments[0].kind !== 'new-port') throw new Error('Regression endpoint changed');
  const port = b.attachments[0].port;
  const conflicts = reservationConflicts(n, 3).find(c => c.requirements.includes(data.edge));
  if (data.known && !conflicts) throw new Error('Expected recorded endpoint conflict');
  const cx = port.connection.x, cy = port.connection.y;
  const x0 = cx - 4, y0 = cy - 4, count = 9, cell = 48, margin = 32, size = count * cell;
  const px = x => margin + (x - x0) * cell, py = y => margin + (y - y0) * cell;
  const xy = p => `${px(p.x) + cell / 2},${py(p.y) + cell / 2}`;
  const rect = (x, y, w, h, attrs) => `<rect x="${px(x)}" y="${py(y)}" width="${w * cell}" height="${h * cell}" ${attrs}/>`;
  const clip = `clip-${index}`;
  let drawing = `<svg viewBox="0 0 ${size + margin * 2} ${size + margin * 2}" role="img" aria-label="${escape(data.title)}; input at ${cx},${cy}">
    <defs><clipPath id="${clip}"><rect x="${margin}" y="${margin}" width="${size}" height="${size}"/></clipPath>
    <marker id="arrow-${index}" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto"><path d="M0,0 L7,3.5 L0,7" fill="context-stroke"/></marker></defs>`;
  for (let i = 0; i <= count; i++) {
    drawing += `<path d="M${margin + i * cell},${margin}v${size} M${margin},${margin + i * cell}h${size}" class="grid"/>`;
    if (i < count) drawing += `<text x="${margin + (i + .5) * cell}" y="20" text-anchor="middle">${x0 + i}</text><text x="24" y="${margin + (i + .5) * cell + 4}" text-anchor="end">${y0 + i}</text>`;
  }
  drawing += `<g clip-path="url(#${clip})">`;
  for (const net of n.transport_networks) {
    for (const s of net.segments) {
      const overlaps = Math.max(s.from.x, s.to.x) >= x0 && Math.min(s.from.x, s.to.x) < x0 + count
        && Math.max(s.from.y, s.to.y) >= y0 && Math.min(s.from.y, s.to.y) < y0 + count;
      if (overlaps) drawing += `<path d="M${xy(s.from)} L${xy(s.to)}" class="${net.transport}"/><title>${escape(net.item)}</title>`;
    }
  }
  for (const p of n.placements) {
    if (p.x + p.width <= x0 || p.x >= x0 + count || p.y + p.height <= y0 || p.y >= y0 + count) continue;
    const selected = p.instance === port.instance;
    drawing += rect(p.x, p.y, p.width, p.height, `class="facility ${selected ? 'selected-facility' : ''}"`);
    const tx = Math.min(x0 + count - 1, Math.max(x0 + 1, p.x + p.width / 2));
    const ty = Math.min(y0 + count - .5, Math.max(y0 + .5, p.y + p.height / 2));
    drawing += `<text x="${px(tx)}" y="${py(ty)}" text-anchor="middle">${escape(p.facility.replace(/-\d+-mode-.*/, ''))}</text>`;
  }
  const corridor = (p, kind) => {
    for (let d = 0; d < 3; d++) {
      drawing += rect(p.connection.x + d * (p.connection.x - p.position.x),
        p.connection.y + d * (p.connection.y - p.position.y), 1, 1, `class="reservation ${kind}"`);
    }
    drawing += `<path d="M${xy(p.position)} L${xy(p.connection)}" class="normal ${kind}" marker-end="url(#arrow-${index})"/>`;
  };
  corridor(port, 'chosen');
  for (const other of conflicts?.reservation_owners ?? []) corridor(other.port, 'other');
  for (const c of n.logistics_components) {
    if (c.position.x >= x0 && c.position.x < x0 + count && c.position.y >= y0 && c.position.y < y0 + count)
      drawing += `<circle cx="${px(c.position.x) + cell / 2}" cy="${py(c.position.y) + cell / 2}" r="8" class="component"/>`;
  }
  drawing += rect(cx, cy, 1, 1, `class="endpoint ${data.known ? 'conflict' : ''}"`);
  const ex = px(cx) + cell / 2, ey = py(cy) + cell / 2;
  drawing += `<text x="${ex}" y="${ey + 6}" text-anchor="middle" class="endpoint-mark">${data.known ? 'X' : '?'}</text></g></svg>`;
  const explanation = data.known
    ? 'Both facility inputs point to the same cell. The other port keeps this cell reserved, so the current router rejects this goal before exploring a path. This is evidence about the current constructor, not a global game-infeasibility proof.'
    : 'The last group records 190 failed route calls. This retained input has no overlapping endpoint reservation. Nearby belts are shown for context; the exact candidate-specific obstruction has not yet been captured.';
  return `<section id="case-${index}" ${index ? 'hidden' : ''} aria-labelledby="tab-${index}">
    <h2>${escape(data.title)}</h2><p class="status">${n.member_instances.length} / ${data.report.required_facilities} facilities · PARTIAL · ${n.bounds.width} × ${n.bounds.height}</p>
    ${drawing}<div class="legend"><span class="key chosen">Selected input + 3-cell reservation</span>${data.known ? '<span class="key other">Other input + reservation</span>' : '<span class="key belt">Existing belt</span><span class="key pipe">Existing pipe</span>'}</div>
    <p><strong>Connection cell (${cx}, ${cy})</strong> · ${escape(port.edge)}-facing ${escape(port.port)}</p>
    <p>${explanation}</p><a href="${data.link}">Open full retained layout and construction history →</a>
    <details><summary>Original requirement and provenance</summary><code>${escape(data.edge)}</code><p>Exact retained coordinates, including facility rotations. Blue/orange arrows point outward from ports; they are port normals, not material-flow arrows. Reservations are constructor policy, not placed belts.</p></details>
    </section>`;
}
const html = `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Downstream growth: input obstruction</title>
<style>
:root{color-scheme:light dark;--bg:light-dark(#fafafa,#14181d);--fg:light-dark(#19232c,#e4ebf1);--line:light-dark(#ccd4dc,#38434e);--block:light-dark(#e5eaf0,#26313e);--blue:light-dark(#1668ae,#7dbbff);--orange:light-dark(#ad5706,#ffc081);--red:light-dark(#b32932,#ff8f98);--belt:light-dark(#5b6876,#9daabd);--pipe:light-dark(#348164,#76bfa3)}
*{box-sizing:border-box}body{margin:0;padding:24px;background:var(--bg);color:var(--fg);font:16px/1.5 system-ui,sans-serif}main{max-width:780px;margin:auto}h1{font-size:24px;margin:0 0 6px}h2{font-size:19px;margin:22px 0 4px}p{margin:10px 0}a{color:var(--blue)}nav{display:flex;gap:8px;flex-wrap:wrap;margin-top:18px}button{font:inherit;color:inherit;background:transparent;border:1px solid var(--line);border-radius:5px;padding:9px 13px;cursor:pointer}button[aria-selected=true]{border-color:var(--blue);box-shadow:inset 0 -3px var(--blue)}svg{display:block;width:100%;max-width:640px;height:auto;margin:auto}svg text{fill:var(--fg);font:14px system-ui}.grid{stroke:var(--line);stroke-width:1;fill:none}.facility{fill:var(--block);stroke:var(--fg);stroke-width:1.5}.selected-facility{stroke:var(--blue);stroke-width:2}.belt,.pipe{fill:none;stroke:var(--belt);stroke-width:5;stroke-linecap:round}.pipe{stroke:var(--pipe);stroke-dasharray:6 3}.chosen{--ink:var(--blue)}.other{--ink:var(--orange)}.reservation{fill:var(--ink);fill-opacity:.16;stroke:var(--ink);stroke-width:2;stroke-dasharray:5 3}.normal{fill:none;stroke:var(--ink);stroke-width:3}.endpoint{fill:none;stroke:var(--blue);stroke-width:3}.conflict{stroke:var(--red)}svg .endpoint-mark{font-size:22px;font-weight:600}.component{fill:var(--bg);stroke:var(--belt);stroke-width:2}.legend{display:flex;gap:16px;flex-wrap:wrap}.key:before{content:'';display:inline-block;width:14px;height:14px;background:var(--ink,var(--belt));margin-right:6px;vertical-align:middle}.key.pipe{--ink:var(--pipe)}.status{font-variant-numeric:tabular-nums}details{margin-top:20px;font-size:14px}code{overflow-wrap:anywhere}footer{margin-top:28px;border-top:1px solid var(--line);padding-top:14px;font-size:14px}@media(max-width:480px){body{padding:14px}svg text{font-size:17px}svg .endpoint-mark{font-size:24px}}
</style><main><h1>Downstream growth: input obstruction</h1><p>Internal outputs closed at each commit. External connections still pending.</p>
<nav aria-label="Failure case"><button id="tab-0" aria-controls="case-0" aria-selected="true">Coarse · confirmed conflict</button><button id="tab-1" aria-controls="case-1" aria-selected="false">Nearby · unresolved route</button></nav>
${cases.map(panel).join('\n')}<footer><a href="heavy-xiranite.html">Complete baseline · 59 / 59</a> · <a href="nearby-4-partial.html">Previous nearby result · 27 / 59</a></footer></main>
<script>const buttons=[...document.querySelectorAll('nav button')];buttons.forEach((button,index)=>button.addEventListener('click',()=>{buttons.forEach((b,i)=>{b.setAttribute('aria-selected',String(i===index));document.getElementById('case-'+i).hidden=i!==index;});}));</script></html>`;
fs.writeFileSync(output, html, { flag: 'wx' });
console.log(JSON.stringify({ success: true, output, bytes: Buffer.byteLength(html) }));
