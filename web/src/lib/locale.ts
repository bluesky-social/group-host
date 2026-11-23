// The reader's language: the one Paraglide picked for this request (a
// cookie, then what their browser asks for, then English), and how
// `Intl` is to say dates, lists and names in it.

import { getLocale, type Locale } from '#lib/paraglide/runtime.js';

/**
 * The `Intl` locale for each of the console's: English is British
 * English's dates, "5 Nov 2026", the day before the month as most of the
 * world reads it.
 */
const INTL: Record<Locale, string> = { en: 'en-GB' };

/** How `Intl` should say things for this reader. */
export function intl(): string {
  return INTL[getLocale()];
}

/**
 * One of each `make(locale)`, made the first time a locale asks: an
 * `Intl` formatter is slow to make and quick to use.
 */
export function perLocale<T>(make: (locale: string) => T): () => T {
  const made = new Map<string, T>();
  return () => {
    const locale = intl();
    let one = made.get(locale);
    if (one === undefined) {
      one = make(locale);
      made.set(locale, one);
    }
    return one;
  };
}
