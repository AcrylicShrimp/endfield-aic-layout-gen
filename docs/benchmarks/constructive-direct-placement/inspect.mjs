// Export the actual retained state and last rejected obstruction, never an invented route.
import fs from 'node:fs';
import { createHash } from 'node:crypto';
const [input, evidencePath, htmlPath] = process.argv.slice(2);
if (process.argv.length !== 5) throw new Error('Usage: node inspect.mjs REPORT NEW_EVIDENCE_JSON NEW_HTML');
const bytes = fs.readFileSync(input), r = JSON.parse(bytes);
const o = r.last_obstruction;
if (!r.final_node || !o?.blocked_requirements.length) throw new Error('Expected a retained state and rejected obstruction');
if (r.complete || r.required_facilities !== 59 || r.final_node.member_instances.length !== 18
  || o.candidate.member_instances.length !== 19 || r.failed_statistics.astar_searches !== 240
  || r.failed_statistics.completed_lane_bundles !== 238) throw new Error('Measured regression counts changed');
const requirement = o.blocked_requirements[0];
const portIn = n => {
  const b = n.boundary_requirements.find(b => b.requirement === requirement);
  if (b?.attachments.length !== 1 || b.attachments[0].kind !== 'new-port') throw new Error('Expected one pending new-port endpoint');
  return b.attachments[0].port;
};
const occupied = (n, x, y) => ({
  facilities: n.placements.filter(p => x >= p.x && x < p.x + p.width && y >= p.y && y < p.y + p.height)
    .map(p => ({ instance: p.instance, facility: p.facility })),
  networks: n.transport_networks.filter(net => net.cells.some(c => c.x === x && c.y === y))
    .map(net => ({ item: net.item, transport: net.transport,
      segments: net.segments.filter(s => s.from.x === x && s.from.y === y || s.to.x === x && s.to.y === y) })),
});
const neighbors = n => {
  const p = portIn(n).connection;
  return Object.fromEntries([['west', -1, 0], ['east', 1, 0], ['north', 0, -1], ['south', 0, 1]]
    .map(([name, dx, dy]) => [name, { cell: [p.x + dx, p.y + dy], ...occupied(n, p.x + dx, p.y + dy) }]));
};
const before = neighbors(r.final_node), after = neighbors(o.candidate);
// This measured local regression has one remaining approach, closed by a new belt corner.
if (before.north.facilities.length || before.north.networks.some(n => n.transport === 'belt')
  || !after.north.networks.some(n => n.transport === 'belt')
  || !after.west.facilities.length || !after.east.facilities.length
  || !after.south.networks.some(n => n.transport === 'belt')) throw new Error('Measured obstruction geometry changed');
const evidence = { schema_version: 1, stage: 'offline-direct-placement-obstruction',
  input, input_sha256: createHash('sha256').update(bytes).digest('hex'), requirement,
  required_facilities: r.required_facilities, complete: r.complete,
  retained_node: r.final_node, rejected_obstruction: o, before_neighbors: before, after_neighbors: after,
  failed_statistics: r.failed_statistics,
  interpretation: 'Last rejected trial only. A new belt corner closes the north approach of an unfinished input; other local approaches are occupied. The constructor rejects this trial and retains the prior node. Not a global infeasibility proof.' };
