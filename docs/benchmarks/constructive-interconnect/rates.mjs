// Exact nonnegative rational arithmetic for diagnostic flow contracts.
const gcd = (a, b) => b ? gcd(b, a % b) : a;
export function rate(n, d = 1n) {
  n = BigInt(n); d = BigInt(d);
  if (n < 0n || d <= 0n) throw new Error('Invalid nonnegative rate');
  const g = gcd(n, d); return { n: n / g, d: d / g };
}
export const add = (a, b) => rate(a.n * b.d + b.n * a.d, a.d * b.d);
export const multiply = (a, b) => rate(a.n * b.n, a.d * b.d);
export const compare = (a, b) => a.n * b.d < b.n * a.d ? -1 : a.n * b.d > b.n * a.d ? 1 : 0;
export const parseRate = a => rate(a.numerator, a.denominator);
export function dto(a) {
  const numerator = Number(a.n), denominator = Number(a.d);
  if (!Number.isSafeInteger(numerator) || !Number.isSafeInteger(denominator)) throw new Error('Rate exceeds safe JSON integer range');
  return { numerator, denominator };
}
export const capacity = c => rate(BigInt(c.quantity) * 1000n, c.duration_ms);
export function lanesFor(a, cap) {
  if (!cap.n) throw new Error('Zero transport capacity');
  const n = a.n * cap.d, d = a.d * cap.n, lanes = Number((n + d - 1n) / d);
  if (!Number.isSafeInteger(lanes)) throw new Error('Lane count overflow');
  return lanes;
}
