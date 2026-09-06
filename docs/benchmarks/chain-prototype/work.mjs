export class Work {
  constructor(limits, now) { this.limits = limits; this.used = { proposals: 0, route_expansions: 0, validations: 0 }; this.now = now; this.start = now(); this.reason = null; }
  timeLeft() { return Math.max(0, this.limits.elapsed_ms - (this.now() - this.start)); }
  alive() { if (this.timeLeft() <= 0) { this.reason = 'elapsed_ms'; return false; } return true; }
  take(kind, count = 1) {
    if (!this.alive()) return false;
    if (this.used[kind] + count > this.limits[kind]) { this.reason = kind; return false; }
    this.used[kind] += count; return true;
  }
  report() { return { limits: this.limits, used: this.used, elapsed_ms: this.now() - this.start, exhausted: this.reason }; }
}
