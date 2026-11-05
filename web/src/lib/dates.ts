// Times as the pages say them: a day, or a day and a time, in the
// reader's own words for them rather than as a record holds them.

const DAY = new Intl.DateTimeFormat('en-GB', {
  day: 'numeric',
  month: 'short',
  year: 'numeric',
  timeZone: 'UTC'
});
const WHEN = new Intl.DateTimeFormat('en-GB', {
  day: 'numeric',
  month: 'short',
  year: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
  timeZone: 'UTC',
  timeZoneName: 'short'
});

/** The day of `iso`, as "5 Nov 2026"; what it was given, if not a time. */
export function day(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? iso : DAY.format(at);
}

/** The day and time of `iso`, as "5 Nov 2026, 04:34 UTC"; what it was given, if not a time. */
export function when(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? iso : WHEN.format(at);
}
