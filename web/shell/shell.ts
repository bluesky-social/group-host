// The page the host serves around a group's site that asks for live data
// (crates/group-host/src/sites.rs): an atproto OAuth client of its own at
// the group's address, holding the visitor's session. The site's page,
// sandboxed in its frame, is all a visitor sees; it asks this page, by
// `os` (kit.js beside it, the messages in protocol.ts), for a read the site declared,
// made as the visitor through their PDS, for the records of one of the
// group's spaces of a type it declared, read whole as the visitor from
// each writer's repo (@opensocial/sdk), for a write it declared, made in
// the visitor's own repo in one of the group's spaces, and to sign them in
// or out.
//
// Signing in is the console's own dialog, with its typeahead, asking only
// for a handle: a page may suggest one, but where a visitor is sent is
// what they see and confirm here. Their password is only ever typed at
// their own PDS.

import { type NsidString, XrpcResponseError, l } from '@atproto/lex';
import {
  BrowserOAuthClient,
  type OAuthSession,
  TokenInvalidError,
  TokenRefreshError,
  TokenRevokedError,
  createDidResolver
} from '@atproto/oauth-client-browser';
import { GroupSpace } from '@opensocial/sdk';
import { mount } from 'svelte';
import consoleStyles from '../src/app.css?inline';
import { SIGN_IN } from '../src/lib/signIn.ts';
import SiteSignIn from './SiteSignIn.svelte';
import { signing } from './signing.svelte.ts';
import type { Ask, Push, Refusal, Reply, Synced, Tell, Written } from './protocol.ts';

type Config = {
  group: string;
  handle: string;
  reads: string[];
  acts: string[];
  writes: { space: string; key?: string; collection: string }[];
  spaces: { space: string; key?: string }[];
  aud: string;
  handleResolver: string;
  plc: string;
  allowHttp: boolean;
  signUpAt: string;
  console: string;
};

const config: Config = JSON.parse(document.getElementById('os-config')?.textContent ?? '{}');
// The host's page names its frame; nothing else is in it.
const frame = document.getElementById('os-page') as HTMLIFrameElement;

// The console's look, for its dialog alone. This page paints nothing of
// its own: under the frame is the browser's plain canvas, so a site shows
// as it draws itself, with no other colour before it loads.
const style = document.createElement('style');
style.textContent = `${consoleStyles}
html:root:not([data-theme='light']) { color-scheme: normal; }
html, body { margin: 0; padding: 0; height: 100%; background: none; }
@media (prefers-color-scheme: dark) { #${SIGN_IN} { color-scheme: dark; } }`;
document.head.append(style);

const client = BrowserOAuthClient.load({
  clientId: `${location.origin}/client-metadata.json`,
  handleResolver: config.handleResolver,
  plcDirectoryUrl: config.plc,
  allowHttp: config.allowHttp,
  responseMode: 'query'
});

const back = location.pathname === '/_os/callback';
/** The page asked for, as the frame shows it, with its query and fragment. */
const asked = `${frame.dataset.src ?? '/_site/'}${location.search}${location.hash}`;
let session: OAuthSession | null = null;

/**
 * The visitor's session, restored, or finished as they come back from
 * signing in: where the frame goes then, the page sign-in started from.
 */
async function restore(): Promise<string> {
  try {
    const found = await (await client).init();
    session = found?.session ?? null;
    if (back) {
      const from = found && 'state' in found && typeof found.state === 'string' ? found.state : '/';
      history.replaceState(null, '', from);
      return `/_site${from}`;
    }
  } catch (e) {
    console.warn('signing in failed', e);
    if (back) {
      history.replaceState(null, '', '/');
      return '/_site/';
    }
  }
  return asked;
}

/** A read the host refused, with its status. */
class Refused extends Error {
  constructor(
    message: string,
    readonly status: number
  ) {
    super(message);
  }
}

