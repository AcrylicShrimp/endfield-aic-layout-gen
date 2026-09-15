import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, access, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

test('cached preparation preserves current manifest and rejects incompatible semantics before copying', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'aic-cache-contract-'));
  try {
    const source = path.join(root, 'source');
    const checkpoint = path.join(source, 'checkpoints/00000002');
    await mkdir(checkpoint, { recursive: true });
    const { contract } = JSON.parse(await readFile(new URL('./training.json', import.meta.url)));
    const valid = { schema: 'aic-training-checkpoint-v2', contract, replay: {} };
    await writeFile(path.join(checkpoint, 'state.json'), JSON.stringify({ update: 2, next: { wave: 1, slots: [] } }));
    const run = output => spawnSync(process.execPath,
      [fileURLToPath(new URL('./prepare-cached.mjs', import.meta.url)), source, output, '2'], { encoding: 'utf8' });
    for (const field of [null, 'observation', 'family_prior']) {
      const manifest = structuredClone(valid);
      if (field) manifest.contract[field] = 'other-v1';
      else delete manifest.contract;
      await writeFile(path.join(checkpoint, 'manifest.json'), JSON.stringify(manifest));
      const output = path.join(root, `invalid-${field}`);
      const result = run(output);
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /aic-learning.io-contract/);
      await assert.rejects(access(output), { code: 'ENOENT' });
    }
    const bytes = JSON.stringify(valid, null, 2);
    await writeFile(path.join(checkpoint, 'manifest.json'), bytes);
    const output = path.join(root, 'valid');
    const result = run(output);
    assert.equal(result.status, 0, result.stderr);
    assert.equal(await readFile(path.join(output, 'checkpoints/00000002/manifest.json'), 'utf8'), bytes);
    assert.equal(JSON.parse(await readFile(path.join(output, 'latest.json'))), 'checkpoints/00000002');
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
