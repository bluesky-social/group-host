// Times as the site says them, in the reader's own zone.

const dayShort = new Intl.DateTimeFormat('en-US', {
  weekday: 'short',
  month: 'short',
  day: 'numeric'
});
const dayLong = new Intl.DateTimeFormat('en-US', {
  weekday: 'long',
  month: 'long',
  day: 'numeric'
});
const clock = new Intl.DateTimeFormat('en-US', { hour: 'numeric', minute: '2-digit' });

/** A card's day: `Thu Nov 19`. */
export const short = (date) => dayShort.format(date).replace(',', '');

/** A dialog's day: `Thursday, November 19`. */
export const long = (date) => dayLong.format(date);

/** A time of day: `6 pm`, `6:30 pm`. */
const at = (date) =>
  clock.format(date).replace(':00', '').replace(' AM', ' am').replace(' PM', ' pm');

/** When an event runs: `6 – 9 pm`, or `6 pm` with no end. */
export function hours(starts, ends) {
  if (!ends) return at(starts);
  const [from, to] = [at(starts), at(ends)];
  const same = from.slice(-2) === to.slice(-2);
  return `${same ? from.slice(0, -3) : from} – ${to}`;
}

/** How long ago `iso` was, or its day once a day has passed. */
export function ago(iso) {
  const s = (Date.now() - Date.parse(iso)) / 1000;
  if (s < 60) return 'just now';
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return short(new Date(iso));
}

/** What a `datetime-local` field holds, as the record keeps it. */
export const iso = (local) => (local ? new Date(local).toISOString() : undefined);

/** The reader's zone, as `UTC-8`. */
export function zone() {
  const hours = -new Date().getTimezoneOffset() / 60;
  return `UTC${hours >= 0 ? '+' : ''}${hours}`;
}
