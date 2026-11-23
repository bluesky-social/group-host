import {
  BlobPermission,
  IncludeScope,
  RepoPermission,
  RpcPermission,
  ScopePermissionsTransition,
  SpacePermission
} from '@atproto/oauth-scopes';
import { m } from '#lib/paraglide/messages.js';

/**
 * What an app asking for some scopes could do, said as an account's own
 * PDS says it on its consent page: a row for each kind of thing (email,
 * identity, the account, Bluesky, chat, each permission set, the
 * repository, calling services, spaces), in its order and with its
 * words, read with the same parser. Each row's details are the tables
 * the PDS opens under its "?". Said of a group: the app would act as it.
 */

export type Kind =
  | 'email'
  | 'identity'
  | 'account'
  | 'bluesky'
  | 'chat'
  | 'set'
  | 'repository'
  | 'authenticate'
  | 'spaces';

/** Which of a collection's writes are let. */
export type Ticks = { create: boolean; update: boolean; delete: boolean };
/** A collection, or any (`*`), and the writes to it. */
export type Data = { collection: string; ticks: Ticks };
/** The spaces one row of space scopes reaches, and what it may do in them. */
export type Spaces = {
  /** Its type, or any (`*`). */
  type: string;
  /** Whose spaces: a handle (`@riders.groups.test`), a DID, or any (`*`). */
  owner: string;
  read: boolean;
  readSelf: boolean;
  manage: boolean;
  data: Data[];
};

export type Permission = {
  kind: Kind;
  title: string;
  /** A line on what it lets the app do. */
  says: string;
  /** What the details say first, above their tables. */
  about?: string;
  data?: Data[];
  /** Whether it may upload files. */
  files?: boolean;
  /** The methods it may call, each with the service it calls. */
  calls?: { method: string; service: string }[];
  spaces?: Spaces[];
};

/** A permission set's title and what it says it is for, as the host gives them. */
export type Label = { scope: string; title?: string; text: string };