/**
 * A read of the host's, as the visitor, through their PDS: its JSON, or,
 * for a method that answers a file (`getSpaceBlob`), the file as a `Blob`.
 */
async function ask(method: string, params: object) {
  if (!session) throw new Refused('sign in', 401);
  const query = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v !== undefined) query.append(k, String(v));
  const res = await session.fetchHandler(`/xrpc/${method}?${query}`, {
    headers: { 'atproto-proxy': config.aud }
  });
  const json = res.headers.get('content-type')?.startsWith('application/json');
  if (res.ok && !json) return await res.blob();
  const body = await res.json().catch(() => null);
  if (!res.ok) throw new Refused(body?.message ?? res.statusText, res.status);
  return body;
}

/**
 * One of the host's procedures, as the visitor, through their PDS: `input`
 * its JSON, and its answer. Refused for want of scope, as a write is.
 */
async function act(method: string, input: object) {
  if (!session) throw new Refused('sign in', 401);
  const res = await session.fetchHandler(`/xrpc/${method}`, {
    method: 'POST',
    headers: { 'atproto-proxy': config.aud, 'content-type': 'application/json' },
    body: JSON.stringify(input)
  });
  const said = await res.json().catch(() => null);
  if (res.ok) return said ?? {};
  return await unwritten(res, said);
}

/**
 * Uploads `file`, a picture, to the visitor's own PDS, for a record the
 * site writes to point to: a site that declares no writes may not, nor
 * upload anything but a picture. Answers the blob's reference.
 */
async function upload(file: unknown) {
  if (!config.writes.length) throw new Error('this site declares no writes');
  if (!(file instanceof Blob) || !file.type.startsWith('image/'))
    throw new Error('only a picture may be uploaded');
  if (!session) throw new Refused('sign in', 401);
  const res = await session.fetchHandler('/xrpc/com.atproto.repo.uploadBlob', {
    method: 'POST',
    headers: { 'content-type': file.type },
    body: file
  });
  const said = await res.json().catch(() => null);
  if (res.ok) return said.blob;
  return await unwritten(res, said);
}

/** The procedure each write is, at the visitor's PDS. */
const WRITES = {
  create: 'com.atproto.space.createRecord',
  put: 'com.atproto.space.putRecord',
  delete: 'com.atproto.space.deleteRecord'
};

/**
 * One of the group's spaces, `at://<group>/space/<type>/<skey>`, by its
 * type and key; nothing for any other URI.
 */
