// What the flows share: the world they run over, signing in as a person
// does it, and an app signing in as a group, as an atproto client does.

import { expect } from '@playwright/test';
import { createHash, generateKeyPairSync, randomBytes, sign } from 'node:crypto';
import { createServer } from 'node:http';
import { serviceAuth } from '../world.mjs';

export const world = JSON.parse(process.env.E2E_WORLD);
export const who = Object.fromEntries(world.accounts.map((a) => [a.handle.split('.')[0], a]));
export const url = (path) => world.base + path;

/// Has `page.goto` to a console page wait until the page answers in the
/// browser (`data-hydrated`, which its root layout sets), so that what a
/// flow types or picks then is not put back as the page hydrates.
export const hydrating = (page) => {
  const go = page.goto.bind(page);
  page.goto = async (to, options) => {
    const res = await go(to, options);
    if (new URL(page.url()).origin === new URL(world.base).origin)
      await page.locator('html[data-hydrated]').waitFor({ state: 'attached' });
    return res;
  };
};

/// Signs in as `name` on the PDS's own page `page` has come to, and
/// approves what was asked.
export const approve = async (page, name) => {
  await page.waitForURL((u) => u.origin === new URL(world.pds).origin);
  await page.getByRole('textbox', { name: 'Password' }).fill(who[name].password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await page.getByRole('button', { name: /^(accept|authorize|allow)$/i }).click();
};

/// `name` joins the group `handle` from their own client, with service
/// auth from their PDS, as the host's method takes it: at once, where
/// anyone can join.
export const joins = async (name, handle) => {
  const { did } = await fetch(
    `${world.host}/xrpc/com.atproto.identity.resolveHandle?handle=${handle}`
  ).then((r) => r.json());
  const lxm = 'org.atgrp.requestJoin';
  const token = await serviceAuth(world.pds, who[name], world.did, lxm);
  const res = await fetch(`${world.host}/xrpc/${lxm}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${token}` },
    body: JSON.stringify({ group: did })
  });
  const said = await res.json();
  expect(said.status, JSON.stringify(said)).toBe('admitted');
};

/// Stands in for the host's handle search (org.atgrp.internal.searchActors)
/// in the browser `context`, as the typeahead service it asks would on the
/// whole network, with names and pictures the reference directory has
/// none of: the world's accounts whose handle holds what is typed, named
/// for themselves. The host's own search is tested in its own tests.
export const suggests = (context) =>
  context.route('**/xrpc/org.atgrp.internal.searchActors?*', (route) => {
    const asked = new URL(route.request().url()).searchParams;
    const q = asked.get('q').toLowerCase();
    const actors = world.accounts
      .filter((a) => a.handle.includes(q))
      .slice(0, Number(asked.get('limit')))
      .map((a) => ({
        did: a.did,
        handle: a.handle,
        displayName: a.handle[0].toUpperCase() + a.handle.split('.')[0].slice(1)
      }));
    return route.fulfill({ json: { actors } });
  });

/// The sign-in dialog, wherever it was opened from.
export const signInDialog = (page) =>
  page.getByRole('dialog', { name: 'Sign in with your Atmosphere account' });

/// Signs in to the console as `name` from the dialog, opened by the
/// header's button or by `from`: their handle, then their PDS's page,
/// which sends the browser back. Enter sends it as typed, under any
/// accounts the field suggests.
export const signIn = async (
  page,
  name,
  from = page.getByRole('banner').getByRole('button', { name: 'Sign in' })
) => {
  const back = page.url();
  await from.click();
  const dialog = signInDialog(page);
  const handle = dialog.getByRole('combobox', { name: 'Handle' });
  await handle.fill(who[name].handle);
  await handle.press('Enter');
  await approve(page, name);
  await page.waitForURL(back);
};

/// Who the header says is signed in: nothing for no one.
export const signedInAs = (page) =>
  page
    .locator('.user-menu summary .handle')
    .textContent({ timeout: 1000 })
    .then(
      (t) => t.trim(),
      () => ''
    );

const b64 = (bytes) => Buffer.from(bytes).toString('base64url');
const sha256 = (text) => b64(createHash('sha256').update(text).digest());

/// An app on this machine, signing in as a group: the loopback client
/// asking for `scope`, with a DPoP key, PKCE, and the nonce the host last
/// gave, as atproto's OAuth clients do. It listens for the browser coming
/// back on a port of its own (`listen`), as a native app does: the
/// loopback client registers its redirect on any port.
export class App {
  static REDIRECT = 'http://127.0.0.1/callback';

  /// Listens for the browser coming back; `back` is where it came to.
  async listen() {
    this.back = new Promise((done) => {
      this.server = createServer((req, res) => {
        res.end('back at the app');
        done(`http://127.0.0.1:${this.server.address().port}${req.url}`);
      });
    });
    await new Promise((r) => this.server.listen(0, '127.0.0.1', r));
    this.redirect = `http://127.0.0.1:${this.server.address().port}/callback`;
    return this;
  }

  close() {
    this.server?.close();
  }

