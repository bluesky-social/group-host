// A subject in a group's moderation queue, as its pages read it: whose it
// is, what happened to it, and the labels it wears now.

import { day } from '#lib/dates.ts';
import { reason } from '#lib/reasons.ts';

export type Actor = { did: string; handle?: string };
export type Said = {
  $type?: string;
  reasonType?: string;
  reason?: string;
  rule?: string;
  status?: string;
  note?: string;
  val?: string;
  deleted?: boolean;
};
export type Event = { id: string; createdAt: string; creator?: Actor; cid?: string; event: Said };
/** A subject: `status` none for one the queue has never had. */
export type Subject = {
  key: string;
  actor: Actor;
  status?: string;
  reportCount: number;
  events: Event[];
};

/** The labels that hide what was posted, which the dialog offers as a takedown. */
export const TAKEDOWN = ['!takedown', '!hide'];

/** What an event is: `reportEvent`, `statusEvent`, `labelEvent`, `ejectEvent`. */
export const kindOf = (e: Event) => (e.event.$type ?? '').split('#')[1] ?? '';

/** The labels a subject wears now: each applied and not since taken back, in the order applied. */
export function labelsOf(events: Event[]): string[] {
  const now: string[] = [];
  for (const e of events) {
    const val = e.event.val;
    if (kindOf(e) !== 'labelEvent' || !val) continue;
    const at = now.indexOf(val);
    if (e.event.deleted) {
      if (at >= 0) now.splice(at, 1);
    } else if (at < 0) now.push(val);
  }
  return now;
}

/** The labels a group has put on any of `subjects`, each once, but those that take down. */
export function labelsUsed(subjects: Subject[]): string[] {
  const used = subjects.flatMap((s) =>
    s.events.filter((e) => kindOf(e) === 'labelEvent' && !e.event.deleted).map((e) => e.event.val)
  );
  return [...new Set(used)].filter((v): v is string => !!v && !TAKEDOWN.includes(v)).sort();
}

/** The version of a record subject its latest event names, if any. */
export const latestCid = (events: Event[]) => events.findLast((e) => e.cid)?.cid;

/** A subject's reports, oldest first. */
export const reports = (s: Subject) => s.events.filter((e) => kindOf(e) === 'reportEvent');

/** What a subject is: an account, or a record by the last word of its collection. */
export function what(key: string): string {
  if (key.startsWith('did:')) return 'Account';
  const kind = key.split('/').at(-2)?.split('.').at(-1) ?? 'record';
  return kind[0].toUpperCase() + kind.slice(1);
}

/** How often it was reported, and when last; or that the queue has never had it. */
export function reported(s: Subject): string {
  if (!s.status) return 'Not in the queue';
  if (!s.reportCount) return 'Not reported';
  const times =
    s.reportCount === 1 ? 'once' : s.reportCount === 2 ? 'twice' : `${s.reportCount} times`;
  const last = reports(s).at(-1);
  if (!last) return `Reported ${times}`;
  return s.reportCount === 1
    ? `Reported on ${day(last.createdAt)}`
    : `Reported ${times}, last on ${day(last.createdAt)}`;
}

/** The reasons it was reported for, in words, each once. */
export const reasons = (s: Subject) => [
  ...new Set(reports(s).map((e) => reason(e.event.reasonType ?? '')))
];

/** What an event was, in a word or two. */
export function title(e: Event): string {
  const kind = kindOf(e);
  if (kind === 'reportEvent') return 'Reported';
  if (kind === 'statusEvent') return e.event.status === 'escalated' ? 'Escalated' : 'Resolved';
  if (kind === 'labelEvent') return e.event.deleted ? 'Label removed' : 'Labelled';
  return 'Removed from the group';
}

/**
 * What was said with an event, if anything: a report's reason, the rule
 * it cites by its title in `rules` (by record key), and its words; a
 * label's value; a note.
 */
export function detail(e: Event, rules: Record<string, string>): string {
  const said = e.event;
  const kind = kindOf(e);
  const cited = (uri: string) => {
    const title = rules[uri.split('/').at(-1) ?? ''];
    return title ? `Breaks “${title}”` : 'Breaks a rule';
  };
  const parts =
    kind === 'reportEvent'
      ? [reason(said.reasonType ?? ''), said.rule ? cited(said.rule) : '', said.reason ?? '']
      : kind === 'labelEvent'
        ? [said.val ?? '']
        : [said.note ?? ''];
  return parts.filter(Boolean).join('. ');
}
