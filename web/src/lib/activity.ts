// What apps did as a group, as the pages say it: each write as a short
// sentence, from the parts the host gives (its operation, the record's
// collection and key, and whom it is about), never as the log's own words.

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
const named = (a?: Actor) => (a ? (a.handle ? `@${a.handle}` : a.did) : 'someone');

/**
 * An app as a reader knows it: one signed in with an app password, by
 * the password's name; this console, as itself; any other by its address.
 */
export function appName(id: string): string {
  if (id === OWN) return 'This console';
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

/** What each operation is said as, of a record of each collection, given whom it is about. */
const SAID: Record<string, (op: string, who: string) => string> = {
  'fyi.opensocial.membership': (op, who) =>
    ({ create: `added ${who}`, update: `changed ${who}'s roles`, delete: `removed ${who}` })[op] ??
    '',
  'fyi.opensocial.invite': (op, who) =>
    ({
      create: `invited ${who}`,
      update: `changed ${who}'s invite`,
      delete: `cancelled ${who}'s invite`
    })[op] ?? '',
  'fyi.opensocial.rule': (op) =>
    ({ create: 'added a rule', update: 'changed a rule', delete: 'deleted a rule' })[op] ?? '',
  'fyi.opensocial.role': (op) =>
    ({ create: 'created a role', update: 'changed a role', delete: 'deleted a role' })[op] ?? '',
  'fyi.opensocial.profile': (op) =>
    op === 'delete' ? 'removed the profile' : 'changed the profile',
  'fyi.opensocial.permissions': () => 'changed what roles can do',
  'fyi.opensocial.access': () => 'changed who sees a space',
  'com.atproto.label.record': (op) => (op === 'delete' ? 'removed a label' : 'added a label'),
  'app.bsky.feed.post': (op) =>
    ({ create: 'posted', update: 'edited a post', delete: 'deleted a post' })[op] ?? ''
};

const DONE: Record<string, string> = { create: 'added', update: 'changed', delete: 'deleted' };

/** What a write did, as the rest of a sentence after who did it ("changed the profile"). */
export function did(a: Act): string {
  const w = a.write;
  if (!w) return a.action;
  const said = SAID[w.collection]?.(w.operation, named(a.subject));
  return said || `${DONE[w.operation] ?? w.operation} a record of ${w.collection}`;
}