  constructor(scope) {
    const id = new URL('http://localhost');
    id.searchParams.set('redirect_uri', App.REDIRECT);
    id.searchParams.set('scope', scope);
    this.id = id.href;
    this.keys = generateKeyPairSync('ec', { namedCurve: 'P-256' });
    this.verifier = b64(randomBytes(32));
    this.nonce = undefined;
  }

  /// A DPoP proof of `method` to `url`, for `token` if it carries one.
  proof(method, url, token) {
    const jwk = this.keys.publicKey.export({ format: 'jwk' });
    const header = { typ: 'dpop+jwt', alg: 'ES256', jwk };
    const at = new URL(url);
    const claims = {
      jti: b64(randomBytes(12)),
      htm: method,
      htu: `${at.origin}${at.pathname}`,
      iat: Math.floor(Date.now() / 1000)
    };
    if (this.nonce) claims.nonce = this.nonce;
    if (token) claims.ath = sha256(token);
    const body = `${b64(JSON.stringify(header))}.${b64(JSON.stringify(claims))}`;
    const sig = sign('sha256', Buffer.from(body), {
      key: this.keys.privateKey,
      dsaEncoding: 'ieee-p1363'
    });
    return `${body}.${b64(sig)}`;
  }

  /// `init` sent to `url` with a fresh proof, once more with the nonce the
  /// host asks for: its status and JSON.
  async send(url, init, token) {
    for (let i = 0; i < 2; i++) {
      const headers = { ...init.headers, dpop: this.proof(init.method ?? 'GET', url, token) };
      if (token) headers.authorization = `DPoP ${token}`;
      const res = await fetch(url, { ...init, headers });
      this.nonce = res.headers.get('dpop-nonce') ?? this.nonce;
      const body = await res.json().catch(() => null);
      const stale =
        body?.error === 'use_dpop_nonce' ||
        res.headers.get('www-authenticate')?.includes('use_dpop_nonce');
      if (!stale) return { status: res.status, body };
    }
    throw new Error(`${url} asked for a nonce twice`);
  }

  form(path, fields) {
    const body = new URLSearchParams({ client_id: this.id, ...fields });
    const headers = { 'content-type': 'application/x-www-form-urlencoded' };
    return this.send(`${world.host}${path}`, { method: 'POST', headers, body });
  }

  /// Pushes a request to sign in: where to send the browser.
  async push(hint) {
    const pushed = await this.form('/oauth/par', {
      response_type: 'code',
      code_challenge: sha256(this.verifier),
      code_challenge_method: 'S256',
      state: 'st4te',
      redirect_uri: this.redirect,
      ...(hint ? { login_hint: hint } : {})
    });
    expect(pushed.status, JSON.stringify(pushed.body)).toBe(201);
    const to = new URL(`${world.host}/oauth/authorize`);
    to.searchParams.set('client_id', this.id);
    to.searchParams.set('request_uri', pushed.body.request_uri);
    return to.href;
  }

  /// The tokens the code the browser came back with is worth.
  async exchange(back) {
    const code = new URL(back).searchParams.get('code');
    const said = await this.form('/oauth/token', {
      grant_type: 'authorization_code',
      code,
      redirect_uri: this.redirect,
      code_verifier: this.verifier
    });
    expect(said.status, JSON.stringify(said.body)).toBe(200);
    return said.body;
  }

  /// Calls the procedure `nsid` on the host with `token`.
  call(nsid, token, input) {
    const init = {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(input)
    };
    return this.send(`${world.host}/xrpc/${nsid}`, init, token);
  }
}

/// A new P-256 key: its private half, and its public half as a did:key.
const p256 = () => {
  const { privateKey, publicKey } = generateKeyPairSync('ec', { namedCurve: 'P-256' });
  const point = publicKey.export({ format: 'der', type: 'spki' }).subarray(-65);
  const compressed = Buffer.concat([Buffer.from([point[64] & 1 ? 3 : 2]), point.subarray(1, 33)]);
  const bytes = Buffer.concat([Buffer.from([0x80, 0x24]), compressed]);
  const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
  let n = BigInt(`0x${bytes.toString('hex')}`);
  let out = '';
  while (n > 0n) {
    out = alphabet[Number(n % 58n)] + out;
    n /= 58n;
  }
  return { privateKey, did: `did:key:z${out}` };
};

/// A new P-256 key, as a did:key: what a steward holds as a recovery key.
export const didKey = () => p256().did;

/// `account`'s session at their own PDS, signed in with their password.
const pdsSession = async (account) => {
  const res = await fetch(`${world.pds}/xrpc/com.atproto.server.createSession`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ identifier: account.handle, password: account.password })
  });
  if (!res.ok) throw new Error(`createSession: ${await res.text()}`);
  return (await res.json()).accessJwt;
};

/// Makes `account`'s inbox at their PDS as their app does: a memberspace
/// that keeps anyone's writes and names no members, so any group may
/// write an invite there and only they read it. Its URI.
export const makeInbox = async (account) => {
  const res = await fetch(`${world.pds}/xrpc/com.atproto.space.createSpace`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      authorization: `Bearer ${await pdsSession(account)}`
    },
    body: JSON.stringify({
      spaceType: 'org.atgrp.invites',
      skey: 'self',
      policy: { $type: 'com.atproto.memberspace.policy', publicWrite: true }
    })
  });
  if (!res.ok) throw new Error(`createSpace: ${await res.text()}`);
  return (await res.json()).space;
};

