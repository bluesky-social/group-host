// A group's settings, each part a page of its own under
// /g/<handle>/settings: what each loads, and the writes its forms make,
// which each page's +page.server.ts takes the ones of it wants.

import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getGroup from '#lib/lexicons/fyi/opensocial/getGroup.ts';
import type * as listAppPasswords from '#lib/lexicons/fyi/opensocial/listAppPasswords.ts';
import type * as listGroupTokens from '#lib/lexicons/fyi/opensocial/listGroupTokens.ts';
import type * as listRecoveryKeys from '#lib/lexicons/fyi/opensocial/listRecoveryKeys.ts';
import { fail, redirect, type Action } from '@sveltejs/kit';
import { FAVICONS } from '$app/env/private';
import { act, xrpc, type Answer } from '#lib/data.ts';
import { refusal, refused } from '#lib/errors.ts';
import { m } from '#lib/paraglide/messages.js';
import { getLocale } from '#lib/paraglide/runtime.js';
import { applyAsGroup, membersOf, withGroup, type Done, type GroupSession } from '#lib/asGroup.ts';
import { SCOPES } from '#lib/actions.ts';
import { ADDRESS, type Place } from '#lib/places.ts';

type Handle = `${string}.${string}`;
type Event = { fetch: typeof fetch; params: { handle?: string } };

/** The group the page's address names, by its handle: its DID. */
export async function groupOf(event: Event) {
  const handle = (event.params.handle ?? '') as Handle;
  return (
    await xrpc<typeof resolveHandle.main>(event, 'com.atproto.identity.resolveHandle', { handle })
  ).did;
}

/**
 * What the person looking may change of a group, as getGroup says of
 * them: whether they configure it (`group.configure` among their
 * actions), and whether they may make app passwords for it (their roles
 * may write any collection as the group, as the host asks of one who
 * makes one). Settings is for one who may do either.
 */
export function changes(view: Answer<typeof getGroup.main>) {
  const viewer = view.group.viewer;
  const mine = new Set(viewer?.roles.map((r) => r.role) ?? []);
  const configures = viewer?.actions?.includes('group.configure') ?? false;
  const issues = (view.bindings ?? []).some(
    (b) => mine.has(b.role) && (b.repoCollections ?? []).some((c) => c.collection === '*')
  );
  return { configures, issues };
}

/**
 * What every settings page shows: the group, whether the person looking
 * may configure it, and the app passwords theirs to see, where they may
 * make them; the pages they may see, as the tabs list them.
 */
export async function settings(event: Event) {
  const group = await groupOf(event);
  const view = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', { group });
  const { configures, issues } = changes(view);
  const passwords =
    configures || issues
      ? await xrpc<typeof listAppPasswords.main>(event, 'fyi.opensocial.listAppPasswords', {
          group
        })
      : null;
  return { ...view, configures, passwords: passwords?.passwords ?? null };
}

/** Whether the group is listed: while anyone may read its profile, its meta space public. */
export function listed(view: Answer<typeof getGroup.main>) {
  const meta = view.spaces?.find((s) => s.uri === view.group.meta);
  return meta?.policy?.publicRead ?? false;
}

/** The apps acting as the group, and what they did lately: to one who configures it. */
export const tokensOf = (event: Event, group: string) =>
  xrpc<typeof listGroupTokens.main>(event, 'fyi.opensocial.listGroupTokens', {
    group: group as Did
  });

/** The group's recovery keys, or null where the host does not keep them. */
export const recoveryOf = (event: Event, group: string) =>
  xrpc<typeof listRecoveryKeys.main>(event, 'fyi.opensocial.listRecoveryKeys', {
    group: group as Did
  }).catch(() => null);

async function asked(event: Event & { request: Request }) {
  return { group: await groupOf(event), form: await event.request.formData() };
}

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();