/** The PDS's rows for `scopes`, asked for as the group with `handle` and `did`. */
export function permissions(
  scopes: string[],
  labels: Label[],
  group: { did: string; handle: string }
): Permission[] {
  const p = new ScopePermissionsTransition(scopes);
  const out: Permission[] = [];
  const email = (['manage', 'read'] as const).find((action) =>
    p.allowsAccount({ attr: 'email', action })
  );
  if (email)
    out.push({
      kind: 'email',
      title: m.permissions_email_title(),
      says: email === 'manage' ? m.permissions_email_manage() : m.permissions_email_read()
    });
  if (p.allowsIdentity({ attr: '*' }))
    out.push({
      kind: 'identity',
      title: m.permissions_identity_title(),
      says: m.permissions_identity_full()
    });
  else if (p.allowsIdentity({ attr: 'handle' }))
    out.push({
      kind: 'identity',
      title: m.permissions_identity_title(),
      says: m.permissions_identity_handle()
    });
  if (p.allowsAccount({ attr: 'status', action: 'manage' }))
    out.push({
      kind: 'account',
      title: m.permissions_account_title(),
      says: m.permissions_account_status()
    });

  const onlyBluesky = blueskyOnly(scopes, p);
  const broad =
    p.hasTransitionGeneric ||
    (['create', 'update', 'delete'] as const).some((action) =>
      p.allowsRepo({ collection: '*', action })
    );
  const bskyRepo = scopes.some(
    (s) =>
      s === 'transition:generic' ||
      (RepoPermission.fromString(s)?.collection.some((c) => c === '*' || bluesky(c)) ?? false)
  );
  const bskyRpc = scopes.some(privateBluesky);
  if (bskyRepo || bskyRpc) {
    const details = !broad && onlyBluesky;
    out.push({
      kind: 'bluesky',
      title: m.permissions_bluesky_title(),
      says:
        broad && bskyRpc
          ? m.permissions_bluesky_broad_private()
          : broad
            ? m.permissions_bluesky_broad()
            : bskyRepo && bskyRpc
              ? m.permissions_bluesky_parts_private()
              : bskyRepo
                ? m.permissions_bluesky_parts()
                : m.permissions_bluesky_private(),
      ...(details && {
        about: m.permissions_bluesky_about(),
        data: repoData(scopes),
        files: files(scopes, p),
        calls: calls(scopes)
      })
    });
  }
  if (p.hasTransitionChatBsky || scopes.some(chat))
    out.push({ kind: 'chat', title: m.permissions_chat_title(), says: m.permissions_chat_says() });

  for (const s of new Set(scopes)) {
    const set = IncludeScope.fromString(s);
    if (!set) continue;
    const label = labels.find((l) => l.scope === s);
    out.push({
      kind: 'set',
      title: label?.title ?? set.nsid,
      says: label?.text ?? m.permissions_set_unknown()
    });
  }

  if (!onlyBluesky) {
    const data = repoData(scopes);
    const all =
      p.hasTransitionGeneric ||
      p.allowsAccount({ attr: 'repo', action: 'manage' }) ||
      (['create', 'update', 'delete'] as const).every((action) =>
        p.allowsRepo({ collection: '*', action })
      );
    if (all)
      out.push({
        kind: 'repository',
        title: m.permissions_repository_title(),
        says: m.permissions_repository_all()
      });
    else if (data.length)
      out.push({
        kind: 'repository',
        title: m.permissions_repository_title(),
        says: m.permissions_repository_publish(),
        about: m.permissions_repository_about(),
        data,
        files: files(scopes, p)
      });
    if (p.hasTransitionGeneric)
      out.push({
        kind: 'authenticate',
        title: m.permissions_authenticate_title(),
        says: m.permissions_authenticate_any()
      });
    else {
      const c = calls(scopes);
      if (c.length)
        out.push({
          kind: 'authenticate',
          title: m.permissions_authenticate_title(),
          says: m.permissions_authenticate_other(),
          about: m.permissions_authenticate_about(),
          calls: c
        });
    }
    const spaces = spaceRows(scopes, group);
    if (spaces.length)
      out.push({
        kind: 'spaces',
        title: m.permissions_spaces_title(),
        says: m.permissions_spaces_says(),
        about: m.permissions_spaces_about(),
        spaces
      });
  }
  return out;
}

/** What the PDS warns of before an app gets so much: every space anywhere, or the whole identity. */
export function warnings(scopes: string[]): string[] {
  const p = new ScopePermissionsTransition(scopes);
  const out: string[] = [];
  if (
    scopes.some((s) => {
      const sp = SpacePermission.fromString(s);
      return sp?.spaceType === '*' && sp.authority === '*';
    })
  )
    out.push(m.permissions_warning_spaces());
  if (p.allowsIdentity({ attr: '*' })) out.push(m.permissions_warning_identity());
  return out;
}

const bluesky = (nsid: string) =>
  nsid === 'com.atproto.moderation.createReport' ||
  nsid.startsWith('app.bsky.') ||
  nsid.startsWith('chat.bsky.');
const appview = (aud: string) => aud === 'did:web:bsky.app#bsky_appview';
const PRIVATE = [
  'getPreferences',
  'block',
  'muteActor',
  'muteActorList',
  'muteThread',
  'unmuteActor',
  'unmuteActorList',
  'unmuteThread',
  'getMutes'
];

function privateBluesky(s: string) {
  if (s === 'transition:generic') return true;
  const rpc = RpcPermission.fromString(s);
  return (
    !!rpc &&
    rpc.lxm.some(
      (m) =>
        m === '*' || PRIVATE.some((p) => m === `app.bsky.actor.${p}` || m === `app.bsky.graph.${p}`)
    )
  );
}

function chat(s: string) {
  const rpc = RpcPermission.fromString(s);
  return !!rpc && !appview(rpc.aud) && rpc.lxm.some((m) => m === '*' || m.startsWith('chat.bsky.'));
}

