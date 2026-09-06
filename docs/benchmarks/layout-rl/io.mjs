import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

export const catalogKeys = ['recipes', 'facilities', 'items', 'transports', 'components'];
export const read = p => JSON.parse(fs.readFileSync(p, 'utf8'));
export const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
export const digest = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');
export const write = (p, value) => fs.writeFileSync(p, JSON.stringify(value) + '\n', { flag: 'wx' });
export const diagnostic = (code, stage, message, entity = null, severity = 'error') => ({ code: `rl-${code}`, severity, stage, message, entity });
export const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), diagnostic(code, 'environment', message, entity)); };
export function args(required, optional = []) {
  const argv = process.argv.slice(2), out = {};
  for (let i = 0; i < argv.length; i += 2) {
    const k = argv[i].slice(2);
    if (!argv[i].startsWith('--') || ![...required, ...optional].includes(k) || out[k] !== undefined || !argv[i + 1]) fail('input-invalid', `Invalid argument ${argv[i]}`);
    out[k] = argv[i + 1];
  }
  for (const k of required) if (out[k] === undefined) fail('input-invalid', `Missing --${k}`);
  return out;
}
export function sourcePaths(entry, found = new Set()) {
  entry = path.resolve(entry);
  if (found.has(entry)) return found; found.add(entry);
  for (const m of fs.readFileSync(entry, 'utf8').matchAll(/from\s+['"]([^'"]+)['"]/g))
    if (m[1].startsWith('.')) sourcePaths(path.resolve(path.dirname(entry), m[1]), found);
  return found;
}

// Single in-flight request per client. Multiple clients belong to independent environment workers.
export class Probe {
  constructor(options, timeoutMs = 3000) {
    this.timeoutMs = timeoutMs; this.ordinal = 0; this.pending = null; this.stderr = ''; this.closed = false;
    this.expectedCatalogs = catalogKeys.map(k => ({ path: path.resolve(options[k]), sha256: hash(options[k]) }));
    this.child = spawn(path.resolve(options.validator), ['--stream', ...catalogKeys.flatMap(k => [`--${k}`, path.resolve(options[k])])], { stdio: ['pipe', 'pipe', 'pipe'] });
    this.lines = createInterface({ input: this.child.stdout });
    this.child.stderr.on('data', d => { this.stderr = (this.stderr + d).slice(-8192); });
    this.child.stdin.on('error', e => this.abort(e.message));
    this.child.on('error', e => this.abort(`Cannot start validator ${options.validator}: ${e.message}`));
    this.child.on('exit', (code, signal) => { this.closed = true; this.abort(`Validator exited (${code}, ${signal}): ${this.stderr}`); });
    this.lines.on('line', line => {
      const pending = this.pending;
      if (!pending) return this.abort('Unexpected validator response');
      try {
        const result = JSON.parse(line);
        if (result.schema_version !== 1 || result.id !== pending.id || !result.report || !Array.isArray(result.catalog_provenance)
          || this.expectedCatalogs.some(e => !result.catalog_provenance.some(p => p.path === e.path && p.sha256 === e.sha256)))
          throw new Error('Mismatched validator response or catalog provenance');
        clearTimeout(pending.timer); this.pending = null; pending.resolve(result.report);
      } catch (e) { this.abort(e.message); }
    });
  }
  abort(message) {
    if (this.pending) { clearTimeout(this.pending.timer); this.pending.reject(Object.assign(new Error(message), diagnostic('validator-process', 'validation', message))); this.pending = null; }
    this.closed = true;
    if (this.child.exitCode === null && this.child.signalCode === null) this.child.kill('SIGKILL');
  }
  check(witness, timeoutMs = this.timeoutMs) {
    if (this.pending || this.closed) fail('validator-state', 'Validator is busy or closed');
    return new Promise((resolve, reject) => {
      const id = this.ordinal++, timer = setTimeout(() => this.abort(`Validator request ${id} exceeded ${timeoutMs} ms`), timeoutMs);
      this.pending = { id, resolve, reject, timer };
      this.child.stdin.write(JSON.stringify({ schema_version: 1, id, witness }) + '\n', e => { if (e) this.abort(e.message); });
    });
  }
  close() { this.lines.close(); this.child.stdin.end(); if (this.pending) this.abort('Validator closed with pending request'); }
}
