// Times as the pages say them: a day, or a day and a time, in the
// reader's own words for them rather than as a record holds them. Always
// in UTC, and said so: the page is drawn on the server, which doesn't
// know the reader's time zone.

import { perLocale } from '#lib/locale.ts';

const DAY = perLocale(
  (locale) =>
    new Intl.DateTimeFormat(locale, {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
      timeZone: 'UTC'
    })
);
const WHEN = perLocale(
  (locale) =>
    new Intl.DateTimeFormat(locale, {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
      timeZone: 'UTC',
      timeZoneName: 'short'
    })
);

// A day, and a time, each kept on one line where words wrap: the spaces
// within them are ones no line breaks at, and only the comma between the
// day and the time is one it may.
const whole = (said: string) => said.replace(/(?<!,) /g, '\u00a0');

/** The day of `iso`, as "5 Nov 2026"; what it was given, if not a time. */
export function day(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? iso : whole(DAY().format(at));
}

/** The day and time of `iso`, as "5 Nov 2026, 04:34 UTC"; what it was given, if not a time. */
export function when(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? iso : whole(WHEN().format(at));
}