const PROFILE = 'fyi.opensocial.profile';
const RULE = 'fyi.opensocial.rule';
/** How many links, topics and languages a profile holds, and how long a topic is, as its lexicon says. */
const MAX_LINKS = 5;
const MAX_TOPICS = 10;
const MAX_LANGUAGES = 5;
const TOPIC_AT_MOST = 64;
const segments = new Intl.Segmenter();
/** The most bytes a link's icon may be, as its lexicon says; and how long its site is waited for. */
const ICON_AT_MOST = 100_000;
const ICON_WAIT = 8_000;

/**
 * The icon of the site at `url`, as the favicon service gives it: a PNG
 * no larger than a link's icon may be; or none, for a site with none, or
 * one too slow, or no service.
 */
async function iconOf(url: string) {
  if (!FAVICONS || !/^https?:\/\//.test(url)) return null;
  const res = await fetch(`${FAVICONS}${encodeURIComponent(url)}`, {
    signal: AbortSignal.timeout(ICON_WAIT)
  }).catch(() => null);
  if (!res?.ok || res.headers.get('content-type') !== 'image/png') return null;
  const bytes = await res.arrayBuffer().catch(() => null);
  return bytes && bytes.byteLength <= ICON_AT_MOST ? bytes : null;
}

/**
 * Where the form says the group meets, as an address: its town and
 * country, over what else the address it had says; none, with neither; a
 * place of another kind kept, while the form gives neither in its place;
 * or why not, a town with no country.
 */
function whereFrom(form: FormData, was?: Place): Place | string | undefined {
  const town = text(form, 'town');
  const country = text(form, 'country');
  if (town && !country) return m.settings_country_needed();
  if (!country) return was && was.$type !== ADDRESS ? was : undefined;
  const { locality: _, ...kept } = was?.$type === ADDRESS ? was : {};
  return { ...kept, $type: ADDRESS, country, ...(town ? { locality: town } : {}) };
}

type Did = `did:${string}:${string}`;

/**
 * A picture the profile names by its CID, as the record holds it: its
 * type and size as the host serves it.
 */
async function picture(event: Event, did: string, cid?: string) {
  if (!cid) return [];
  const res = await event.fetch(`/img/${did}/${cid}`);
  if (!res.ok) return [];
  const size = (await res.arrayBuffer()).byteLength;
  return [{ $type: 'blob', ref: { $link: cid }, mimeType: res.headers.get('content-type'), size }];
}

/**
 * The group's meta space, and its profile there as it is now, to put back
 * whole with what is changed: what the form does not show is kept.
 */
async function profileNow(event: Event, group: string) {
  const { group: now } = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', {
    group: group as `did:${string}:${string}`
  });
  const kept = Object.fromEntries(
    ['description', 'url', 'links', 'location', 'online', 'topics', 'languages'].flatMap((k) =>
      k in now ? [[k, now[k as keyof typeof now]]] : []
    )
  );
  const [avatar] = await picture(event, now.did, now.avatar);
  const [banner] = await picture(event, now.did, now.banner);
  const pictures = { ...(avatar ? { avatar } : {}), ...(banner ? { banner } : {}) };
  const profile: Record<string, unknown> = {
    ...kept,
    ...pictures,
    displayName: now.displayName ?? now.handle,
    joinPolicy: now.joinPolicy ?? 'approval'
  };
  // Shown only to someone signed in, as everyone here is.
  return { meta: now.meta ?? '', profile };
}

type Profile = Record<string, unknown>;

/** What `write` comes to, given the group's session in its meta space and its profile now: or why not. */
async function asMeta(
  event: Event,
  group: string,
  write: (session: GroupSession, profile: Profile) => Promise<Done>
) {
  const { meta, profile } = await profileNow(event, group);
  return withGroup(event, group, meta, (session) => write(session, profile));
}

/** The profile put back whole, as the group. */
const putProfile = (session: GroupSession, profile: Profile) =>
  session.write('com.atproto.space.putRecord', {
    collection: PROFILE,
    rkey: 'self',
    record: { $type: PROFILE, ...profile }
  });