fs.writeFileSync(evidencePath, JSON.stringify(evidence, null, 2) + '\n', { flag: 'wx' });
const esc = s => String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const relativeCells = n => {
  const p = portIn(n).connection;
  return new Set(n.transport_networks.flatMap(net => net.cells.map(c => `${net.transport}:${net.item}:${c.x - p.x}:${c.y - p.y}`)));
};
const oldCells = relativeCells(r.final_node);
function panel(n, rejected) {
  const port = portIn(n), cx = port.connection.x, cy = port.connection.y;
  const count = 9, cell = 48, margin = 32, size = count * cell, x0 = cx - 4, y0 = cy - 4;
  const px = x => margin + (x - x0) * cell, py = y => margin + (y - y0) * cell;
  const xy = p => `${px(p.x) + cell / 2},${py(p.y) + cell / 2}`;
  const clip = rejected ? 'after-clip' : 'before-clip';
  let svg = `<svg viewBox="0 0 ${size + margin * 2} ${size + margin * 2}" role="img" aria-label="${rejected ? 'Rejected' : 'Retained'} input neighborhood at ${cx},${cy}"><defs><clipPath id="${clip}"><rect x="${margin}" y="${margin}" width="${size}" height="${size}"/></clipPath></defs>`;
  for (let i = 0; i <= count; i++) {
    svg += `<path class="grid" d="M${margin + i * cell},${margin}v${size} M${margin},${margin + i * cell}h${size}"/>`;
    if (i < count) svg += `<text x="${margin + (i + .5) * cell}" y="21" text-anchor="middle">${x0 + i}</text><text x="25" y="${margin + (i + .5) * cell + 5}" text-anchor="end">${y0 + i}</text>`;
  }
  svg += `<g clip-path="url(#${clip})">`;
  for (const net of n.transport_networks) {
    const changed = p => !oldCells.has(`${net.transport}:${net.item}:${p.x - cx}:${p.y - cy}`);
    for (const p of net.cells) svg += `<circle class="route ${net.transport} ${changed(p) ? 'new-route' : ''}" cx="${px(p.x) + cell / 2}" cy="${py(p.y) + cell / 2}" r="1"><title>${esc(net.item)}</title></circle>`;
    for (const s of net.segments) {
      const added = [s.from, s.to].some(changed);
      svg += `<path class="route ${net.transport} ${added ? 'new-route' : ''}" d="M${xy(s.from)} L${xy(s.to)}"><title>${esc(net.item)}${added ? ' · added in rejected trial' : ''}</title></path>`;
    }
  }
  for (const p of n.placements) {
    if (p.x + p.width <= x0 || p.x >= x0 + count || p.y + p.height <= y0 || p.y >= y0 + count) continue;
    svg += `<rect class="facility ${p.instance === port.instance ? 'selected' : ''}" x="${px(p.x)}" y="${py(p.y)}" width="${p.width * cell}" height="${p.height * cell}"/>`;
    const name = p.facility.replace(/-\d+-mode-.*/, '').replaceAll('-', ' ');
    const tx = (Math.max(x0, p.x) + Math.min(x0 + count, p.x + p.width)) / 2;
    const ty = (Math.max(y0, p.y) + Math.min(y0 + count, p.y + p.height)) / 2;
    svg += `<text class="facility-name" x="${px(tx)}" y="${py(ty)}" text-anchor="middle">${esc(name)}</text>`;
  }
  svg += `<rect class="endpoint" x="${px(cx)}" y="${py(cy)}" width="${cell}" height="${cell}"/><text class="mark" x="${px(cx) + cell / 2}" y="${py(cy) + cell / 2 + 7}" text-anchor="middle">${rejected ? 'X' : 'P'}</text></g></svg>`;
  return `<section id="${rejected ? 'after' : 'before'}" ${rejected ? '' : 'hidden'}><h2>${rejected ? 'Rejected trial · 19 facilities, not committed' : 'Before · retained 18-facility partial layout'}</h2>${svg}<p>Unfinished input <strong>(${cx}, ${cy})</strong> · ${esc(port.port)}</p></section>`;
}
const html = `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Connection-first placement: remaining input blocked</title>
<style>:root{color-scheme:light dark;--bg:light-dark(#fafafa,#14181d);--fg:light-dark(#19232c,#e4ebf1);--line:light-dark(#ccd4dc,#38434e);--block:light-dark(#e5eaf0,#26313e);--blue:light-dark(#1668ae,#7dbbff);--orange:light-dark(#ad5706,#ffc081);--red:light-dark(#b32932,#ff8f98);--belt:light-dark(#5b6876,#9daabd);--pipe:light-dark(#348164,#76bfa3)}*{box-sizing:border-box}body{margin:0;padding:24px;background:var(--bg);color:var(--fg);font:16px/1.5 system-ui}main{max-width:780px;margin:auto}h1{font-size:24px;margin:0}h2{font-size:19px;margin:18px 0 4px}p{margin:10px 0}a{color:var(--blue)}nav{display:flex;gap:8px;flex-wrap:wrap;margin:18px 0}button{font:inherit;color:inherit;background:transparent;border:1px solid var(--line);border-radius:5px;padding:9px 13px;cursor:pointer}button[aria-pressed=true]{border-color:var(--blue);box-shadow:inset 0 -3px var(--blue)}svg{display:block;width:100%;max-width:640px;height:auto;margin:auto}svg text{fill:var(--fg);font:14px system-ui}.grid{stroke:var(--line);stroke-width:1;fill:none}.route{fill:none;stroke:var(--belt);stroke-width:6;stroke-linecap:round}.pipe{stroke:var(--pipe);stroke-dasharray:6 3}.new-route{stroke:var(--orange);stroke-width:8}.facility{fill:var(--block);stroke:var(--fg);stroke-width:1.5}.selected{stroke:var(--blue);stroke-width:2}.endpoint{fill:none;stroke:var(--red);stroke-width:3}svg .mark{font-size:22px;font-weight:600}.legend{display:flex;gap:16px;flex-wrap:wrap}.legend span:before{content:'';display:inline-block;width:14px;height:14px;margin-right:6px;background:var(--belt)}.legend .new:before{background:var(--orange)}.legend .pipe-key:before{background:var(--pipe)}details{margin-top:16px;font-size:14px}code{overflow-wrap:anywhere}footer{margin-top:24px;border-top:1px solid var(--line);padding-top:14px}@media(max-width:480px){body{padding:14px}svg text{font-size:17px}svg .facility-name{font-size:16px}}</style>
<main><h1>Nearby placement, unfinished input trapped</h1><p>Flow → east. Last rejected trial shown; the retained factory remains partial at 18 / 59.</p>
<nav aria-label="Compare construction states"><button type="button" aria-pressed="false" data-state="before">Before · retained</button><button type="button" aria-pressed="true" data-state="after">After · rejected</button></nav>
${panel(r.final_node, false)}${panel(o.candidate, true)}
<div class="legend"><span>Existing belt</span><span class="new">New rejected belt</span><span class="pipe-key">Existing pipe</span></div>
<p>The input is flanked by facilities. An existing belt occupies the cell below. The new belt turns immediately above the input, closing the remaining approach. The constructor rejects this route rather than committing the obstruction.</p>
<p>This is the last recorded counterexample, not an explanation of every rejected candidate or proof that no layout exists. The 3-cell reservation protects the input and cells to its west; it does not preserve the north approach shown here.</p>
<details><summary>Requirement and measured failure</summary><code>${esc(requirement)}</code><p>Final transaction: 240 A* calls, 238 completed lane bundles rejected by remaining-boundary checks. Exact coordinates and networks are taken from the stored report.</p></details>
<footer><a href="direct-east-partial.html">Open full 18-facility layout and construction history →</a><p><a href="fixed-east-coarse-partial.html">Earlier fixed-orientation result · 54 / 59</a> · <a href="heavy-xiranite.html">Complete baseline · 59 / 59</a></p></footer></main>
<script>document.querySelectorAll('[data-state]').forEach(button=>button.addEventListener('click',()=>{document.querySelectorAll('[data-state]').forEach(b=>b.setAttribute('aria-pressed',String(b===button)));for(const id of ['before','after'])document.getElementById(id).hidden=id!==button.dataset.state;}));</script></html>`;
fs.writeFileSync(htmlPath, html, { flag: 'wx' });
console.log(JSON.stringify({ success: true, evidence: evidencePath, visualization: htmlPath }));
