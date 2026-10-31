// Amounts in cents, parsed from and formatted for the screens.
const MAX_CENTS = 100_000_000; // 1,000,000.00

export function parseAmount(text: string): number {
  const t = text.trim();
  const m = /^(-?)(\d+)(?:\.(\d{1,2}))?$/.exec(t);
  if (!m) throw new RangeError(`not an amount: ${text}`);
  const [, sign, whole, frac = ''] = m;
  const cents = Number(whole) * 100 + Number(frac.padEnd(2, '0'));
  if (cents >= MAX_CENTS) throw new RangeError(`amount too large: ${text}`);
  return sign ? -cents : cents;
}

export function formatAmount(cents: number): string {
  const sign = cents < 0 ? '-' : '';
  const abs = Math.abs(cents);
  const whole = Math.floor(abs / 100)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  const frac = String(abs % 100).padStart(2, '0');
  return `${sign}${whole}.${frac}`;
}
