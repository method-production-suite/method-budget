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

/** Adds thousands separators (`sep`, a comma by default) to a string of digits. */
function group(digits, sep = ',') {
  let out = '';
  for (let i = 0; i < digits.length; i++) {
    if (i > 0 && (digits.length - i) % 3 === 0) out += sep;
    out += digits[i];
  }
  return out;
}

/**
 * Cents written in currency `c`'s own format (decisions.md, entry 61): its
 * separators (`1,234.56`, or `1.234,56` when `c.separators` is
 * `point_comma`) and its symbol before the value (`$1,234.56`) or after it
 * (`1.234,56 €` when `c.symbol_after`); negatives in parentheses. `places`
 * 0 rounds to whole units, half away from zero. `c` is the budget's
 * currency entry (missing: the number alone, comma and point).
 */
export function currencyMoney(cents, c, places = 2) {
  if (cents === null || cents === undefined || !Number.isFinite(cents)) return '';
  const cur = c && typeof c === 'object' ? c : {};
  const [sep, mark] = cur.separators === 'point_comma' ? ['.', ','] : [',', '.'];
  const a = Math.abs(cents);
  const n =
    places === 0
      ? group(String(Math.trunc((a + 50) / 100)), sep)
      : `${group(String(Math.trunc(a / 100)), sep)}${mark}${String(a % 100).padStart(2, '0')}`;
  const symbol = typeof cur.symbol === 'string' ? cur.symbol.trim() : '';
  const s = !symbol ? n : cur.symbol_after === true ? `${n} ${symbol}` : `${symbol}${n}`;
  const neg = cents < 0 && (places !== 0 || Math.trunc((a + 50) / 100) !== 0);
  return neg ? `(${s})` : s;
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
