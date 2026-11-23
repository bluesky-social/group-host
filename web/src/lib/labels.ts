// A label's name and description in the reader's language. A group's
// labeler defines each of its labels in one or more languages
// (`locales`); the reader sees theirs, else English, else the first.

import { getLocale } from '#lib/paraglide/runtime.js';

/** One language's words for a label, as a label value definition has them. */
export type LabelLocale = { lang: string; name: string; description: string };

/** The words for a label that this reader reads best, if it has any. */
export function localized(locales: LabelLocale[]): LabelLocale | undefined {
  const reader = getLocale();
  const base = (lang: string) => lang.split('-')[0].toLowerCase();
  return (
    locales.find((l) => l.lang === reader) ??
    locales.find((l) => base(l.lang) === base(reader)) ??
    locales.find((l) => base(l.lang) === 'en') ??
    locales[0]
  );
}
