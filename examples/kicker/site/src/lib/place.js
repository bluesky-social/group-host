// What the site asks the group host, all of it through `os` (the host's
// /_os/kit.js, loaded before the app): the reads `getGroup`, `listMembers`
// and `getSpaceBlob`, every record in the place's spaces (`os.space`, each
// synced by the host's page with each writer's repo), and writes
// of each kind of record below in the visitor's own repo in the place's
// spaces, as the site's record declares them (harness/seed.mjs publishes
// it so). What each room shows of the records is worked out in views.js.

import { stamped } from './views.js';

/** The place's records, by collection, each in one of its spaces. */
export const NS = 'com.example.place';

/**
 * The host's space type for a group's site: the room and the calendar are
 * each a space of it, under their own key (harness/seed.mjs makes them).
 */
const SITE_SPACE = 'fyi.opensocial.siteSpace';

/** The host's `os`, which the page around the site answers. */
const os = () => window.os;

/** The record key of `uri`: its last part. */
export const keyOf = (uri) => uri.split('/').at(-1);

/**
 * The place, who is looking, and its members: `members` a Map of DID to
 * `{handle, roles}`, `viewer` their standing (`member`), and `space(kind)`
 * the URI of the place's space of that kind. Signed out, or turned away
 * by the host, only the place and who is looking.
 */
export async function context() {
  const group = await os().group();
  const me = await os().me();
  const space = (kind) => `at://${group.did}/space/${SITE_SPACE}/${kind}`;
  const base = {
    group,
    me,
    space,
    place: null,
    members: new Map(),
    viewer: null
  };
  if (!me.signedIn) return base;
  try {
    const [{ group: place }, { members }] = await Promise.all([
      os().call('fyi.opensocial.getGroup', { group: group.did }),
      os().call('fyi.opensocial.listMembers', { group: group.did, limit: 100 })
    ]);
    const roles = (m) => m.roles.map((r) => r.role);
    const byDid = new Map(
      members.map((m) => [m.actor.did, { handle: m.actor.handle, roles: roles(m) }])
    );
    return { ...base, place, members: byDid, viewer: place.viewer };
  } catch (e) {
    if (e.locked) return base;
    throw e;
  }
}

/** The place's space of `kind`, as the kit opens it: synced, held and written as the SDK's GroupSpace is. */
const spaceOf = (ctx, kind) => os().space(ctx.space(kind));

/**
 * Every record the place's space of `kind` holds, of any writer, once the
 * host's page has brought it up to date: GroupRecord[], in no order.
 */
export async function held(ctx, kind) {
  const space = spaceOf(ctx, kind);
  await space.sync();
  return space.records();
}

/** Calls `fn(records)` whenever a later sync changes the space of `kind`; returns what stops it. */
export const onHeld = (ctx, kind, fn) => spaceOf(ctx, kind).onChange(fn);

/**
 * A record of `fields`, as the lexicon has it: made now, or, written over
 * `was` (the record held at its key), when that was made. Its `$type` is
 * the collection it is written in. Plain data, as it is posted to the
 * host's page: a field taken from what a room holds (a picture already on
 * a card, say) is a Svelte state proxy, which a message cannot carry.
 */
const record = (fields, was) =>
  JSON.parse(JSON.stringify(stamped(fields, was, new Date().toISOString())));

/** Writes a new record of `collection` in the visitor's repo in the space of `kind`. */
export const create = (ctx, kind, collection, fields) =>
  spaceOf(ctx, kind).create(`${NS}.${collection}`, record(fields));

/**
 * Writes `fields` at `rkey`, in place of `was`, the record held there (none
 * if nothing is), keeping when it was first made.
 */
export const put = (ctx, kind, collection, rkey, fields, was) =>
  spaceOf(ctx, kind).put(`${NS}.${collection}`, record(fields, was), { rkey });

/** Deletes the visitor's record at `rkey`. */
export const remove = (ctx, kind, collection, rkey) =>
  spaceOf(ctx, kind).delete(`${NS}.${collection}`, { rkey });

/** What `did` holds in the place: their roles, none if no member. */
export const rolesOf = (ctx, did) => ctx.members.get(did)?.roles ?? [];
export const isAdmin = (ctx) => rolesOf(ctx, ctx.me.did).includes('admin');
/** Whether the viewer publishes events: a host, or an admin. */
export const isHost = (ctx) => isAdmin(ctx) || rolesOf(ctx, ctx.me.did).includes('host');
/** Whether the viewer reads the group host's moderation queue, as their roles let them. */
export const moderates = (ctx) => !!ctx.viewer?.actions?.includes('mod.read');
/** Someone's handle, as the place knows them, else their DID. */
export const handleOf = (ctx, did) => ctx.members.get(did)?.handle ?? did;