/** What a group's settings change: each drawn again after, or the host's refusal said. */
const all = {
  picture: async (event) => {
    const { group, form } = await asked(event);
    const slot = text(form, 'slot') === 'banner' ? 'banner' : 'avatar';
    const file = form.get('file');
    if (!(file instanceof File) || file.size === 0)
      return fail(400, { failed: m.settings_picture_needed() });
    const bytes = await file.arrayBuffer();
    return asMeta(event, group, async (session, profile) => {
      const up = await session.upload(bytes, file.type || 'application/octet-stream');
      if (!('blob' in up)) return up;
      return putProfile(session, { ...profile, [slot]: up.blob });
    });
  },
  unpicture: async (event) => {
    const { group, form } = await asked(event);
    const slot = text(form, 'slot') === 'banner' ? 'banner' : 'avatar';
    return asMeta(event, group, async (session, profile) => {
      const kept = { ...profile };
      delete kept[slot];
      return putProfile(session, kept);
    });
  },
  visibility: async (event) => {
    const { group, form } = await asked(event);
    const { group: now, spaces } = await xrpc<typeof getGroup.main>(
      event,
      'fyi.opensocial.getGroup',
      { group }
    );
    const meta = spaces?.find((s) => s.uri === now.meta);
    // The rest of the policy is kept: only whether anyone may read it changes.
    const policy = {
      grants: [],
      ...meta?.policy,
      $type: 'fyi.opensocial.policy',
      publicRead: text(form, 'seen') === 'public'
    };
    return withGroup(event, group, now.meta ?? '', (session) =>
      session.write('com.atproto.space.updateSpace', { policy })
    );
  },
  profile: async (event) => {
    const { group, form } = await asked(event);
    return asMeta(event, group, async (session, now) => {
      // The form's own fields replace what is there: one left empty goes.
      const profile: Record<string, unknown> = {
        ...now,
        displayName: text(form, 'displayName'),
        joinPolicy: text(form, 'joinPolicy') || now.joinPolicy
      };
      for (const k of ['description', 'url']) {
        if (text(form, k)) profile[k] = text(form, k);
        else delete profile[k];
      }
      if (form.has('country')) {
        const place = whereFrom(form, now.location as Place | undefined);
        if (typeof place === 'string') return fail(400, { failed: place });
        if (place) profile.location = place;
        else delete profile.location;
      }
      if (form.has('topics')) {
        const topics = [
          ...new Set(
            text(form, 'topics')
              .split(',')
              .map((t) => t.trim())
              .filter(Boolean)
          )
        ];
        if (topics.length > MAX_TOPICS)
          return fail(400, { failed: m.settings_topics_too_many({ count: MAX_TOPICS }) });
        if (topics.some((t) => [...segments.segment(t)].length > TOPIC_AT_MOST))
          return fail(400, { failed: m.settings_topic_too_long({ count: TOPIC_AT_MOST }) });
        if (topics.length) profile.topics = topics.map((topic) => ({ topic }));
        else delete profile.topics;
      }
      // Each sent with a blank first, so a form with none chosen still says so.
      if (form.has('language')) {
        const languages = [...new Set(form.getAll('language').map(String).filter(Boolean))].slice(
          0,
          MAX_LANGUAGES
        );
        if (languages.length) profile.languages = languages.map((language) => ({ language }));
        else delete profile.languages;
      }
      if (form.has('online')) {
        if (form.getAll('online').includes('on')) profile.online = true;
        else delete profile.online;
      }
      return putProfile(session, profile);
    });
  },
  link: async (event) => {
    const { group, form } = await asked(event);
    // Added after those it has, with its title if it has one, and its
    // site's icon, kept as the group's own, if the site has one.
    const url = text(form, 'url');
    const title = text(form, 'title');
    return asMeta(event, group, async (session, now) => {
      const had = (now.links as object[] | undefined) ?? [];
      if (had.length >= MAX_LINKS)
        return fail(400, { failed: m.settings_links_too_many({ count: MAX_LINKS }) });
      const bytes = await iconOf(url);
      const up = bytes ? await session.upload(bytes, 'image/png') : null;
      const icon = up && 'blob' in up ? { icon: up.blob } : {};
      const links = [...had, { url, ...(title ? { title } : {}), ...icon }];
      return putProfile(session, { ...now, links });
    });
  },
  unlink: async (event) => {
    const { group, form } = await asked(event);
    const at = Number(text(form, 'index'));
    return asMeta(event, group, async (session, now) => {
      const links = ((now.links as object[] | undefined) ?? []).filter((_, i) => i !== at);
      const { links: _, ...rest } = now;
      return putProfile(session, links.length ? { ...rest, links } : rest);
    });
  },
  rule: async (event) => {
    const { group, form } = await asked(event);
    const record = { $type: RULE, title: text(form, 'title'), text: text(form, 'text') };
    return asMeta(event, group, (session) =>
      session.write('com.atproto.space.createRecord', { collection: RULE, record })
    );
  },
  unrule: async (event) => {
    const { group, form } = await asked(event);
    const rkey = text(form, 'rkey');
    return asMeta(event, group, (session) =>
      session.write('com.atproto.space.deleteRecord', { collection: RULE, rkey })
    );
  },
  role: async (event) => {
    const { group, form } = await asked(event);
    const key = text(form, 'role');
    const { roles, bindings } = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', {
      group: group as Did
    });
    const was = roles?.find((r) => r.key === key);
    // What the form does not show stays as it was: among its scopes, any
    // that is no action's.
    const named = new Set(Object.values(SCOPES));
    const kept =
      bindings
        ?.find((b) => b.role === key)
        ?.scopes?.map((s) => s.scope)
        .filter((s) => !named.has(s)) ?? [];
    const actions = form.getAll('action').map((a) => SCOPES[String(a) as keyof typeof SCOPES]);
    const role = {
      $type: 'fyi.opensocial.role',
      ...(was?.description ? { description: was.description } : {}),
      ...(was?.bot ? { bot: true } : {}),
      displayName: text(form, 'name'),
      default: form.get('default') === 'on',
      scopes: [...new Set([...kept, ...actions])],
      assignable: form.getAll('assignable').map(String),
      createdAt: new Date().toISOString()
    };
    return applyAsGroup(event, group, membersOf(group), [
      {
        action: was ? 'update' : 'create',
        collection: 'fyi.opensocial.role',
        rkey: key,
        value: role
      }
    ]);
  },
  unrole: async (event) => {
    const { group, form } = await asked(event);
    const key = text(form, 'role');
    return applyAsGroup(event, group, membersOf(group), [
      { action: 'delete', collection: 'fyi.opensocial.role', rkey: key }
    ]);
  },
  revoke: async (event) => {
    const { group, form } = await asked(event);
    return act(event, 'fyi.opensocial.revokeGroupToken', { group, tokenId: text(form, 'tokenId') });
  },
  password: async (event) => {
    const { group, form } = await asked(event);
    const res = await event.fetch('/xrpc/fyi.opensocial.createAppPassword', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        group,
        name: text(form, 'name'),
        privileged: form.get('privileged') === 'on'
      })
    });
    const answer = await res.json().catch(() => null);
    if (!res.ok) return fail(res.status, { failed: refusal(answer, res.status) });
    return { made: { name: answer.name as string, password: answer.password as string } };
  },
  key: async (event) => {
    const { group, form } = await asked(event);
    return act(event, 'fyi.opensocial.addRecoveryKey', { group, key: text(form, 'key') });
  },
  unkey: async (event) => {
    const { group, form } = await asked(event);
    return act(event, 'fyi.opensocial.removeRecoveryKey', { group, key: text(form, 'key') });
  },
  unpassword: async (event) => {
    const { group, form } = await asked(event);
    return act(event, 'fyi.opensocial.revokeAppPassword', { group, name: text(form, 'name') });
  },
  handle: async (event) => {
    const { group, form } = await asked(event);
    const handle = text(form, 'domain').replace(/^@/, '').toLowerCase();
    return newHandle(event, group, handle);
  },
  unhandle: async (event) => {
    const { group, form } = await asked(event);
    return newHandle(event, group, text(form, 'address'));
  },
  deactivate: async (event) => {
    const { group } = await asked(event);
    // Off, its page is where it is switched back on.
    return (
      (await act(event, 'fyi.opensocial.deactivateGroup', { group })) ??
      redirect(303, `/g/${event.params.handle}`)
    );
  },
  activate: async (event) => {
    const { group } = await asked(event);
    return act(event, 'fyi.opensocial.activateGroup', { group });
  },
  define: async (event) => {
    const { group, form } = await asked(event);
    const label = text(form, 'label').toLowerCase().replace(/\s+/g, '-');
    return act(event, 'fyi.opensocial.putLabelDefinition', {
      group,
      label,
      name: text(form, 'name'),
      lang: getLocale(),
      description: text(form, 'description'),
      severity: text(form, 'severity') || 'inform',
      blurs: text(form, 'blurs') || 'none',
      defaultSetting: text(form, 'defaultSetting') || 'warn'
    });
  },
  undefine: async (event) => {
    const { group, form } = await asked(event);
    return act(event, 'fyi.opensocial.deleteLabelDefinition', {
      group,
      label: text(form, 'label')
    });
  },
  site: async (event) => {
    const { group, form } = await asked(event);
    const files = await siteFiles(form);
    if (!files.length) return fail(400, { failed: m.settings_site_folder_needed() });
    // A 404.html at the top of the folder is its page for addresses not
    // found, as static hosts take it.
    const notFound = files.some((f) => f.path === '404.html') ? { notFound: '404.html' } : {};
    const fallback = text(form, 'fallback') ? { fallback: text(form, 'fallback') } : {};
    const reads = form.getAll('reads').map((method) => ({ method: String(method) }));
    const acts = form.getAll('acts').map((method) => ({ method: String(method) }));
    // Each one per line: a space as `<type>` or `<type>/<key>`; a write as
    // that and `:<collection>`.
    const [spaceLines, writeLines] = ['spaces', 'writes'].map((key) =>
      text(form, key)
        .split('\n')
        .map((l) => l.trim())
        .filter(Boolean)
    );
    const spaces = [...new Set(spaceLines)].map(spaceOf);
    if (spaces.some((s) => !s)) return fail(400, { failed: m.settings_site_spaces_bad() });
    const writes = writeLines.map((line) => {
      const at = line.indexOf(':');
      const space = at < 0 ? undefined : spaceOf(line.slice(0, at));
      const collection = line.slice(at + 1).trim();
      return space && collection ? { ...space, collection } : undefined;
    });
    if (writes.some((w) => !w)) return fail(400, { failed: m.settings_site_writes_bad() });
    return act(event, 'fyi.opensocial.putSite', {
      group,
      files,
      ...notFound,
      ...fallback,
      ...(reads.length ? { reads } : {}),
      ...(acts.length ? { acts } : {}),
      ...(spaces.length ? { spaces } : {}),
      ...(writes.length ? { writes } : {}),
      ...(form.has('inbox') ? { inbox: true } : {})
    });
  },
  unsite: async (event) => {
    const { group } = await asked(event);
    return act(event, 'fyi.opensocial.deleteSite', { group });
  },
  domain: async (event) => {
    const { group, form } = await asked(event);
    const res = await event.fetch('/xrpc/fyi.opensocial.putSiteDomain', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ group })
    });
    if (res.ok) return;
    const domain = text(form, 'domain');
    return fail(res.status, {
      failed: await refused(res, {
        NotReaching: () => m.settings_domain_not_reaching({ domain }),
        NoOwnDomain: m.settings_domain_no_own
      })
    });
  },
  undomain: async (event) => {
    const { group } = await asked(event);
    return act(event, 'fyi.opensocial.deleteSiteDomain', { group });
  },
  delete: async (event) => {
    const { group } = await asked(event);
    // Gone, there is no page of it to come back to.
    return (await act(event, 'fyi.opensocial.deleteGroup', { group })) ?? redirect(303, '/');
  }
} satisfies Record<string, Action<{ handle: string }>>;

