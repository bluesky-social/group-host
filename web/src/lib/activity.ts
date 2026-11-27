// What was done as a group, as the pages say it: each write as a short
// sentence, from the parts the host gives (its operation, the record's
// collection and key, the space it was in, a space's change, and whom it
// is about), never as the log's own words; and the apps that did it, by
// the names their client metadata gives.

import { m } from '#lib/paraglide/messages.js';

type Actor = { did: string; handle?: string };
/** An app as its client metadata names it, as the host read it. */
export type Client = { clientId: string; name?: string; uri?: string; logo?: string };
type Write = {
  operation: string;
  collection: string;
  rkey: string;
  space?: string;
  description?: string;
};
type Space = { space: string; operation: string; publicRead?: boolean };
export type Act = {
  member: string;
  actor?: Actor;
  clientId?: string;
  client?: Client;
  action: string;
  write?: Write;
  space?: Space;
  subject?: Actor;
  at: string;
};

/** The client a member uses to act as the group from this console, or the command. */
const OWN = 'getGroupAuth';
/** What an app password's grant is named by, before the password's name. */
const PASSWORD = 'app-password:';

/** Someone, by their handle where it is known. */
const named = (a?: Actor) => (a ? (a.handle ? `@${a.handle}` : a.did) : m.activity_someone());

/**
 * Where an app is, as people know it: the host of its client ID, the
 * address its metadata was read from, so an app cannot claim another's.
 * None for this console, an app password, or an app on this device.
 */
export function domain(id: string): string | undefined {
  if (!/^https?:\/\//.test(id)) return undefined;
  const at = new URL(id);
  return at.origin === 'http://localhost' && at.pathname === '/' ? undefined : at.hostname;
}

/**
 * An app as a reader knows it: this console, as itself; one signed in
 * with an app password, by the password's name; any other by the name its
 * metadata gives, or where it is.
 */
export function appName(id: string, client?: Client): string {
  if (id === OWN) return m.activity_this_console();
  if (id.startsWith(PASSWORD)) return id.slice(PASSWORD.length);
  return client?.name ?? domain(id) ?? id;
}

/** Whether an app did it, for a member who let it in, rather than the member themselves. */
export const byApp = (a: Act) => !!a.clientId && a.clientId !== OWN;

/** Whether an app password's app did it. */
export const byPassword = (a: Act) => !!a.clientId?.startsWith(PASSWORD);

/**
 * Who did it: an app, by its name and, where it has one, its domain
 * beside it ("Grain (grain.social)"); or the member themselves, from here.
 */
export function doer(a: Act): string {
  if (!a.clientId || !byApp(a)) return named(a.actor ?? { did: a.member });
  const name = appName(a.clientId, a.client);
  const at = a.client?.name ? domain(a.clientId) : undefined;
  return at ? m.activity_app({ name, domain: at }) : name;
}

/** The name an avatar is drawn from: the app's or the member's, past the marks around it. */
export const bare = (a: Act) =>
  byApp(a) ? appName(a.clientId!, a.client) : doer(a).replace(/^@/, '');

/** A part of an NSID in words: `privateGallery`, "private gallery". */
export const words = (part: string) => part.replace(/([a-z])([A-Z])/g, '$1 $2').toLowerCase();

/** A kind of space by what the group keeps in it: its profile, its members, its pool. */
export function spaceThing(kind: string): string {
  if (kind === 'org.atgrp.meta') return m.activity_space_profile();
  if (kind === 'org.atgrp.members') return m.activity_space_members();
  return words(kind.split('.').at(-1) ?? kind);
}

/** A space of the group's, by what it keeps ("the group's pool"), from its URI. */
function spaceName(uri: string): string {
  const kind = uri.split('/')[2] ?? '';
  if (kind === 'org.atgrp.inbox') return m.activity_space_inbox();
  return m.activity_space_of({ thing: spaceThing(kind) });
}

/** A kind of record by its app and its name's last part: `social.grain.actor.profile`, "Grain profile". */
function recordKind(collection: string): string {
  const parts = collection.split('.');
  if (parts.length < 3) return collection;
  const app = parts[1].charAt(0).toUpperCase() + parts[1].slice(1);
  return m.activity_record_kind({ app, thing: words(parts.at(-1)!) });
}

/** One record of `collection`: the group's own, as its profile is, or one of many. */
function record(w: Write): string {
  const kind = recordKind(w.collection);
  if (w.rkey === 'self') return m.activity_record_own({ kind });
  return /^[aeiou]/i.test(kind) ? m.activity_record_an({ kind }) : m.activity_record_a({ kind });
}

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
  'org.atgrp.about': (op) => (op === 'delete' ? m.activity_about_delete : m.activity_about_change),
  'org.atgrp.config': () => m.activity_config_change,
  'org.atgrp.confirmation': (op) =>
    op === 'delete' ? m.activity_confirmation_delete : m.activity_confirmation_create,
  'com.atproto.label.record': (op) =>
    op === 'delete' ? m.activity_label_delete : m.activity_label_create,
  'app.bsky.feed.post': (op) =>
    ({
      create: m.activity_post_create,
      update: m.activity_post_update,
      delete: m.activity_post_delete
    })[op]
};

type Inputs = { doer: string; record: string };
/** A write to another app's record, said of the record, in the group's repo or in a space. */
const DONE: Record<string, [(i: Inputs) => string, (i: Inputs & { space: string }) => string]> = {
  create: [m.activity_record_create, m.activity_record_create_in],
  update: [m.activity_record_update, m.activity_record_update_in],
  delete: [m.activity_record_delete, m.activity_record_delete_in]
};

/** What a change to one of the group's spaces is said as. */
function spaceLine(doer: string, s: Space): string {
  const meta = s.space.split('/')[2] === 'org.atgrp.meta';
  const space = spaceName(s.space);
  if (s.operation === 'create') return m.activity_space_create({ doer, space });
  if (s.operation === 'delete') return m.activity_space_delete({ doer, space });
  if (s.publicRead === true)
    return meta ? m.activity_group_public({ doer }) : m.activity_space_public({ doer, space });
  if (s.publicRead === false)
    return meta
      ? m.activity_group_members_only({ doer })
      : m.activity_space_members_only({ doer, space });
  return m.activity_space_update({ doer, space });
}

/** What another app's record is, as its lexicon says, under the sentence naming it. */
export const about = (a: Act) =>
  a.write && !SAID[a.write.collection]?.(a.write.operation) ? a.write.description : undefined;

/** What was done, as a sentence: who did it, and what it did ("@alice changed the profile"). */
export function line(a: Act): string {
  const who = doer(a);
  if (a.space) return spaceLine(who, a.space);
  const w = a.write;
  if (!w) return m.activity_action({ doer: who, action: a.action });
  const said = SAID[w.collection]?.(w.operation);
  if (said) return said({ doer: who, who: named(a.subject) });
  const inputs = { doer: who, record: record(w) };
  const done = DONE[w.operation];
  if (!done) return m.activity_record_other({ ...inputs, operation: w.operation });
  return w.space ? done[1]({ ...inputs, space: spaceName(w.space) }) : done[0](inputs);
}
