// A subject in a group's moderation queue, as its pages read it: whose it
// is, what happened to it, and the labels it wears now.

import { day, when } from '#lib/dates.ts';
import { reason } from '#lib/reasons.ts';
import { m } from '#lib/paraglide/messages.js';

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
/**
 * A record subject as the host's index holds it, which a moderator sees
 * though the group took it down (`takenDown`).
 */
export type Reviewed = {
  uri: string;
  cid: string;
  value: unknown;
  createdAt: string;
  takenDown: boolean;
};
/** A subject: `status` none for one the queue has never had; `record`, a record subject itself. */
export type Subject = {
  key: string;
  actor: Actor;
  status?: string;
  reportCount: number;
  events: Event[];
  record?: Reviewed;
};

/** The labels that hide what was posted, which the dialog offers as a takedown. */
export const TAKEDOWN = ['!takedown', '!hide'];

/** What a record says in words: each field of it that is text, a number or yes or no, but its type; times last. */
export function wordsOf(value: unknown): { name: string; text: string }[] {
  const time = (v: unknown) => typeof v === 'string' && /^\d{4}-\d{2}-\d{2}T/.test(v);
  const fields = Object.entries((value ?? {}) as Record<string, unknown>);
  return fields
    .filter(([name, v]) => name !== '$type' && ['string', 'number', 'boolean'].includes(typeof v))
    .sort(([, a], [, b]) => Number(time(a)) - Number(time(b)))
    .map(([name, v]) => ({ name, text: time(v) ? when(v as string) : String(v) }));
}

/** A file a record holds: the field it is in, its CID, and what kind of file. */
export type Held = { field: string; cid: string; mimeType: string };

/** The files a record holds, at any depth, each once, by the dotted path to it. */
export function filesOf(value: unknown, field = ''): Held[] {
  if (!value || typeof value !== 'object') return [];
  const blob = value as { $type?: string; ref?: { $link?: string }; mimeType?: string };
  if (blob.$type === 'blob' && blob.ref?.$link)
    return [{ field, cid: blob.ref.$link, mimeType: blob.mimeType ?? '' }];
  const found = Object.entries(value).flatMap(([k, v]) => filesOf(v, field ? `${field}.${k}` : k));
  return found.filter((f, i) => found.findIndex((g) => g.cid === f.cid) === i);
}

/** Where a moderator reviewing the record `uri` reads the file `cid` it holds, taken down or not. */
export function reviewedFile(uri: string, cid: string): string {
  const group = uri.slice('at://'.length).split('/')[0];
  const asked = new URLSearchParams({ group, subject: uri, cid });
  return `/xrpc/org.atgrp.internal.getModSubjectBlob?${asked}`;
}

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

/**
 * The space of `group`'s a label on `key` goes in, beside its subject: a
 * record in a space, that space; an account, or a record in no space,
 * the members space, where an account's labels go.
 */
export function besideOf(group: string, key: string): string {
  const [authority, space, kind, skey] = key.slice('at://'.length).split('/');
  if (key.startsWith('at://') && space === 'space')
    return `at://${authority}/space/${kind}/${skey}`;
  return `at://${group}/space/org.atgrp.members/self`;
}

/** A subject's reports, oldest first. */
export const reports = (s: Subject) => s.events.filter((e) => kindOf(e) === 'reportEvent');

/** What a subject is: an account, or a record by the last word of its collection. */
export function what(key: string): string {
  if (key.startsWith('did:')) return m.moderation_what_account();
  const kind = key.split('/').at(-2)?.split('.').at(-1);
  if (!kind) return m.moderation_what_record();
  return kind[0].toUpperCase() + kind.slice(1);
}

/** How often it was reported, and when last; or that the queue has never had it. */
export function reported(s: Subject): string {
  if (!s.status) return m.moderation_not_in_queue();
  if (!s.reportCount) return m.moderation_not_reported();
  const count = s.reportCount;
  const last = reports(s).at(-1);
  if (!last)
    return count === 2 ? m.moderation_reported_twice() : m.moderation_reported_times({ count });
  const on = day(last.createdAt);
  if (count === 1) return m.moderation_reported_on({ day: on });
  return count === 2
    ? m.moderation_reported_twice_last({ day: on })
    : m.moderation_reported_times_last({ count, day: on });
}

/** The reasons it was reported for, in words, each once. */
export const reasons = (s: Subject) => [
  ...new Set(reports(s).map((e) => reason(e.event.reasonType ?? '')))
];

/** What an event was, in a word or two. */
export function title(e: Event): string {
  const kind = kindOf(e);
  if (kind === 'reportEvent') return m.moderation_title_reported();
  if (kind === 'statusEvent')
    return e.event.status === 'escalated'
      ? m.moderation_title_escalated()
      : m.moderation_title_resolved();
  if (kind === 'labelEvent')
    return e.event.deleted ? m.moderation_title_label_removed() : m.moderation_title_labelled();
  return m.moderation_title_removed();
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
    return title ? m.moderation_breaks({ rule: title }) : m.moderation_breaks_a_rule();
  };
  const parts =
    kind === 'reportEvent'
      ? [reason(said.reasonType ?? ''), said.rule ? cited(said.rule) : '', said.reason ?? '']
      : kind === 'labelEvent'
        ? [said.val ?? '']
        : [said.note ?? ''];
  return parts.filter(Boolean).join(m.moderation_detail_separator());
}
