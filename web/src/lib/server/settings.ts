// A group's settings, each part a page of its own under
// /g/<handle>/settings: what each loads, and the writes its forms make,
// which each page's +page.server.ts takes the ones of it wants.

import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getGroup from '#lib/lexicons/org/atgrp/internal/getGroup.ts';
import type * as listAppPasswords from '#lib/lexicons/org/atgrp/internal/listAppPasswords.ts';
import type * as listGroupTokens from '#lib/lexicons/org/atgrp/internal/listGroupTokens.ts';
import type * as listRecoveryKeys from '#lib/lexicons/org/atgrp/internal/listRecoveryKeys.ts';
import { fail, redirect, type Action } from '@sveltejs/kit';
import { act, xrpc, type Answer } from '#lib/data.ts';
import { refusal, refused } from '#lib/errors.ts';
import { m } from '#lib/paraglide/messages.js';
import { getLocale } from '#lib/paraglide/runtime.js';
import { applyAsGroup, membersOf, withGroup, type Done, type GroupSession } from '#lib/asGroup.ts';
import { SCOPES, SETS } from '#lib/actions.ts';

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
  const view = await xrpc<typeof getGroup.main>(event, 'org.atgrp.internal.getGroup', {
    group
  });
  const { configures, issues } = changes(view);
  const passwords =
    configures || issues
      ? await xrpc<typeof listAppPasswords.main>(event, 'org.atgrp.internal.listAppPasswords', {
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
  xrpc<typeof listGroupTokens.main>(event, 'org.atgrp.internal.listGroupTokens', {
    group: group as Did
  });

/** The group's recovery keys, or null where the host does not keep them. */
export const recoveryOf = (event: Event, group: string) =>
  xrpc<typeof listRecoveryKeys.main>(event, 'org.atgrp.internal.listRecoveryKeys', {
    group: group as Did
  }).catch(() => null);

async function asked(event: Event & { request: Request }) {
  return { group: await groupOf(event), form: await event.request.formData() };
}

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();

const ABOUT = 'org.atgrp.about';
const CONFIG = 'org.atgrp.config';
const RULE = 'org.atgrp.rule';
type Did = `did:${string}:${string}`;

/** A role ID as the host takes one: label value syntax, at least three characters. */
const ROLE_ID = /^[a-z-]{3,64}$/;

/**
 * A picture the about record names by its CID, as the record holds it:
 * its type and size as the host serves it.
 */
async function picture(event: Event, did: string, cid?: string) {
  if (!cid) return undefined;
  const res = await event.fetch(`/img/${did}/${cid}`);
  if (!res.ok) return undefined;
  const size = (await res.arrayBuffer()).byteLength;
  return { $type: 'blob', ref: { $link: cid }, mimeType: res.headers.get('content-type'), size };
}

type About = Record<string, unknown>;

/**
 * The group's meta space, its about record there as it is now, to put
 * back whole with what is changed (what the form does not show is kept),
 * and how people join, as its config record says.
 */
async function aboutNow(event: Event, group: string) {
  const { group: now } = await xrpc<typeof getGroup.main>(event, 'org.atgrp.internal.getGroup', {
    group: group as Did
  });
  const avatar = await picture(event, now.did, now.avatar);
  const about: About = {
    ...(now.description ? { description: now.description } : {}),
    ...(now.website ? { website: now.website } : {}),
    ...(avatar ? { avatar } : {}),
    displayName: now.displayName ?? now.handle
  };
  // Shown only to someone signed in, as everyone here is.
  return { meta: now.meta ?? '', about, joinPolicy: now.joinPolicy ?? 'approval' };
}

/** What `write` comes to, given the group's session in its meta space and its about record now: or why not. */
async function asMeta(
  event: Event,
  group: string,
  write: (session: GroupSession, about: About, joinPolicy: string) => Promise<Done>
) {
  const { meta, about, joinPolicy } = await aboutNow(event, group);
  return withGroup(event, group, meta, (session) => write(session, about, joinPolicy));
}

/** The about record put back whole, as the group. */
const putAbout = (session: GroupSession, about: About) =>
  session.write('com.atproto.space.putRecord', {
    collection: ABOUT,
    rkey: 'self',
    record: { $type: ABOUT, ...about }
  });

/** What a group's settings change: each drawn again after, or the host's refusal said. */
const all = {
  picture: async (event) => {
    const { group, form } = await asked(event);
    const file = form.get('file');
    if (!(file instanceof File) || file.size === 0)
      return fail(400, { failed: m.settings_picture_needed() });
    const bytes = await file.arrayBuffer();
    return asMeta(event, group, async (session, about) => {
      const up = await session.upload(bytes, file.type || 'application/octet-stream');
      if (!('blob' in up)) return up;
      return putAbout(session, { ...about, avatar: up.blob });
    });
  },
  unpicture: async (event) => {
    const { group } = await asked(event);
    return asMeta(event, group, async (session, about) => {
      const kept = { ...about };
      delete kept.avatar;
      return putAbout(session, kept);
    });
  },
  visibility: async (event) => {
    const { group, form } = await asked(event);
    const { group: now, spaces } = await xrpc<typeof getGroup.main>(
      event,
      'org.atgrp.internal.getGroup',
      { group }
    );
    const meta = spaces?.find((s) => s.uri === now.meta);
    // The rest of the policy is kept: only whether anyone may read it changes.
    const policy = {
      grants: [],
      ...meta?.policy,
      $type: 'org.atgrp.policy',
      publicRead: text(form, 'seen') === 'public'
    };
    return withGroup(event, group, now.meta ?? '', (session) =>
      session.write('com.atproto.space.updateSpace', { policy })
    );
  },
  profile: async (event) => {
    const { group, form } = await asked(event);
    return asMeta(event, group, async (session, now, joinPolicy) => {
      // The form's own fields replace what is there: one left empty goes.
      const about: About = { ...now, displayName: text(form, 'displayName') };
      for (const k of ['description', 'website']) {
        if (text(form, k)) about[k] = text(form, k);
        else delete about[k];
      }
      const failed = await putAbout(session, about);
      // How people join is its config record, written only when it changes.
      const policy = text(form, 'joinPolicy') || joinPolicy;
      if (failed || policy === joinPolicy) return failed;
      return session.write('com.atproto.space.putRecord', {
        collection: CONFIG,
        rkey: 'self',
        record: { $type: CONFIG, joinPolicy: policy }
      });
    });
  },
  rule: async (event) => {
    const { group, form } = await asked(event);
    const record = { $type: RULE, title: text(form, 'title'), text: text(form, 'text') };
    return withGroup(event, group, membersOf(group), (session) =>
      session.write('com.atproto.space.createRecord', { collection: RULE, record })
    );
  },
  unrule: async (event) => {
    const { group, form } = await asked(event);
    const rkey = text(form, 'rkey');
    return withGroup(event, group, membersOf(group), (session) =>
      session.write('com.atproto.space.deleteRecord', { collection: RULE, rkey })
    );
  },
  role: async (event) => {
    const { group, form } = await asked(event);
    const key = text(form, 'role');
    const { roles, bindings } = await xrpc<typeof getGroup.main>(
      event,
      'org.atgrp.internal.getGroup',
      {
        group: group as Did
      }
    );
    const was = roles?.find((r) => r.key === key);
    if (!was && !ROLE_ID.test(key)) return fail(400, { failed: m.settings_role_id_invalid() });
    // The form shows the group's permission sets and the actions; any
    // other permission it does not show stays as it was.
    const named = new Set([...Object.values(SCOPES), ...SETS.map((s) => s.scope)]);
    const kept =
      bindings
        ?.find((b) => b.role === key)
        ?.permissions?.map((s) => s.scope)
        .filter((s) => !named.has(s)) ?? [];
    const sets = form.getAll('set').map((k) => SETS.find((s) => s.key === k)?.scope);
    const actions = form.getAll('action').map((a) => SCOPES[String(a) as keyof typeof SCOPES]);
    const chosen = [...sets, ...actions].filter((s) => s !== undefined);
    const name = text(form, 'name');
    const role = {
      $type: 'org.atgrp.role',
      ...(was?.description ? { description: was.description } : {}),
      ...(was?.bot ? { bot: true } : {}),
      ...(name ? { displayName: name } : {}),
      default: form.get('default') === 'on',
      permissions: [...new Set([...kept, ...chosen])],
      canAssign: form.getAll('canAssign').map(String)
    };
    return applyAsGroup(event, group, membersOf(group), [
      {
        action: was ? 'update' : 'create',
        collection: 'org.atgrp.role',
        rkey: key,
        value: role
      }
    ]);
  },
  unrole: async (event) => {
    const { group, form } = await asked(event);
    const key = text(form, 'role');
    return applyAsGroup(event, group, membersOf(group), [
      { action: 'delete', collection: 'org.atgrp.role', rkey: key }
    ]);
  },
  revoke: async (event) => {
    const { group, form } = await asked(event);
    return act(event, 'org.atgrp.internal.revokeGroupToken', {
      group,
      tokenId: text(form, 'tokenId')
    });
  },
  password: async (event) => {
    const { group, form } = await asked(event);
    const res = await event.fetch('/xrpc/org.atgrp.internal.createAppPassword', {
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
    return act(event, 'org.atgrp.internal.addRecoveryKey', { group, key: text(form, 'key') });
  },
  unkey: async (event) => {
    const { group, form } = await asked(event);
    return act(event, 'org.atgrp.internal.removeRecoveryKey', {
      group,
      key: text(form, 'key')
    });
  },
  unpassword: async (event) => {
    const { group, form } = await asked(event);
    return act(event, 'org.atgrp.internal.revokeAppPassword', {
      group,
      name: text(form, 'name')
    });
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
      (await act(event, 'org.atgrp.internal.deactivateGroup', { group })) ??
      redirect(303, `/g/${event.params.handle}`)
    );
  },
  activate: async (event) => {
    const { group } = await asked(event);
    return act(event, 'org.atgrp.internal.activateGroup', { group });
  },
  define: async (event) => {
    const { group, form } = await asked(event);
    const label = text(form, 'label').toLowerCase().replace(/\s+/g, '-');
    return act(event, 'org.atgrp.internal.putLabelDefinition', {
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
    return act(event, 'org.atgrp.internal.deleteLabelDefinition', {
      group,
      label: text(form, 'label')
    });
  },
  domain: async (event) => {
    const { group, form } = await asked(event);
    const res = await event.fetch('/xrpc/org.atgrp.internal.putGroupDomain', {
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
    return act(event, 'org.atgrp.internal.deleteGroupDomain', { group });
  },
  delete: async (event) => {
    const { group } = await asked(event);
    // Gone, there is no page of it to come back to.
    return (await act(event, 'org.atgrp.internal.deleteGroup', { group })) ?? redirect(303, '/');
  }
} satisfies Record<string, Action<{ handle: string }>>;

/** The writes named, as a page's actions. */
const pick = <K extends keyof typeof all>(...names: K[]) =>
  Object.fromEntries(names.map((n) => [n, all[n]])) as Pick<typeof all, K>;
/**
 * Gives the group `handle`, and goes to its settings by it, as the old
 * one names it no more; or says why not, in the reader's words where
 * the host's refusal is one they can act on.
 */
async function newHandle(event: Event, group: string, handle: string) {
  const res = await event.fetch('/xrpc/org.atgrp.internal.updateGroupHandle', {
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
  'picture',
  'unpicture',
  'visibility',
  'deactivate',
  'domain',
  'undomain',
  'delete'
);
/** What a group switched off is: switched back on, or deleted, from its own page. */
export const switchedOffActions = pick('activate', 'delete');
export const ruleActions = pick('rule', 'unrule');
export const labelActions = pick('define', 'undefine');
export const roleActions = pick('role', 'unrole');
export const appActions = pick('revoke', 'password', 'unpassword');
export const recoveryActions = pick('key', 'unkey');
