// Explicit bounded benchmark preparation; normal training generates fresh slots.
import { readFile, mkdir, cp, access, writeFile } from 'node:fs/promises';
import path from 'node:path';

const [sourceArg, outputArg, checkpointArg = '2'] = process.argv.slice(2);
if (!sourceArg || !outputArg || !/^\d+$/.test(checkpointArg)) {
  throw new Error('Usage: node tools/native-training/prepare-cached.mjs SOURCE OUTPUT CHECKPOINT');
}
const source = path.resolve(sourceArg), output = path.resolve(outputArg);
try {
  await access(output);
  throw new Error(`Output already exists: ${output}`);
} catch (error) {
  if (error.code !== 'ENOENT') throw error;
}
const ordinal = Number(checkpointArg);
if (!Number.isSafeInteger(ordinal) || ordinal <= 0) throw new Error('Invalid checkpoint ordinal');
const checkpoint = `checkpoints/${String(ordinal).padStart(8, '0')}`;
const read = async file => JSON.parse(await readFile(file, 'utf8'));
const manifest = await read(path.join(source, checkpoint, 'manifest.json'));
const expectedSchema = 'aic-training-checkpoint-v8';
if (manifest.schema !== expectedSchema) {
  throw new Error(`aic-learning.checkpoint-version: ${path.join(source, checkpoint, 'manifest.json')}: expected ${expectedSchema}; actual ${JSON.stringify(manifest.schema ?? null)}`);
}
const state = await read(path.join(source, checkpoint, 'state.json'));
if (state.update !== ordinal || !state.next) {
  throw new Error('A completed native training wave and its actual next plan are required');
}
const slots = state.next.slots.filter(slot => !slot.inherited).map(slot =>
  `inputs/${String(state.next.wave).padStart(8, '0')}/${String(slot.stream).padStart(4, '0')}`);
for (const slot of slots) await access(path.join(source, slot, 'manifest.json'));
for (const name of ['inputs', 'replay', 'checkpoints', 'evaluations', 'updates']) {
  await mkdir(path.join(output, name), { recursive: true });
}
await cp(path.join(source, checkpoint), path.join(output, checkpoint), { recursive: true });
for (const archive of Object.values(manifest.replay)) {
  if (!/^replay\/\d+\.bin$/.test(archive.path)) throw new Error('Noncanonical replay path');
  await cp(path.join(source, archive.path), path.join(output, archive.path));
}
for (const slot of slots) await cp(path.join(source, slot), path.join(output, slot), { recursive: true });
await writeFile(path.join(output, 'latest.json'), `${JSON.stringify(checkpoint)}\n`);
console.log(JSON.stringify({ schema: 'aic-cached-training-preparation-v1', source, output,
  checkpoint, wave: state.next.wave, actual_fresh_slots: slots.length }));
