// What apps did as a group, as the pages say it: each write as a short
// sentence, from the parts the host gives (its operation, the record's
// collection and key, and whom it is about), never as the log's own words.

import { m } from '#lib/paraglide/messages.js';

type Actor = { did: string; handle?: string };
type Write = { operation: string; collection: string; rkey: string; space?: string };
export type Act = {
  member: string;
  actor?: Actor;
  clientId?: string;
  action: string;
  write?: Write;
  subject?: Actor;
  at: string;
};

/** The client a member uses to act as the group from this console, or the command. */
const OWN = 'getGroupAuth';

/** An app by its address's host, as people know it, past the scheme. */
export const app = (id: string) =>
  id.replace(/^https?:\/\//, '').replace(/\/client-metadata\.json$|\/$/, '');

/** Someone, by their handle where it is known. */
const named = (a?: Actor) => (a ? (a.handle ? `@${a.handle}` : a.did) : m.activity_someone());

/**
 * An app as a reader knows it: one signed in with an app password, by
 * the password's name; this console, as itself; any other by its address.
 */
export function appName(id: string): string {
  if (id === OWN) return m.activity_this_console();
  const password = id.match(/^app-password:(.+)$/);
  return password ? password[1] : app(id);
}

/** Who did it: the app, by its name or address; or the member themselves, from here. */
export const doer = (a: Act): string =>
  a.clientId && a.clientId !== OWN ? appName(a.clientId) : named(a.actor ?? { did: a.member });

/** The name an avatar is drawn from, past the marks around it. */
export const bare = (name: string) => name.replace(/^@/, '');

/** Whether an app did it, for a member who let it in, rather than the member themselves. */
export const byApp = (a: Act) => !!a.clientId && a.clientId !== OWN;

/** A sentence said of who did it (`doer`) and whom it is about (`who`). */
type Say = (inputs: { doer: string; who: string }) => string;

/** What each operation is said as, of a record of each collection. */
const SAID: Record<string, (op: string) => Say | undefined> = {
  'org.atgrp.membership': (op) =>
    ({
      create: m.activity_membership_create,
      update: m.activity_membership_update,
      delete: m.activity_membership_delete
    })[op],
  'org.atgrp.invite': (op) =>
    ({
      create: m.activity_invite_create,
      update: m.activity_invite_update,
      delete: m.activity_invite_delete
    })[op],
  'org.atgrp.rule': (op) =>
    ({
      create: m.activity_rule_create,
      update: m.activity_rule_update,
      delete: m.activity_rule_delete
    })[op],
  'org.atgrp.role': (op) =>
    ({
      create: m.activity_role_create,
      update: m.activity_role_update,
      delete: m.activity_role_delete
    })[op],
  'org.atgrp.profile': (op) =>
    op === 'delete' ? m.activity_profile_delete : m.activity_profile_change,
  'com.atproto.label.record': (op) =>
    op === 'delete' ? m.activity_label_delete : m.activity_label_create,
  'app.bsky.feed.post': (op) =>
    ({
      create: m.activity_post_create,
      update: m.activity_post_update,
      delete: m.activity_post_delete
    })[op]
};

/** A write to a collection said of no more than that. */
const DONE: Record<string, (inputs: { doer: string; collection: string }) => string> = {
  create: m.activity_record_create,
  update: m.activity_record_update,
  delete: m.activity_record_delete
};

/** What a space's making, change or end is said as, from the log's `<verb> space <uri>`. */
const SPACE: Record<string, (inputs: { doer: string }) => string> = {
  created: m.activity_space_create,
  updated: m.activity_space_update,
  deleted: m.activity_space_delete
};

/** What was done, as a sentence: who did it, and what the write did ("@alice changed the profile"). */
export function line(a: Act): string {
  const who = doer(a);
  const w = a.write;
  if (!w) {
    const space = SPACE[a.action.match(/^(\w+) space /)?.[1] ?? ''];
    return space ? space({ doer: who }) : m.activity_action({ doer: who, action: a.action });
  }
  const said = SAID[w.collection]?.(w.operation);
  if (said) return said({ doer: who, who: named(a.subject) });
  const done = DONE[w.operation];
  return done
    ? done({ doer: who, collection: w.collection })
    : m.activity_record_other({ doer: who, operation: w.operation, collection: w.collection });
}
