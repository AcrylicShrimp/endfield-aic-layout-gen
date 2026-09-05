// Optional diagnostic preload: Node --import ./clone-profile.mjs <closure command>.
import fs from 'node:fs';
const output = process.env.AIC_CLONE_PROFILE_OUTPUT;
if (!output) throw new Error('Missing AIC_CLONE_PROFILE_OUTPUT for clone timing preload');
const original = globalThis.structuredClone, categories = {};
globalThis.structuredClone = function(value, ...args) {
  const kind = value?.witness?.transport_networks ? 'candidate' : value?.transport_networks && value?.placements ? 'witness' : Array.isArray(value) && value[0]?.edge ? 'ports' : 'other';
  const t = performance.now();
  try { return original(value, ...args); }
  finally { const c = categories[kind] ??= { calls: 0, ms: 0 }; c.calls++; c.ms += performance.now() - t; }
};
process.on('exit', () => fs.writeFileSync(output, JSON.stringify({ schema_version: 1, categories }, null, 2) + '\n', { flag: 'wx' }));
