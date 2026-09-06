// A view into one shared account. Absolute counters also charge direct A* expansion updates.
export class WorkSlice {
  constructor(parent, caps) {
    this.parent = parent; this.used = parent.used; this.start = { ...parent.used }; this.reason = null;
    this.limits = Object.fromEntries(Object.entries(parent.limits).map(([k, v]) => [k, k in caps ? Math.min(v, (this.start[k] ?? 0) + caps[k]) : v]));
  }
  timeLeft() { return this.parent.timeLeft(); }
  alive() { if (!this.parent.alive()) this.reason = this.parent.reason; return !this.reason; }
  take(kind, count = 1) {
    if (!this.alive()) return false;
    if (this.used[kind] + count > this.limits[kind]) { this.reason = kind; return false; }
    this.used[kind] += count; return true;
  }
  report() {
    return { used: Object.fromEntries(Object.entries(this.used).map(([k, v]) => [k, v - this.start[k]])), exhausted: this.reason,
      scope: this.reason ? this.reason === 'elapsed_ms' || this.used[this.reason] >= this.parent.limits[this.reason] ? 'global' : 'slice' : null };
  }
}
