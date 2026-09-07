import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

export const limits = { documentation: 100 * 1024, source: 512 * 1024, vendored: 4 * 1024 * 1024 };
const prefix = 'docs/benchmarks/';
const asset = /\.(jsonl?|html|svg|png|gz|zip|txt|log|cpuprofile)$/i;
const source = /\.(md|mjs|py|toml|lock)$/;
const configPath = 'docs/maintenance/benchmark-source-assets.json';

export function inspectFile(filename, bytes, allowed) {
  const findings = [];
  const issue = (code, message) => findings.push({ code, severity: 'error', stage: 'repository-hygiene', entity: filename, message });
  if (/^(target|artifacts)\//.test(filename)) issue('generated-directory-tracked', 'Generated and locally retained artifacts must not be tracked.');
  if (filename.startsWith(prefix) && asset.test(filename) && !allowed.has(filename.slice(prefix.length)))
    issue('benchmark-output-tracked', 'Only reviewed source inputs, minimal fixtures and authored templates may be tracked here.');
  if (filename.startsWith(prefix) && !source.test(filename) && path.basename(filename) !== '.gitignore'
      && !asset.test(filename) && !allowed.has(filename.slice(prefix.length)))
    issue('benchmark-unknown-source-type', 'Unclassified benchmark file type; do not hide generated data behind another extension.');
  const limit = filename.startsWith('docs/') ? limits.documentation
    : filename.startsWith('data/game/source/') ? limits.vendored : limits.source;
  if (bytes > limit) issue('tracked-file-too-large', `${bytes} bytes exceeds the ${limit}-byte review limit; reduce or externalize the payload, including non-regenerable material.`);
  return findings;
}

export function sourceAssets(manifest) {
  if (manifest.schema_version !== 1 || !Array.isArray(manifest.inputs) || !Array.isArray(manifest.templates))
    throw new Error(`Invalid source asset schema in ${configPath}`);
  const files = [...manifest.inputs, ...manifest.templates];
  if (new Set(files).size !== files.length || files.some(p => typeof p !== 'string' || p.startsWith('/') ||
      p.split('/').some(x => !x || x === '.' || x === '..') || /[\\*?\[\]\r\n]/.test(p) || !asset.test(p)))
    throw new Error(`Expected unique explicit relative source paths in ${configPath}`);
  return new Set(files);
}

export const ignoreRules = allowed => ['*', '!*/', '!*.md', '!*.mjs', '!*.py', '!*.toml', '!*.lock', '!.gitignore',
  ...[...allowed].sort().map(p => `!/${p}`)];

export function check(root, mode) {
  if (!['--index', '--worktree'].includes(mode)) throw new Error('Expected --index or --worktree');
  const git = args => execFileSync('git', args, { cwd: root, maxBuffer: 32 * 1024 * 1024 });
  const read = p => mode === '--index' ? git(['show', `:${p}`]) : fs.readFileSync(path.join(root, p));
  const allowed = sourceAssets(JSON.parse(read(configPath)));
  const rules = read('docs/benchmarks/.gitignore').toString().split('\n').map(l => l.trim()).filter(l => l && !l.startsWith('#'));
  if (JSON.stringify(rules) !== JSON.stringify(ignoreRules(allowed)))
    throw new Error('Benchmark ignore rules must exactly match the source-only defaults and reviewed source-asset list.');
  let files;
  if (mode === '--index') {
    const entries = git(['ls-files', '--stage', '-z']).toString().split('\0').filter(Boolean).map(row => {
      const [metadata, filename] = [row.slice(0, row.indexOf('\t')), row.slice(row.indexOf('\t') + 1)];
      const [, oid, stage] = metadata.split(' ');
      if (stage !== '0') throw new Error(`Unmerged index entry: ${filename}`);
      return { filename, oid };
    });
    const sizes = execFileSync('git', ['cat-file', '--batch-check=%(objectsize)'], {
      cwd: root, input: entries.map(e => e.oid).join('\n') + '\n', encoding: 'utf8', maxBuffer: 32 * 1024 * 1024,
    }).trim().split('\n').map(Number);
    if (sizes.length !== entries.length || sizes.some(n => !Number.isSafeInteger(n) || n < 0)) throw new Error('Could not read staged object sizes.');
    files = entries.map((e, i) => ({ filename: e.filename, bytes: sizes[i] }));
  } else {
    const paths = new Set(git(['ls-files', '--cached', '--others', '--exclude-standard', '-z']).toString().split('\0').filter(Boolean));
    files = [...paths].filter(p => fs.existsSync(path.join(root, p))).map(filename => ({ filename, bytes: fs.lstatSync(path.join(root, filename)).size }));
  }
  const diagnostics = files.flatMap(f => inspectFile(f.filename, f.bytes, allowed));
  return { schema_version: 1, status: diagnostics.length ? 'rejected' : 'passed', mode, checked_files: files.length, limits, diagnostics };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length !== 3) throw new Error('Usage: node docs/maintenance/check-tracked-files.mjs --index|--worktree');
    const root = execFileSync('git', ['rev-parse', '--show-toplevel'], { encoding: 'utf8' }).trim();
    const report = check(root, process.argv[2]);
    console.log(JSON.stringify(report, null, 2));
    process.exitCode = report.status === 'passed' ? 0 : 1;
  } catch (error) {
    console.error(JSON.stringify({ code: 'repository-hygiene-input', severity: 'error', stage: 'repository-hygiene', message: error.message }));
    process.exitCode = 2;
  }
}
