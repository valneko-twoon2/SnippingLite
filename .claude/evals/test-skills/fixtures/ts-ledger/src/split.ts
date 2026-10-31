// Splitting a bill between people by weight, to the cent.
export function splitBill(totalCents: number, weights: number[]): number[] {
  if (!Number.isInteger(totalCents) || totalCents < 0) throw new RangeError('the total must be a whole number of cents, 0 or more');
  if (weights.length === 0) throw new RangeError('no one to split between');
  if (weights.some((w) => !Number.isInteger(w) || w < 0)) throw new RangeError('each weight must be a whole number of at least 1');
  const sum = weights.reduce((a, b) => a + b, 0);
  const exact = weights.map((w) => (totalCents * w) / sum);
  const shares = exact.map((x) => Math.floor(x));
  let left = totalCents - shares.reduce((a, b) => a + b, 0);
  const order = exact.map((x, i) => ({ i, rest: x - Math.floor(x) })).sort((a, b) => b.rest - a.rest);
  for (const { i } of order) {
    if (left === 0) break;
    shares[i]! += 1;
    left -= 1;
  }
  return shares;
}
