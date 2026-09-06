import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { reconstruct } from './model.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
const read = p => JSON.parse(fs.readFileSync(p));
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
try {
  const args = process.argv.slice(2), options = {}, keys = ['case', 'source-image', 'recipes', 'facilities', 'items', 'transports', 'components', 'buildings', 'validator', 'output-directory'];
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const provenance = keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) }));
  const detail = reconstruct(read(options.case), catalogs, read(options.buildings)), output = path.resolve(options['output-directory']);
  fs.mkdirSync(output); write(path.join(output, 'witness.json'), detail.witness);
  const checked = await executeValidator(path.resolve(options.validator), ['--witness', path.join(output, 'witness.json'),
    ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(output, 'validation.json')], path.join(output, 'validator.log'));
  write(path.join(output, 'validator-process.json'), checked);
  const validation = read(path.join(output, 'validation.json'));
  detail.accepted = checked.code === 0 && validation.covered_geometry_validated;
  write(path.join(output, 'detail.json'), detail);
  // Presentation only: keep power equipment outside the production witness and validation claim.
  const annotationScript = `<script>
const originalDraw = draw;
draw = () => {
  originalDraw();
  for (const a of detail.reconstruction.annotations) {
    const {s,x,y}=layout;
    svg.append(el('rect',{x:x+a.x*s,y:y+a.y*s,width:a.width*s,height:a.height*s,fill:'var(--bg)',stroke:'var(--other)','stroke-dasharray':'4 3','data-power-annotation':a.id}));
    svg.append(el('text',{x:x+(a.x+a.width/2)*s,y:y+(a.y+a.height/2)*s,'text-anchor':'middle','font-size':12},a.id+' power'));
  }
  document.getElementById('scope').textContent='Screenshot transcription, not optimization. Power shown as an inferred annotation; power supply is not validated. Raw-material inputs and battery export are not shown or invented. Rates assume full utilization; screenshot operation is unverified.';
}; draw();
</script>`;
  const html = render(detail, validation, catalogs).replace('<h1>Recipe-bound interconnect · actual catalog facilities</h1>', '<h1>Screenshot reconstruction · medium-capacity Valley battery</h1>').replace('</html>', annotationScript + '</html>');
  fs.writeFileSync(path.join(output, 'index.html'), html, { flag: 'wx' });
  for (const p of provenance) if (hash(p.path) !== p.sha256) throw new Error(`Input changed: ${p.path}`);
  const report = { schema_version: 1, status: detail.accepted ? 'visible-subgraph-validated' : 'reconstruction-rejected', factory_validated: false,
    output_directory: output, source_image_sha256: hash(options['source-image']), witness_sha256: hash(path.join(output, 'witness.json')),
    metrics: detail.reconstruction.metrics, routes: detail.reconstruction.routes.map(({ cells, ...r }) => r),
    pending_recipe_terminals: validation.recipe_terminals.filter(t => t.pending_rate.numerator > 0), provenance };
  write(path.join(output, 'report.json'), report);
  console.log(JSON.stringify({ status: report.status, metrics: report.metrics, pending_recipe_terminals: report.pending_recipe_terminals.length, output }));
  if (!detail.accepted) process.exitCode = 2;
} catch (e) {
  console.error(JSON.stringify({ code: e.code ?? 'reconstruction-validation', severity: 'error', stage: 'screenshot-reconstruction', message: e.message, entity: e.entity ?? null }));
  process.exitCode = 1;
}