/** A file's bytes as text, where they are UTF-8: kept as they are, a byte-order mark included. */
const UTF8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

/**
 * The files of a folder chosen to publish, as `putSite` takes them, each
 * at its path under the folder (`paths`, as the browser gives them, in
 * the files' order, or else each file's name); those whose names start
 * with a dot left out, as the command leaves them. Each as text if it is
 * UTF-8, else as bytes.
 */
async function siteFiles(form: FormData) {
  const paths: unknown = JSON.parse(text(form, 'paths') || '[]');
  const given = form.getAll('files').filter((f): f is File => f instanceof File && f.name !== '');
  const named = given.map((file, i) => ({
    file,
    path: (Array.isArray(paths) && typeof paths[i] === 'string' && paths[i]) || file.name
  }));
  // The folder's own name is no part of the site's paths.
  const top = named[0]?.path.split('/')[0] ?? '';
  const under = named.every((n) => n.path.startsWith(`${top}/`));
  const files = [];
  for (const { file, path: whole } of named) {
    const path = under ? whole.slice(top.length + 1) : whole;
    if (path.split('/').some((s) => s.startsWith('.'))) continue;
    const bytes = new Uint8Array(await file.arrayBuffer());
    try {
      files.push({ path, text: UTF8.decode(bytes) });
    } catch {
      const $bytes = Buffer.from(bytes).toString('base64').replace(/=+$/, '');
      files.push({ path, bytes: { $bytes } });
    }
  }
  return files;
}