// ---- moderation, the group host's: reports in, labels out ----

/** The group's labels: its own records of this, in the space of what each is on. */
export const LABEL = 'com.atproto.label.record';

/** atproto's own labels, which every group applies: each hides, and says no more. */
const BUILT_IN = {
  '!hide': { defaultSetting: 'hide', severity: 'none' },
  '!takedown': { defaultSetting: 'hide', severity: 'none' }
};

/**
 * How what wears the labels `vals` is shown, as the group defines each:
 * `hide` if any hides it, else `warn` if any warns of it, else `show`;
 * the names of the group's own that say anything; and whether the
 * moderators marked it at all (`any`). A value the group does not define
 * is passed over, as atproto has apps do.
 */
export function judged(ctx, vals = []) {
  const defined = Object.fromEntries((ctx.place?.labels ?? []).map((d) => [d.identifier, d]));
  const defs = vals.map((v) => BUILT_IN[v] ?? defined[v]).filter(Boolean);
  const own = vals.map((v) => defined[v]).filter(Boolean);
  const said = own.filter((d) => d.defaultSetting !== 'ignore' || d.severity !== 'none');
  const names = [...new Set(said.map((d) => d.locales?.[0]?.name ?? d.identifier))];
  const has = (setting) => defs.some((d) => d.defaultSetting === setting);
  const setting = has('hide') ? 'hide' : has('warn') ? 'warn' : 'show';
  return { setting, names, any: setting !== 'show' || names.length > 0 };
}

/** A record, at the version read, as a report or an appeal names it. */
const subjectOf = (record) => ({
  $type: 'fyi.opensocial.defs#recordSubject',
  uri: record.uri,
  cid: record.cid
});

/**
 * Reports `record` to the place's moderators, into the group host's queue,
 * as the visitor: `reasonType` one of atproto's, with their own words and
 * the rule it breaks, if they give them.
 */
export const report = (ctx, record, reasonType, { reason, rule } = {}) =>
  os().act('fyi.opensocial.createReport', {
    group: ctx.group.did,
    subject: subjectOf(record),
    reasonType,
    ...(reason ? { reason } : {}),
    ...(rule ? { rule } : {})
  });

/** Appeals what the moderators did to the visitor's own `record`, in their words. */
export const appeal = (ctx, record, reason) =>
  report(ctx, record, 'com.atproto.moderation.defs#reasonAppeal', { reason });

/** The rules a report may cite: each its title and the URI of its record. */
export const rulesOf = (ctx) =>
  (ctx.place?.rules ?? []).map((r) => ({
    title: r.title,
    uri: `at://${ctx.group.did}/space/fyi.opensocial.meta/self/${ctx.group.did}/fyi.opensocial.rule/${r.rkey}`
  }));

/** Where a moderator reviews `uri`, in the group host's console. */
export const reviewAt = (ctx, uri) =>
  `${ctx.group.console}/g/${ctx.group.handle}/moderation?review=${encodeURIComponent(uri)}`;

/** Uploads `file`, a picture, to the visitor's own PDS: its reference, for a record. */
export const upload = (file) => os().upload(file);

/** Each picture read, as a URL the page shows, by its CID: its bytes never change. */
const pictures = new Map();

/**
 * The URL of `blob`, a picture `repo` uploaded for a record in the place's
 * space of `kind`, read through the host, which asks their PDS for it.
 */
export function picture(ctx, kind, repo, blob) {
  const cid = blob.ref.$link;
  if (!pictures.has(cid)) {
    const read = os()
      .call('fyi.opensocial.getSpaceBlob', {
        group: ctx.group.did,
        space: ctx.space(kind),
        repo,
        cid
      })
      .then((file) => URL.createObjectURL(file));
    pictures.set(cid, read);
    read.catch(() => pictures.delete(cid));
  }
  return pictures.get(cid);
}

/** Calls `fn` whenever who is looking changes. */
export const onChange = (fn) => os().onChange(fn);

/** Opens the host's sign-in dialog. */
export const signIn = () => os().signIn();

/** Signs the visitor out; `onChange` hears of it. */
export const signOut = () => os().signOut();
