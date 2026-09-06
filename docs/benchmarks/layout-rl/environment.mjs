import { reembed, extractChains } from '../constructive-local-reembedding/model.mjs';
import { compactInterface } from '../constructive-auto-interface/compact.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { actionsFor, observe, contextFor } from './features.mjs';
import { prepareRoot, checkRoot, semanticKey } from './coverage.mjs';
import { transform } from './geometry.mjs';
import { compare, score } from './score.mjs';
import { catalogKeys, digest, fail, read } from './io.mjs';

export function validateEnvironmentPlan(p) {
  if (!p || ['horizon', 'route_expansions_per_action', 'validator_ms', 'episode_ms', 'max_actions', 'max_tokens'].some(k => !Number.isSafeInteger(p[k]) || p[k] < 1)
    || p.horizon > 1024 || p.route_expansions_per_action > 200000 || p.max_actions > 4096 || p.max_tokens > 1024
    || typeof p.connected_pairs !== 'boolean' || !Array.isArray(p.translation_steps) || !p.translation_steps.length
    || new Set(p.translation_steps).size !== p.translation_steps.length || p.translation_steps.some(n => !Number.isSafeInteger(n) || n < 1 || n > 8))
    fail('input-invalid', 'Invalid environment plan');
}

export class Environment {
  constructor(dataset, probe) {
    validateEnvironmentPlan(dataset.environment);
    this.plan = dataset.environment; this.probe = probe;
    this.catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(dataset.options[k])]));
    this.roots = new Map(dataset.roots.map(r => [r.id, { ...r, graph: prepareRoot(r, this.catalogs) }]));
    this.checkedRoots = new Set(); this.root = null;
  }
  async reset(id, { steps = this.plan.horizon, elapsed_ms = this.plan.episode_ms } = {}) {
    const root = this.roots.get(id);
    if (!root) fail('input-invalid', 'Unknown root', id);
    if (!Number.isSafeInteger(steps) || steps < 1 || steps > 16384 || !Number.isSafeInteger(elapsed_ms) || elapsed_ms < 1 || elapsed_ms > 300000)
      fail('input-invalid', 'Invalid episode budget', { steps, elapsed_ms });
    if (!this.checkedRoots.has(id)) {
      const validation = await this.probe.check(root.witness);
      if (!checkRoot(root, root.graph, root.witness, validation).factory_validated) fail('root-rejected', 'Worker root failed independent validation', id);
      this.checkedRoots.add(id);
    }
    this.root = root; this.current = root.witness; this.best = root.witness; this.currentScore = score(root.witness, this.catalogs); this.bestScore = this.currentScore;
    this.initial = this.currentScore; this.limit = steps; this.used = 0; this.elapsedLimit = elapsed_ms; this.started = performance.now();
    this.work = { proposals: 0, validations: 0, route_expansions: 0, route_ms: 0, validator_ms: 0, feature_ms: 0, accepted: 0, improvements: 0, worse_proposals: 0, worse_transitions: 0 };
    this.trace = []; this.incumbents = [{ proposal: 0, score: this.bestScore.tuple, witness_sha256: digest(this.best) }]; this.worst = this.currentScore.tuple;
    this.candidates = new Map(); this.nextCandidate = 0;
    this.featureState = null;
    return this.observation();
  }
  expired() { return performance.now() - this.started >= this.elapsedLimit; }
  get done() { return this.used >= this.limit || this.expired(); }
  observation() {
    if (!this.root) fail('state-invalid', 'Reset an environment before observing');
    const began = performance.now(); let features;
    if (this.featureState?.witness === this.current) {
      this.actions = this.featureState.actions;
      const context = contextFor(this.currentScore, this.bestScore, this.initial, Math.max(0, this.limit - this.used), this.limit);
      features = { ...this.featureState.features, actions: this.featureState.features.actions.map(a => [...a.slice(0, 15), ...context]) };
    } else {
      this.actions = actionsFor(this.current, this.root.graph, this.plan);
      features = observe(this.current, this.root.graph, this.actions, this.currentScore, this.bestScore, this.initial, Math.max(0, this.limit - this.used), this.limit, this.plan);
      this.featureState = { witness: this.current, features, actions: this.actions };
    }
    this.work.feature_ms += performance.now() - began;
    return { root: this.root.id, features, current_score: this.currentScore.tuple, best_score: this.bestScore.tuple, initial_score: this.initial.tuple,
      used: this.used, remaining: Math.max(0, this.limit - this.used), done: this.done, factory_validated: true };
  }
  async evaluate(action, { advance = true } = {}) {
    if (!this.root || this.done) fail('budget-exhausted', 'Episode ended; retained complete incumbent', this.root?.id);
    const before = this.current, beforeScore = this.currentScore, began = performance.now();
    this.used++; this.work.proposals++;
    const record = { proposal: this.used, action, parent_sha256: digest(before), before: beforeScore.tuple, status: 'pending', accepted: false, route_expansions: 0, validator_attempted: false };
    let candidate = null, candidateScore = null, generationEnded = false;
    try {
      let result; const detail = { witness: before, labels: {}, catalog_ports: project(before, this.catalogs), port_groups: { sources: [], targets: [] }, embedding: 'rl-private' };
      const routeStart = performance.now();
      if (action.kind === 'compact') {
        candidate = compactInterface(detail, this.catalogs).witness;
        result = { status: digest(candidate) === digest(before) ? 'unchanged' : 'candidate', routes: [] };
      } else if (action.kind === 'translate') {
        const chains = extractChains(before).chains;
        const priority = [...chains].sort((a, b) => Number(a.transport !== 'pipe') - Number(b.transport !== 'pipe') || a.id - b.id).map(c => c.id);
        result = reembed(detail, this.catalogs, { moves: [{ instances: action.instances, dx: action.dx, dy: action.dy }], endpointPolicy: 'terminal-flexible',
          routePriority: priority, totalWorkBudget: this.plan.route_expansions_per_action, workBudget: this.plan.route_expansions_per_action,
          onRouteResult: r => { record.route_expansions += r.expanded; this.work.route_expansions += r.expanded; } });
        candidate = result.detail?.witness ?? null;
      } else fail('input-invalid', 'Unknown modification kind', action.kind);
      record.route_ms = performance.now() - routeStart; this.work.route_ms += record.route_ms; generationEnded = true;
      if (record.route_expansions > this.plan.route_expansions_per_action) fail('budget-invariant', 'Action exceeded shared route budget');
      record.status = result.status; record.routes = result.routes; record.diagnostics = result.diagnostics;
      if (result.status === 'unchanged') { candidate = before; candidateScore = beforeScore; record.accepted = true; }
      else if (result.status === 'candidate') {
        candidate = transform(candidate);
        if (semanticKey(candidate) !== this.root.semantic_key) fail('semantic-invariant', 'Modification changed frozen inventory, ports, flows or external identity');
        if (this.expired()) record.status = 'late-before-validation';
        else {
          const validationStart = performance.now(); this.work.validations++; record.validator_attempted = true;
          let validation;
          // The episode deadline controls acceptance, not validator process health. Rounding
          // a nearly exhausted episode down to a 1-2 ms process timeout can kill a healthy
          // probe before the episode itself expires. In-flight work has its separate hard cap.
          try { validation = await this.probe.check(candidate, this.plan.validator_ms); }
          finally { record.validator_ms = performance.now() - validationStart; this.work.validator_ms += record.validator_ms; }
          record.validation_sha256 = digest({ ...validation, elapsed_ms: 0 });
          const coverage = checkRoot(this.root, this.root.graph, candidate, validation);
          record.accepted = coverage.factory_validated === true && !this.expired(); record.status = record.accepted ? 'accepted' : this.expired() ? 'late-result' : 'validation-rejected';
          if (record.accepted) candidateScore = score(candidate, this.catalogs);
        }
      }
    } catch (e) {
      if (!generationEnded) { record.route_ms = performance.now() - began; this.work.route_ms += record.route_ms; }
      if (['rl-semantic-invariant', 'rl-budget-invariant', 'rl-input-invalid'].includes(e.code)) throw e;
      record.status = e.code ?? 'candidate-error'; record.message = e.message;
      record.diagnostics = [{ code: record.status, severity: e.severity ?? 'warning', stage: e.stage ?? 'local-modification', message: e.message, entity: e.entity ?? action.instances ?? null }];
      if (record.status === 'rl-validator-process' && !this.expired()) throw e;
    }
    if (record.accepted && this.expired()) { record.accepted = false; record.status = 'late-result'; }
    if (record.accepted) {
      this.work.accepted++; record.score = candidateScore.tuple; record.witness_sha256 = digest(candidate);
      if (compare(candidateScore.tuple, this.bestScore.tuple) < 0) {
        this.best = candidate; this.bestScore = candidateScore; this.work.improvements++;
        this.incumbents.push({ proposal: this.used, score: candidateScore.tuple, witness_sha256: record.witness_sha256 });
      }
      if (compare(candidateScore.tuple, this.worst) > 0) this.worst = candidateScore.tuple;
      if (compare(candidateScore.tuple, beforeScore.tuple) > 0) this.work.worse_proposals++;
      if (advance) {
        if (compare(candidateScore.tuple, beforeScore.tuple) > 0) this.work.worse_transitions++;
        this.current = candidate; this.currentScore = candidateScore;
      }
      const id = this.nextCandidate++; this.candidates.set(id, { witness: candidate, score: candidateScore, parent: record.parent_sha256 }); record.candidate = id;
      // Baseline callers retain candidate handles for only one neighborhood at a time.
      if (advance) this.candidates.clear();
    }
    record.best = this.bestScore.tuple; record.elapsed_ms = performance.now() - began; this.trace.push(record);
    return record;
  }
  async step(index) {
    const observation = this.observation();
    if (observation.done) return { ...observation, transition: { status: 'budget-exhausted', accepted: false, performed: false } };
    if (!Number.isSafeInteger(index) || !this.actions[index] || !observation.features.action_mask[index]) fail('input-invalid', 'Unknown or masked action index', index);
    const record = await this.evaluate(this.actions[index]);
    return { ...this.observation(), transition: record };
  }
  summary({ witnesses = false, trace = false } = {}) {
    return { root: this.root.id, factory_validated: true, initial_score: this.initial, current_score: this.currentScore, best_score: this.bestScore,
      worst_score: this.worst, work: this.work, elapsed_ms: performance.now() - this.started,
      termination: this.expired() ? 'elapsed-budget' : this.used >= this.limit ? 'proposal-budget' : 'caller-ended',
      incumbents: this.incumbents, best_witness_sha256: digest(this.best), ...(witnesses ? { best_witness: this.best, current_witness: this.current } : {}), ...(trace ? { trace: this.trace } : {}) };
  }
}