/// The order of P-256, whose upper half a signature's `s` is kept out of.
const ORDER = 0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551n;

/// The repos in the writer set of `account`'s `inbox`, as their PDS keeps
/// it, read as their app syncs it: a credential for a key of its own,
/// from their delegation token, and the request signed with that key.
export const inboxWriters = async (account, inbox) => {
  const space = encodeURIComponent(inbox);
  const delegated = await fetch(
    `${world.pds}/xrpc/com.atproto.space.getDelegationToken?space=${space}`,
    { headers: { authorization: `Bearer ${await pdsSession(account)}` } }
  );
  if (!delegated.ok) throw new Error(`getDelegationToken: ${await delegated.text()}`);
  const key = p256();
  const issued = await fetch(`${world.pds}/xrpc/com.atproto.space.authorizeSpaceSync`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      space: inbox,
      credentialKey: key.did,
      delegationToken: (await delegated.json()).token
    })
  });
  if (!issued.ok) throw new Error(`authorizeSpaceSync: ${await issued.text()}`);
  const authorization = `Atproto-Space-Credential ${(await issued.json()).credential}`;
  const input = '("authorization" "atproto-space-audience");tag="atproto-space-credential"';
  const base = `"authorization": ${authorization}\n"atproto-space-audience": ${account.did}\n"@signature-params": ${input}`;
  const signature = sign('sha256', Buffer.from(base), {
    key: key.privateKey,
    dsaEncoding: 'ieee-p1363'
  });
  const s = BigInt(`0x${signature.subarray(32).toString('hex')}`);
  if (s > ORDER / 2n)
    signature.set(Buffer.from((ORDER - s).toString(16).padStart(64, '0'), 'hex'), 32);
  const res = await fetch(`${world.pds}/xrpc/com.atproto.space.listWriters?space=${space}`, {
    headers: {
      authorization,
      'atproto-space-audience': account.did,
      'signature-input': `sig=${input}`,
      signature: `sig=:${signature.toString('base64')}:`
    }
  });
  if (!res.ok) throw new Error(`listWriters: ${await res.text()}`);
  return (await res.json()).repos.map((r) => r.repo);
};

/// An MCP client on this machine, as an AI app signs in: it finds the
/// host's authorization server from the MCP server's metadata, registers
/// itself, and sends the browser to say yes, with PKCE; then calls tools
/// with the Bearer token it is given.
export class McpClient {
  async listen() {
    this.back = new Promise((done) => {
      this.server = createServer((req, res) => {
        res.end('back at the app');
        done(`http://127.0.0.1:${this.server.address().port}${req.url}`);
      });
    });
    await new Promise((r) => this.server.listen(0, '127.0.0.1', r));
    this.redirect = `http://127.0.0.1:${this.server.address().port}/callback`;
    this.verifier = b64(randomBytes(32));
    return this;
  }

  close() {
    this.server?.close();
  }

  /// Registers, and answers where to send the browser.
  async authorize(name) {
    const resource = await (
      await fetch(`${world.host}/.well-known/oauth-protected-resource/mcp`)
    ).json();
    this.mcp = resource.resource;
    const issuer = new URL(resource.authorization_servers[0]);
    const meta = await (
      await fetch(`${issuer.origin}/.well-known/oauth-authorization-server${issuer.pathname}`)
    ).json();
    this.token = meta.token_endpoint;
    const registered = await fetch(meta.registration_endpoint, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ client_name: name, redirect_uris: [this.redirect] })
    });
    expect(registered.status).toBe(201);
    this.id = (await registered.json()).client_id;
    const to = new URL(meta.authorization_endpoint);
    for (const [k, v] of Object.entries({
      response_type: 'code',
      client_id: this.id,
      redirect_uri: this.redirect,
      code_challenge: sha256(this.verifier),
      code_challenge_method: 'S256',
      state: 'st4te',
      resource: this.mcp
    }))
      to.searchParams.set(k, v);
    return to.href;
  }

  /// The tokens the code the browser came back with is worth.
  async exchange(back) {
    const body = new URLSearchParams({
      grant_type: 'authorization_code',
      code: new URL(back).searchParams.get('code'),
      redirect_uri: this.redirect,
      code_verifier: this.verifier,
      client_id: this.id
    });
    const res = await fetch(this.token, { method: 'POST', body });
    expect(res.status).toBe(200);
    return res.json();
  }

  /// Calls the tool `name` with `args`: the status, and its result.
  async tool(token, name, args) {
    const res = await fetch(this.mcp, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        accept: 'application/json, text/event-stream',
        authorization: `Bearer ${token}`
      },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: 1,
        method: 'tools/call',
        params: { name, arguments: args }
      })
    });
    return { status: res.status, result: (await res.json().catch(() => null))?.result };
  }
}
