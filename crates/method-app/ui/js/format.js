// Number formatting. Money arrives as integer cents; it is never turned into a
// float for arithmetic, only split into whole units and cents for display.

/** 123456789 -> "1,234,567.89"; negatives in parentheses: "(1,000.00)". */
export function money(cents) {
  if (cents === null || cents === undefined || !Number.isFinite(cents)) return '';
  const neg = cents < 0;
  const a = Math.abs(cents);
  const whole = Math.trunc(a / 100);
  const frac = String(a % 100).padStart(2, '0');
  const s = `${group(String(whole))}.${frac}`;
  return neg ? `(${s})` : s;
}

/** Adds thousands separators to a string of digits. */
function group(digits) {
  let out = '';
  for (let i = 0; i < digits.length; i++) {
    if (i > 0 && (digits.length - i) % 3 === 0) out += ',';
    out += digits[i];
  }
  return out;
}

/**
 * A decimal string from the engine ("12.5", "-3", "0.333333333") for display,
 * with thousands separators and negatives in parentheses.
 */
export function decimal(s) {
  if (s === null || s === undefined) return '';
  const str = String(s);
  const neg = str.startsWith('-');
  const body = neg ? str.slice(1) : str;
  const [int, frac] = body.split('.');
  const t = group(int) + (frac ? `.${frac}` : '');
  return neg ? `(${t})` : t;
}

/** "a, b" from a list of names. */
export function names(list) {
  return Array.isArray(list) ? list.join(', ') : '';
}