function groupSpace(space: string) {
  const [authority, kw, type, skey, more] = space.replace(/^at:\/\//, '').split('/');
  if (!space.startsWith('at://') || authority !== config.group || kw !== 'space') return;
  if (!type || !skey || more !== undefined) return;
  return { type, skey };
}

/**
 * Whether `ref` is a space `named` names: one of its type, and the one
 * under its key when it names one.
 */
function names(named: { space: string; key?: string }, ref: { type: string; skey: string }) {
  return named.space === ref.type && (named.key === undefined || named.key === ref.skey);
}

/**
 * Whether the site declared writes of `collection` in `space`: one of the
 * group's spaces it names with that collection.
 */
function declared(space: string, collection: string) {
  const ref = groupSpace(space);
  return !!ref && config.writes.some((w) => names(w, ref) && w.collection === collection);
}

/** A write the frame asks, its space and collection checked against the site's declaration. */
type Write = Extract<Ask, { type: 'create' | 'put' | 'delete' }>;

/**
 * A write in the visitor's own repo, in one of the group's spaces, at their
 * PDS: a record `value` of `collection`, its `$type`. Answers the record's
 * `uri` and `cid`, or nothing for a delete.
 */
async function write(m: Write): Promise<Written | undefined> {
  if (!session) throw new Refused('sign in', 401);
  const { space, collection } = m;
  const rkey = 'rkey' in m && m.rkey ? String(m.rkey) : undefined;
  const value = 'value' in m ? m.value : undefined;
  if (m.type !== 'delete' && (!value || typeof value !== 'object' || Array.isArray(value)))
    throw new Error('a record is an object');
  const body = {
    repo: session.sub,
    space,
    collection,
    ...(rkey ? { rkey } : {}),
    ...(value ? { record: { ...value, $type: collection } } : {})
  };
  const res = await session.fetchHandler(`/xrpc/${WRITES[m.type]}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body)
  });
  const said = await res.json().catch(() => null);
  if (!res.ok) return await unwritten(res, said);
  return m.type === 'delete' ? undefined : { uri: said?.uri, cid: said?.cid };
}

/** Whether a refusal of the visitor's PDS is for want of scope. */
const unscoped = (status: number, error?: string, message?: string) =>
  (status === 401 || status === 403) &&
  `${error ?? ''} ${message ?? ''}`.toLowerCase().includes('scope');

/**
 * A write or an upload the visitor's PDS refused, thrown. A sign-in from
 * before the site wrote this, refused for want of scope, is forgotten, so
 * signing in again grants it.
 */
async function unwritten(
  res: Response,
  said: { error?: string; message?: string } | null
): Promise<never> {
  if (unscoped(res.status, said?.error, said?.message)) {
    await drop();
    throw new Refused('sign in', 401);
  }
  throw new Refused(said?.message ?? res.statusText, res.status);
}

/**
 * Forgets the visitor's session, and the spaces read as them, so that the
 * frame shows nobody signed in.
 */
async function drop() {
  await session?.signOut().catch(() => {});
  session = null;
  viewer = {};
  spaces.clear();
  changed();
}

/**
 * The records the visitor reads, kept in this address's IndexedDB by the
 * key GroupSpace names them by (the member's DID, then the space's URI),
 * so that a member's next visit reads on from where this one stopped. It
 * only saves reading again: a browser that keeps nothing reads in full.
 */
const KEPT = 'spaces';
let keeping: Promise<IDBDatabase> | undefined;
const kept = () =>
  (keeping ??= new Promise((resolve, reject) => {
    const open = indexedDB.open('os-spaces', 1);
    open.onupgradeneeded = () => open.result.createObjectStore(KEPT);
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  }));

/** One request of the kept records, in a transaction of its own. */
async function keep<T>(mode: IDBTransactionMode, ask: (s: IDBObjectStore) => IDBRequest<T>) {
  const request = ask((await kept()).transaction(KEPT, mode).objectStore(KEPT));
  return await new Promise<T>((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

const store: GroupSpace.Store = {
  get: (key) =>
    keep('readonly', (s) => s.get(key) as IDBRequest<string | undefined>).catch((err) => {
      console.warn('reading kept records failed', err);
      return undefined;
    }),
  put: (key, value) =>
    keep('readwrite', (s) => s.put(value, key)).then(
      () => {},
      (err) => console.warn('keeping records failed', err)
    )
};

/** Resolves the group's DID to its host, on the PLC the host names. */
const didResolver = createDidResolver({ plcDirectoryUrl: config.plc });

/** A space of the group's, of a type this page knows only by its NSID. */
type Read = GroupSpace<l.Space<NsidString, 'any', readonly []>>;

/**
 * The group's spaces the frame has read, by URI, each as the visitor, for
 * the page's life: each read reads on from the last, and tells the frame
 * when it changes the records.
 */
const spaces = new Map<string, Read>();

/**
 * One of the group's spaces, of `type` and key `skey`, brought up to date
 * as the visitor reads it: every record held, and the writers whose host
 * failed, whose records last read are kept.
 */
async function sync(uri: string, type: string, skey: string): Promise<Synced> {
  if (!session) throw new Refused('sign in', 401);
  let space = spaces.get(uri);
  if (!space) {
    const opened = GroupSpace.open(
      l.space('any', type as NsidString, []),
      { authority: config.group as `did:${string}:${string}`, skey },
      { agent: session, store, didResolver }
    );
    opened.onChange((records) => {
      if (spaces.get(uri) !== opened) return;
      const told: Push = { os: 1, type: 'records', space: uri, records };
      frame.contentWindow?.postMessage(told, '*');
    });
    spaces.set(uri, (space = opened));
  }
  let failed;
  try {
    ({ failed } = await space.sync());
  } catch (err) {
    return await unread(err);
  }
  for (const { repo, error } of failed) console.warn(`reading ${repo} in ${uri} failed`, error);
  return {
    failed: failed.map(({ repo, error }) => ({
      repo,
      error: error instanceof Error ? error.message : String(error)
    })),
    records: space.records()
  };
}

/** The errors an error was caused by, it first. */
function* causes(err: unknown) {
  for (let e = err; e !== undefined; e = e instanceof Error ? e.cause : undefined) yield e;
}

/**
 * A read of a space that failed, thrown: refused for who is looking when
 * the authority does not let the visitor in (`SpaceNotFound`, 401, 403).
 * A session that ended, or one from before the site read this space,
 * without its scope, is forgotten, so signing in again grants it.
 */
async function unread(err: unknown): Promise<never> {
  const ended = [...causes(err)].some(
    (e) =>
      e instanceof TokenRefreshError ||
      e instanceof TokenRevokedError ||
      e instanceof TokenInvalidError ||
      (e instanceof XrpcResponseError && (e.error === 'ExpiredToken' || e.error === 'InvalidToken'))
  );
  const said = err instanceof XrpcResponseError ? err : undefined;
  if (ended || (said && unscoped(said.status, said.error, said.message))) {
    await drop();
    throw new Refused('sign in', 401);
  }
  if (said?.error === 'SpaceNotFound') throw new Refused(said.message || 'SpaceNotFound', 403);
  if (said && (said.status === 401 || said.status === 403)) throw new Refused(said.message, 403);
  throw err;
}

// Who is signed in, by their handle; a session whose server can't be
// reached any more (a dev world started again, say) is forgotten here.
let viewer: { handle?: string } = {};
async function look() {
  if (!session) return;
  try {
    const v = await ask('fyi.opensocial.getViewer', {});
    viewer = { handle: v?.actor?.handle };
  } catch (err) {
    if (!(err instanceof Refused)) {
      await session.signOut().catch(() => {});
      session = null;
    }
  }
}

/** The frame's first page, once the session is known; its asks wait for it. */
const ready = restore().then(async (start) => (await look(), start));

/** Who is looking, as `os.me()` answers it. */
const me = () => ({ signedIn: !!session, did: session?.sub, handle: viewer.handle });

/** Tells the page who is looking now. */
const changed = () => {
  const told: Push = { os: 1, type: 'changed', me: me() };
  frame.contentWindow?.postMessage(told, '*');
};

// The sign-in dialog: a handle, or making an account, then off to the
// visitor's own server, coming back to the page it was opened on.
const go = (to: string, options: { prompt?: 'create' } = {}) => {
  signing.failed = '';
  client
    .then((c) => c.signIn(to, { state: location.pathname + location.search, ...options }))
    .catch((err) => {
      signing.failed = options.prompt
        ? `We couldn't reach ${new URL(to).host} to make an account. Try again in a moment.`
        : `We couldn't find ${to}. Check your handle and try again.`;
      console.warn(err);
    });
};
mount(SiteSignIn, {
  target: document.body,
  props: {
    signUpAt: config.signUpAt,
    assets: config.console,
    onhandle: (handle: string) => go(handle),
    oncreate: () => go(config.signUpAt, { prompt: 'create' })
  }
});

