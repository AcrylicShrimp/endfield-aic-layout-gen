import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { check, inspectFile, sourceAssets, limits, ignoreRules } from './check-tracked-files.mjs';

const allowed = sourceAssets({ schema_version: 1, inputs: ['example/input.json'], templates: ['example/template.html'] });
test('reviewed inputs and source templates are distinct from generated output', () => {
  for (const filename of ['docs/benchmarks/example/input.json', 'docs/benchmarks/example/template.html', 'docs/benchmarks/example/run.mjs'])
    assert.deepEqual(inspectFile(filename, 100, allowed), []);
  for (const ext of ['json', 'html', 'jsonl', 'gz', 'png', 'log', 'cpuprofile'])
    assert.equal(inspectFile(`docs/benchmarks/example/result.${ext}`, 1, allowed)[0].code, 'benchmark-output-tracked');
  assert.equal(inspectFile('docs/benchmarks/example/result.bin', 1, allowed)[0].code, 'benchmark-unknown-source-type');
});
test('non-regenerable documents and reviewed fixtures still have a byte limit', () => {
  for (const filename of ['docs/report.md', 'docs/handoff.patch', 'docs/benchmarks/example/input.json']) {
    assert.deepEqual(inspectFile(filename, limits.documentation, allowed), []);
    assert.equal(inspectFile(filename, limits.documentation + 1, allowed)[0].code, 'tracked-file-too-large');
  }
  assert.equal(inspectFile('artifacts/checkpoint.json', 1, allowed)[0].code, 'generated-directory-tracked');
  assert.equal(inspectFile('target/run.json', 1, allowed)[0].code, 'generated-directory-tracked');
});
test('source inputs and upstream table snapshots have explicit finite limits', () => {
  assert.equal(inspectFile('crates/aic-data/src/example.rs', limits.source + 1, allowed)[0].code, 'tracked-file-too-large');
  assert.deepEqual(inspectFile('data/game/source/table-cfg/ItemTable.json', limits.vendored, allowed), []);
  assert.equal(inspectFile('data/game/source/table-cfg/ItemTable.json', limits.vendored + 1, allowed)[0].code, 'tracked-file-too-large');
});
test('source-asset declarations cannot grant wildcard or escaping exceptions', () => {
  for (const entry of ['../raw.json', '/raw.json', '**/*.json', 'example//raw.json', 'example\\raw.json'])
    assert.throws(() => sourceAssets({ schema_version: 1, inputs: [entry], templates: [] }), /explicit relative/);
  assert.throws(() => sourceAssets({ schema_version: 1, inputs: ['a.json'], templates: ['a.json'] }), /unique/);
});

test('index mode checks staged bytes and rejects force-added ignored output', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-hygiene-test-'));
  const write = (name, value) => { const p = path.join(root, name); fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, value); };
  const git = args => execFileSync('git', args, { cwd: root, stdio: 'pipe' });
  try {
    git(['init', '--quiet']);
    write('docs/maintenance/benchmark-source-assets.json', JSON.stringify({ schema_version: 1, inputs: [], templates: [] }));
    write('docs/benchmarks/.gitignore', ignoreRules(new Set()).join('\n') + '\n');
    write('docs/report.md', 'x'.repeat(limits.documentation + 1));
    git(['add', '.']);
    write('docs/report.md', 'Small unstaged replacement.\n');
    assert.equal(check(root, '--worktree').status, 'passed');
    assert.equal(check(root, '--index').diagnostics[0].code, 'tracked-file-too-large');
    git(['add', 'docs/report.md']);
    write('docs/benchmarks/run/report.json', '{}\n');
    git(['add', '--force', 'docs/benchmarks/run/report.json']);
    assert.equal(check(root, '--index').diagnostics[0].code, 'benchmark-output-tracked');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('the source asset list and ignore exceptions must agree', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-hygiene-exceptions-'));
  try {
    execFileSync('git', ['init', '--quiet', root]);
    fs.mkdirSync(path.join(root, 'docs/maintenance'), { recursive: true });
    fs.mkdirSync(path.join(root, 'docs/benchmarks'), { recursive: true });
    fs.writeFileSync(path.join(root, 'docs/maintenance/benchmark-source-assets.json'), JSON.stringify({ schema_version: 1, inputs: ['a.json', 'b.json'], templates: [] }));
    const valid = ignoreRules(new Set(['a.json', 'b.json']));
    for (const invalid of [valid.map(l => l === '!/b.json' ? '!/a.json' : l), valid.slice(1), [...valid, '!**/*.json']]) {
      fs.writeFileSync(path.join(root, 'docs/benchmarks/.gitignore'), invalid.join('\n') + '\n');
      assert.throws(() => check(root, '--worktree'), /exactly match/);
    }
    fs.writeFileSync(path.join(root, 'docs/benchmarks/.gitignore'), valid.join('\n') + '\n');
    assert.equal(check(root, '--worktree').status, 'passed');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});
