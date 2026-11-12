// The page the host serves around a group's site that asks for live data
// (crates/group-host/src/sites.rs): an atproto OAuth client of its own at
// the group's address, holding the visitor's session. The site's page,
// sandboxed in its frame, is all a visitor sees; it asks this page, by
// `os` (crates/group-host/src/site_kit.js), for a read the site declared,
// made as the visitor through their PDS, and to sign them in or out.
//
// Signing in is the console's own dialog, with its typeahead, asking only
// for a handle: a page may suggest one, but where a visitor is sent is
// what they see and confirm here. Their password is only ever typed at
// their own PDS.

import { BrowserOAuthClient, type OAuthSession } from '@atproto/oauth-client-browser';
import { mount } from 'svelte';
import consoleStyles from '../src/app.css?inline';
import { SIGN_IN } from '../src/lib/signIn.ts';
import SiteSignIn from './SiteSignIn.svelte';
import { signing } from './signing.svelte.ts';

type Config = {
  group: string;
  handle: string;
  reads: string[];
  aud: string;
  handleResolver: string;
  plc: string;
  allowHttp: boolean;
  signUpAt: string;
  console: string;
};
type Message = {
  os?: number;
  type?: string;
  id?: number;
  method?: string;
  params?: object;
  hint?: string;
  path?: string;
  title?: string;
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
@media (prefers-color-scheme: dark) { #${SIGN_IN} { color-scheme: dark; } }
#os-page { display: block; width: 100%; height: 100%; border: 0; }`;
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

/** A read of the host's, as the visitor, through their PDS. */
async function ask(method: string, params: object) {
  if (!session) throw new Refused('sign in', 401);
  const query = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v !== undefined) query.append(k, String(v));
  const res = await session.fetchHandler(`/xrpc/${method}?${query}`, {
    headers: { 'atproto-proxy': config.aud }
  });
  const body = await res.json().catch(() => null);
  if (!res.ok) throw new Refused(body?.message ?? res.statusText, res.status);
  return body;
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
const changed = () => frame.contentWindow?.postMessage({ os: 1, type: 'changed', me: me() }, '*');

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
function follow({ path, title }: Message) {
  if (typeof path !== 'string' || !path.startsWith('/_site/')) return;
  history.replaceState(null, '', path.slice('/_site'.length));
  document.title = (typeof title === 'string' && title.trim().slice(0, 200)) || config.handle;
}

addEventListener('message', async (e) => {
  if (e.source !== frame.contentWindow) return;
  const m: Message = e.data;
  if (!m || m.os !== 1) return;
  if (m.type === 'at') return follow(m);
  await ready;
  const reply = (body: object) =>
    frame.contentWindow?.postMessage({ os: 1, type: 'reply', id: m.id, ...body }, '*');
  switch (m.type) {
    case 'signIn':
      return openSignIn(m.hint);
    case 'signOut':
      await session?.signOut().catch(() => {});
      session = null;
      viewer = {};
      return changed();
    case 'group':
      return reply({ value: { did: config.group, handle: config.handle } });
    case 'me':
      return reply({ value: me() });
    case 'call': {
      const method = String(m.method);
      if (!config.reads.includes(method))
        return reply({ error: `${method} is none of the reads this site declares` });
      if (!session) return reply({ error: 'sign in', locked: true, reason: 'sign in' });
      try {
        return reply({ value: await ask(method, m.params ?? {}) });
      } catch (err) {
        const locked = err instanceof Refused && (err.status === 401 || err.status === 403);
        return reply({
          error: err instanceof Error ? err.message : String(err),
          ...(locked ? { locked: true, reason: 'not for you' } : {})
        });
      }
    }
  }
});

// The page of the site loads now, this page listening; back from signing
// in, once it is known which page that started from.
if (back) ready.then((start) => (frame.src = start));
else frame.src = asked;