function openSignIn(hint?: string) {
  const dialog = document.getElementById(SIGN_IN);
  if (session || !dialog || dialog.matches(':popover-open')) return;
  signing.failed = '';
  dialog.showPopover();
  const field = dialog.querySelector('input:not([type=hidden])');
  if (field instanceof HTMLInputElement && typeof hint === 'string') {
    field.value = hint.slice(0, 253);
    field.dispatchEvent(new Event('input', { bubbles: true }));
  }
}

/**
 * The frame's page, in the address bar and the tab, so that it is what a
 * reload or a copied link opens. The frame's own history entries already
 * make Back go back through it.
 */
function follow({ path, title }: Extract<Tell, { type: 'at' }>) {
  if (typeof path !== 'string' || !path.startsWith('/_site/')) return;
  history.replaceState(null, '', path.slice('/_site'.length));
  document.title = (typeof title === 'string' && title.trim().slice(0, 200)) || config.handle;
}

/**
 * What the frame is told of a read or a write refused: locked, with why,
 * where it was refused for who is looking (no one, or not them).
 */
function refusal(err: unknown): Refusal {
  const locked = err instanceof Refused && (err.status === 401 || err.status === 403);
  const reason = err instanceof Error && err.message === 'sign in' ? 'sign in' : 'not for you';
  return {
    error: err instanceof Error ? err.message : String(err),
    ...(locked ? { locked: true, reason } : {})
  };
}

