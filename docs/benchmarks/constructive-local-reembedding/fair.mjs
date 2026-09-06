// Interleave finite domains without materializing their Cartesian products.
export function* diagonalProduct(xs, ys) {
  for (let rank = 0; rank < xs.length + ys.length - 1; rank++)
    for (let x = Math.max(0, rank - ys.length + 1); x <= Math.min(rank, xs.length - 1); x++)
      yield [xs[x], ys[rank - x]];
}

export function* interleave(iterators) {
  let active = iterators.map((iterator, index) => ({ iterator, index }));
  while (active.length) {
    const next = [];
    for (const entry of active) {
      const item = entry.iterator.next();
      if (!item.done) { next.push(entry); yield { index: entry.index, value: item.value }; }
    }
    active = next;
  }
}
