import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { reconstruct, verifyReferenceMargins } from './model.mjs';
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
  for (const k of keys.filter(k => k !== 'buildings')) if (!options[k]) throw new Error(`Missing --${k}`);
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const provenance = keys.filter(k => k !== 'output-directory' && options[k]).map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) }));
  const spec = read(options.case);
  if (spec.annotations?.length && !options.buildings) throw new Error(`Case ${options.case} contains annotations; supply --buildings`);
  const detail = reconstruct(spec, catalogs, options.buildings ? read(options.buildings) : {}), output = path.resolve(options['output-directory']);
  fs.mkdirSync(output); write(path.join(output, 'witness.json'), detail.witness);
  const checked = await executeValidator(path.resolve(options.validator), ['--witness', path.join(output, 'witness.json'),
    ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(output, 'validation.json')], path.join(output, 'validator.log'));
  write(path.join(output, 'validator-process.json'), checked);
  const validation = read(path.join(output, 'validation.json'));
  let marginCheck;
  try { marginCheck = verifyReferenceMargins(spec, validation); }
  catch (e) { marginCheck = { status: 'reference-margins-rejected', diagnostic: { code: e.code, severity: 'error', stage: 'screenshot-reconstruction', message: e.message, entity: e.entity } }; }
  detail.accepted = checked.code === 0 && marginCheck.status === 'internal-reference-validated';
  detail.reference_margin_check = marginCheck;
  write(path.join(output, 'detail.json'), detail);
  // Presentation only: keep power equipment outside the production witness and validation claim.
  const scriptJson = value => JSON.stringify(value).replaceAll('<', '\\u003c');
  const annotationScript = `<script>
document.querySelector('h1').textContent=${scriptJson(`Screenshot reference · ${spec.id}`)};
document.title=${scriptJson(`Screenshot reference · ${spec.id}`)};
document.getElementById('normal').remove();
document.getElementById('overload').remove();
const originalDraw = draw;
draw = () => {
  originalDraw();
  document.getElementById('status').textContent=${scriptJson(`${detail.witness.placements.length} facilities · ${detail.reconstruction.metrics.bounds.width} × ${detail.reconstruction.metrics.bounds.height} used bounds · covered geometry ${validation.covered_geometry_validated ? 'PASS' : 'FAIL'} · reference margins ${detail.accepted ? 'PASS' : 'FAIL'} · not a complete factory`)};
  for (const a of detail.reconstruction.annotations) {
    const {s,x,y}=layout;
    svg.append(el('rect',{x:x+a.x*s,y:y+a.y*s,width:a.width*s,height:a.height*s,fill:'var(--bg)',stroke:'var(--other)','stroke-dasharray':'4 3','data-power-annotation':a.id}));
    svg.append(el('text',{x:x+(a.x+a.width/2)*s,y:y+(a.y+a.height/2)*s,'text-anchor':'middle','font-size':12},a.id+' power'));
  }
  document.getElementById('scope').textContent=${scriptJson(`Screenshot transcription, not optimization or a complete factory. ${spec.rate_assumptions ?? 'Rates are declared assumptions.'} Unobserved: ${(spec.unobserved ?? []).join('; ')}. Any power annotation is outside validation.`)};
}; draw();
</script>`;
  const html = render(detail, validation, catalogs).replace('</html>', annotationScript + '</html>');
  fs.writeFileSync(path.join(output, 'index.html'), html, { flag: 'wx' });
  for (const p of provenance) if (hash(p.path) !== p.sha256) throw new Error(`Input changed: ${p.path}`);
  const report = { schema_version: 2, status: detail.accepted ? 'internal-reference-validated' : 'reconstruction-rejected', factory_validated: false,
    output_directory: output, source_image_sha256: hash(options['source-image']), witness_sha256: hash(path.join(output, 'witness.json')),
    metrics: detail.reconstruction.metrics, routes: detail.reconstruction.routes.map(({ cells, ...r }) => r),
    reference_margin_check: marginCheck, pending_recipe_terminals: validation.recipe_terminals.filter(t => t.pending_rate.numerator > 0), provenance };
  write(path.join(output, 'report.json'), report);
  console.log(JSON.stringify({ status: report.status, metrics: report.metrics, pending_recipe_terminals: report.pending_recipe_terminals.length, output }));
  if (!detail.accepted) process.exitCode = 2;
} catch (e) {
  console.error(JSON.stringify({ code: e.code ?? 'reconstruction-validation', severity: 'error', stage: 'screenshot-reconstruction', message: e.message, entity: e.entity ?? null }));
  process.exitCode = 1;
}