addEventListener('message', async (e) => {
  if (e.source !== frame.contentWindow) return;
  const m: Ask | Tell = e.data;
  if (!m || m.os !== 1) return;
  if (m.type === 'at') return follow(m);
  await ready;
  if (m.type === 'signIn') return openSignIn(m.hint);
  if (m.type === 'signOut') {
    // What was kept of the spaces read as them, forgotten with them.
    const did = session?.sub;
    await drop();
    if (did)
      await keep('readwrite', (s) => s.delete(IDBKeyRange.bound(`${did} `, `${did} \uffff`))).catch(
        (err) => console.warn('forgetting kept records failed', err)
      );
    return;
  }
  const reply = (body: { value: unknown } | Refusal) => {
    const told: Reply = { os: 1, type: 'reply', id: m.id, ...body };
    frame.contentWindow?.postMessage(told, '*');
  };
  const locked: Refusal = { error: 'sign in', locked: true, reason: 'sign in' };
  switch (m.type) {
    case 'group':
      return reply({
        value: { did: config.group, handle: config.handle, console: config.console }
      });
    case 'me':
      return reply({ value: me() });
    case 'call': {
      const method = String(m.method);
      if (!config.reads.includes(method))
        return reply({ error: `${method} is none of the reads this site declares` });
      if (!session) return reply(locked);
      try {
        return reply({ value: await ask(method, m.params ?? {}) });
      } catch (err) {
        return reply(refusal(err));
      }
    }
    case 'act': {
      const method = String(m.method);
      if (!config.acts.includes(method))
        return reply({ error: `${method} is none of the acts this site declares` });
      if (!session) return reply(locked);
      try {
        return reply({ value: await act(method, m.input ?? {}) });
      } catch (err) {
        return reply(refusal(err));
      }
    }
    case 'sync': {
      const space = String(m.space);
      const ref = groupSpace(space);
      if (!ref || !config.spaces.some((s) => names(s, ref)))
        return reply({ error: `${space} is none of the spaces this site declares` });
      if (!session) return reply(locked);
      try {
        return reply({ value: await sync(space, ref.type, ref.skey) });
      } catch (err) {
        return reply(refusal(err));
      }
    }
    case 'create':
    case 'put':
    case 'delete': {
      const [space, collection] = [String(m.space), String(m.collection)];
      if (!declared(space, collection))
        return reply({
          error: `${collection} in ${space} is none of the writes this site declares`
        });
      if (!session) return reply(locked);
      try {
        return reply({ value: await write({ ...m, space, collection }) });
      } catch (err) {
        return reply(refusal(err));
      }
    }
    case 'upload':
      try {
        return reply({ value: await upload(m.file) });
      } catch (err) {
        return reply(refusal(err));
      }
  }
});

// The page of the site loads now, this page listening; back from signing
// in, once it is known which page that started from.
if (back) ready.then((start) => (frame.src = start));
else frame.src = asked;