/** A space as typed, `<type>` or `<type>/<key>`; nothing when either part is empty. */
function spaceOf(line: string): { space: string; key?: string } | undefined {
  const at = line.indexOf('/');
  const space = (at < 0 ? line : line.slice(0, at)).trim();
  const key = at < 0 ? undefined : line.slice(at + 1).trim();
  if (!space || key === '') return;
  return key === undefined ? { space } : { space, key };
}

/** The writes named, as a page's actions. */
const pick = <K extends keyof typeof all>(...names: K[]) =>
  Object.fromEntries(names.map((n) => [n, all[n]])) as Pick<typeof all, K>;
/**
 * Gives the group `handle`, and goes to its settings by it, as the old
 * one names it no more; or says why not, in the reader's words where
 * the host's refusal is one they can act on.
 */
async function newHandle(event: Event, group: string, handle: string) {
  const res = await event.fetch('/xrpc/fyi.opensocial.updateGroupHandle', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ group, handle })
  });
  if (res.ok) redirect(303, `/g/${handle}/settings`);
  return fail(res.status, {
    failed: await refused(res, {
      InvalidHandle: () => m.settings_handle_invalid({ handle }),
      NotPointing: () => m.settings_handle_not_pointing({ handle }),
      HandleNotAvailable: () => m.settings_handle_taken({ handle })
    })
  });
}

export const profileActions = pick(
  'profile',
  'handle',
  'unhandle',
  'link',
  'unlink',
  'picture',
  'unpicture',
  'visibility',
  'deactivate',
  'delete'
);
/** What a group switched off is: switched back on, or deleted, from its own page. */
export const switchedOffActions = pick('activate', 'delete');
export const ruleActions = pick('rule', 'unrule');
export const labelActions = pick('define', 'undefine');
export const roleActions = pick('role', 'unrole');
export const appActions = pick('revoke', 'password', 'unpassword');
export const recoveryActions = pick('key', 'unkey');
export const siteActions = pick('site', 'unsite', 'domain', 'undomain');