/** Whether every repo and rpc scope is Bluesky's own, which the Bluesky row then says alone. */
function blueskyOnly(scopes: string[], p: ScopePermissionsTransition) {
  if (p.allowsAccount({ attr: 'repo', action: 'manage' })) return false;
  let one = false;
  for (const s of scopes) {
    const rpc = RpcPermission.fromString(s);
    const repo = RepoPermission.fromString(s);
    if (rpc) {
      one = true;
      if (!appview(rpc.aud) && !rpc.lxm.every((m) => m !== '*' && bluesky(m))) return false;
    } else if (repo) {
      one = true;
      if (!repo.collection.every((c) => c !== '*' && bluesky(c))) return false;
    }
  }
  return one;
}

const none = (): Ticks => ({ create: false, update: false, delete: false });

/** Each collection the repo scopes name, with the writes to it; any (`*`) lends its writes to each. */
function repoData(scopes: string[]): Data[] {
  const by = new Map<string, Ticks>();
  for (const s of scopes) {
    const repo = RepoPermission.fromString(s);
    for (const c of repo?.collection ?? []) {
      const t = by.get(c) ?? none();
      for (const a of repo!.action) t[a] = true;
      by.set(c, t);
    }
  }
  return sorted(by);
}

function sorted(by: Map<string, Ticks>): Data[] {
  const any = by.get('*');
  return [...by]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([collection, t]) => ({
      collection,
      ticks: {
        create: t.create || !!any?.create,
        update: t.update || !!any?.update,
        delete: t.delete || !!any?.delete
      }
    }));
}

const files = (scopes: string[], p: ScopePermissionsTransition) =>
  p.hasTransitionGeneric || scopes.some((s) => BlobPermission.fromString(s) !== null);

/** Each method the rpc scopes name, with the service it is called on, in words. */
function calls(scopes: string[]) {
  const by = new Map<string, Set<string>>();
  for (const s of scopes) {
    const rpc = RpcPermission.fromString(s);
    if (!rpc) continue;
    const lxm = by.get(rpc.aud) ?? new Set();
    for (const m of rpc.lxm) lxm.add(m);
    by.set(rpc.aud, lxm);
  }
  return [...by]
    .sort(([a], [b]) => a.localeCompare(b))
    .flatMap(([aud, lxm]) =>
      (lxm.has('*') ? ['*'] : [...lxm].sort()).map((method) => ({ method, service: service(aud) }))
    );
}

function service(aud: string) {
  if (aud === '*') return m.permissions_service_any();
  if (aud.startsWith('did:web:api.bsky.app#')) return m.permissions_service_bluesky();
  if (aud.startsWith('did:web:api.bsky.chat#')) return m.permissions_service_bluesky_chat();
  if (aud.startsWith('did:web:') && aud.includes('#'))
    return aud.slice('did:web:'.length, aud.indexOf('#'));
  return aud;
}

/** The space scopes, one row for each type, owner and key, their reads and writes merged. */
function spaceRows(scopes: string[], group: { did: string; handle: string }): Spaces[] {
  const rows = new Map<string, Spaces & { writes: Map<string, Ticks> }>();
  for (const s of scopes) {
    const sp = SpacePermission.fromString(s);
    if (!sp) continue;
    const owner =
      sp.authority === group.did || sp.authority === 'self' ? `@${group.handle}` : sp.authority;
    const key = `${sp.spaceType}|${owner}|${sp.skey}`;
    const row = rows.get(key) ?? {
      type: sp.spaceType,
      owner,
      read: false,
      readSelf: false,
      manage: false,
      data: [],
      writes: new Map()
    };
    rows.set(key, row);
    if (sp.action.includes('read')) row.read = true;
    if (sp.action.includes('read_self')) row.readSelf = true;
    if (sp.manage) row.manage = true;
    for (const c of sp.collection ?? []) {
      const t = row.writes.get(c) ?? none();
      for (const a of sp.action)
        if (a === 'create' || a === 'update' || a === 'delete') t[a] = true;
      row.writes.set(c, t);
    }
  }
  return [...rows]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([, { writes, ...row }]) => ({ ...row, data: sorted(writes) }));
}
