// Bounded public schema inspection, not a corpus scraper or community-content redistribution tool.
import fs from 'node:fs';
import path from 'node:path';
import { args, diagnostic, digest, hash, read, write } from './environment.mjs';

try {
  const options = args(['manifest', 'output-directory']), plan = read(options.manifest), output = options['output-directory'];
  if (plan.schema_version !== 1 || plan.industrial.maximum_documents > 8 || plan.endfieldtools.maximum_documents > 12 || plan.timeout_ms > 10000 || plan.max_response_bytes > 5000000) throw new Error('Source-inspection cap exceeded');
  fs.mkdirSync(output); const requests = [], results = {};
  async function get(url) {
    const start = performance.now(), response = await fetch(url, { signal: AbortSignal.timeout(plan.timeout_ms) });
    const chunks = []; let bytes = 0;
    for await (const chunk of response.body) {
      bytes += chunk.byteLength; if (bytes > plan.max_response_bytes) throw new Error(`Source size guard exceeded: ${url}`); chunks.push(chunk);
    }
    const raw = Buffer.concat(chunks).toString('utf8'), filename = `response-${requests.length}.json`;
    const evidence = { url, final_url: response.url, status: response.status, bytes, elapsed_ms: performance.now() - start, file: filename };
    requests.push(evidence);
    if (!response.ok) { evidence.error = 'http-error'; return null; }
    const data = JSON.parse(raw); write(path.join(output, filename), data); evidence.sha256 = hash(path.join(output, filename)); return data;
  }
  const index = await get(plan.industrial.index), documents = [];
  if (index) for (const id of index.folders.flatMap(f => f.blueprints).slice(0, plan.industrial.maximum_documents)) {
    const j = await get(new URL(`${id}.json`, plan.industrial.index).href); if (!j) continue;
    const entities = Object.values(j.entities ?? {}), links = j.slotLinks ?? [];
    documents.push({ id, schema_version: j.schemaVersion, entities: entities.length, slot_links: links.length,
      all_entity_geometry: entities.length > 0 && entities.every(e => typeof e.definitionId === 'string' && Number.isFinite(e.position?.x) && Number.isFinite(e.position?.y) && [0, 90, 180, 270].includes(e.rotation)),
      entity_fields: [...new Set(entities.flatMap(Object.keys))], link_fields: [...new Set(links.flatMap(Object.keys))], definition_types: new Set(entities.map(e => e.definitionId)).size,
      configuration_fields: [...new Set(entities.flatMap(e => Object.keys(e.config ?? {})))],
      explicit_cheat_entities: entities.filter(e => /cheat|infinite/.test(e.definitionId)).length,
      ignore_stock_entities: entities.filter(e => Object.entries(e.config ?? {}).some(([k, v]) => /ignoreStock/.test(k) && v === true)).length,
      has_creator_reference: /bilibili\.com/.test(j.description ?? ''),
      geometry_fingerprint_raw_ids_excluded: digest(entities.map(e => ({ definitionId: e.definitionId, position: e.position, rotation: e.rotation, config: e.config })).sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)))) });
  }
  results.industrial = { indexed_documents: index?.folders.flatMap(f => f.blueprints).length ?? null, inspected: documents,
    scope: 'Public repository examples, not a large corpus. Simulator configuration and game-rule compatibility require independent checks. No training/republication permission inferred.' };
  const talos = await get(plan.talos.index), first = talos?.data?.[0], detail = first ? await get(plan.talos.detail_prefix + encodeURIComponent(first.id)) : null;
  const talosDetail = detail?.data ?? detail;
  results.talos = { reported_total: talos?.meta?.total ?? null, inspected_list_records: talos?.data?.length ?? 0, list_fields: Object.keys(first ?? {}), detail_fields: Object.keys(talosDetail ?? {}),
    has_entities_or_buildings: !!(talosDetail?.entities || talosDetail?.buildings || talosDetail?.factory_data),
    scope: 'One list record and one detail inspected. Metadata/share codes are not decoded geometry. API terms restrict monetization and redistribution; no bulk harvest.' };
  const listed = await get(plan.endfieldtools.index), inspected = [];
  if (Array.isArray(listed)) for (const entry of listed.slice(0, plan.endfieldtools.maximum_documents)) {
    const d = await get(plan.endfieldtools.detail_prefix + encodeURIComponent(entry.id)); if (!d) continue;
    const geometry = d.factory_data;
    inspected.push({ id: entry.id, slug: entry.slug, plan_type: d.plan_type, geometry_present: !!geometry, geometry_fields: Object.keys(geometry ?? {}),
      buildings: Array.isArray(geometry?.buildings) ? geometry.buildings.length : null,
      building_fields: [...new Set((geometry?.buildings ?? []).flatMap(Object.keys))],
      conveyor_lines: Array.isArray(geometry?.conveyorLines) ? geometry.conveyorLines.length : null,
      conveyor_fields: [...new Set((geometry?.conveyorLines ?? []).flatMap(Object.keys))] });
  }
  results.endfieldtools = { sampling: 'First twelve all-time popular results; NOT a random or exhaustive sample', inspected,
    geometry_documents: inspected.filter(d => d.geometry_present).length,
    scope: 'A public read-only route used by the shipped UI. API/schema availability does not establish a dataset reuse license. No community data used to train this experiment.' };
  write(path.join(output, 'sources.json'), { schema_version: 1, inspected_at: new Date().toISOString(), manifest_sha256: hash(options.manifest), requests, results });
  console.log(JSON.stringify({ industrial_documents: documents.length, talos_reported_total: results.talos.reported_total, endfieldtools_sample_geometry: results.endfieldtools.geometry_documents, output }));
} catch (e) { console.error(JSON.stringify(diagnostic('source-inspection-failed', 'source-inspection', e.message))); process.exitCode = 1; }
