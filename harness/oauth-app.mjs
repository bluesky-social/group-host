// An app signing in as a group, as atproto's OAuth clients do: a DPoP
// key, PKCE, and the nonce the host last gave. The e2e flows sign the
// loopback client in with it (e2e/pages.mjs); `serveApps` is a world's
// apps on the web (../world.mjs), each with its client metadata, as an
// app such as Grain publishes its own.

import { createHash, generateKeyPairSync, randomBytes, sign } from 'node:crypto';
import { createServer } from 'node:http';
import { crc32, deflateSync } from 'node:zlib';

const b64 = (bytes) => Buffer.from(bytes).toString('base64url');
const sha256 = (text) => b64(createHash('sha256').update(text).digest());

/// A client `id` of the host at `host`, the browser sent back to `redirect`.
export class OAuthClient {
  constructor(host, id, redirect) {
    this.host = host;
    this.id = id;
    this.redirect = redirect;
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
    return this.send(`${this.host}${path}`, { method: 'POST', headers, body });
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
    if (pushed.status !== 201) throw new Error(`par: ${JSON.stringify(pushed.body)}`);
    const to = new URL(`${this.host}/oauth/authorize`);
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
    if (said.status !== 200) throw new Error(`token: ${JSON.stringify(said.body)}`);
    return said.body;
  }

  /// Calls the procedure `nsid` on the host with `token`.
  call(nsid, token, input) {
    const init = {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(input)
    };
    return this.send(`${this.host}/xrpc/${nsid}`, init, token);
  }
}

/// A square PNG of `size`: a disc of `fg` on `bg`, as an app's icon.
function icon(size, bg, fg) {
  const rows = Buffer.alloc((size * 3 + 1) * size);
  const r = size * 0.32;
  for (let y = 0; y < size; y++) {
    const at = y * (size * 3 + 1);
    for (let x = 0; x < size; x++) {
      const inside = Math.hypot(x + 0.5 - size / 2, y + 0.5 - size / 2) < r;
      for (let c = 0; c < 3; c++) rows[at + 1 + x * 3 + c] = (inside ? fg : bg)[c];
    }
  }
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type), data]);
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body));
    return Buffer.concat([len, body, crc]);
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header[8] = 8;
  header[9] = 2;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(rows)),
    chunk('IEND', Buffer.alloc(0))
  ]);
}

/// The apps a world has, by the path their metadata is under. Darkroom is
/// two clients, as Grain is: one that signs in as the group and, signed
/// in, writes its profile for the group; and one, with no icon, that only
/// reads the group's spaces, as an admin connects it.
export const APPS = {
  darkroom: {
    name: 'Darkroom',
    logo: icon(96, [38, 36, 48], [232, 160, 72]),
    scope: 'atproto repo:app.bsky.feed.post repo:dev.darkroom.actor.profile',
    signedIn: (client, token, group) =>
      client.call('com.atproto.repo.putRecord', token, {
        repo: group,
        collection: 'dev.darkroom.actor.profile',
        rkey: 'self',
        record: { $type: 'dev.darkroom.actor.profile', createdAt: new Date().toISOString() }
      })
  },
  'darkroom-groups': {
    name: 'Darkroom',
    scope: [
      'atproto',
      'space:org.atgrp.meta?action=read',
      'space:org.atgrp.members?action=read',
      'rpc:org.atgrp.listJoinRequests?aud=*'
    ].join(' ')
  }
};

/// The world's apps, on the web at `localhost` as their origin: each
/// app's client metadata at `/<app>/client-metadata.json` and its icon at
/// `/<app>/logo.png`; `/<app>/sign-in?hint=<group>` sends the browser to
/// sign it in as the group, and `/<app>/callback` takes it back, the app
/// then acting as it. Their metadata names no `client_uri`, which the
/// console would show with the port the world was given. `/dns-query`
/// answers every name with nothing, as the host's DNS-over-HTTPS in a
/// world: no lexicon of another app's resolves, and nothing is asked of
/// the internet. Its origin, and `close`.
export async function serveApps(host) {
  const pending = new Map();
  let origin;
  const server = createServer(async (req, res) => {
    const at = new URL(req.url, origin);
    const send = (status, type, body) => {
      res.writeHead(status, { 'content-type': type });
      res.end(body);
    };
    if (at.pathname === '/dns-query')
      return send(200, 'application/dns-json', JSON.stringify({ Status: 3 }));
    const [, name, what] = at.pathname.split('/');
    const app = APPS[name];
    if (!app) return send(404, 'text/plain', 'no such app');
    const id = `${origin}/${name}/client-metadata.json`;
    const redirect = `${origin}/${name}/callback`;
    try {
      if (what === 'client-metadata.json') {
        const metadata = {
          client_id: id,
          client_name: app.name,
          ...(app.logo ? { logo_uri: `${origin}/${name}/logo.png` } : {}),
          redirect_uris: [redirect],
          scope: app.scope,
          grant_types: ['authorization_code', 'refresh_token'],
          response_types: ['code'],
          token_endpoint_auth_method: 'none',
          application_type: 'web',
          dpop_bound_access_tokens: true
        };
        return send(200, 'application/json', JSON.stringify(metadata));
      }
      if (what === 'logo.png' && app.logo) return send(200, 'image/png', app.logo);
      if (what === 'sign-in') {
        const client = new OAuthClient(host, id, redirect);
        const to = await client.push(at.searchParams.get('hint') ?? undefined);
        pending.set(name, client);
        res.writeHead(302, { location: to });
        return res.end();
      }
      if (what === 'callback') {
        const client = pending.get(name);
        pending.delete(name);
        if (!client || !at.searchParams.get('code'))
          return send(400, 'text/plain', 'not signed in');
        const tokens = await client.exchange(at.href);
        if (app.signedIn) {
          const done = await app.signedIn(client, tokens.access_token, tokens.sub);
          if (done.status !== 200) throw new Error(JSON.stringify(done.body));
        }
        return send(200, 'text/plain', `${app.name} is signed in.`);
      }
      return send(404, 'text/plain', 'not here');
    } catch (e) {
      return send(500, 'text/plain', String(e));
    }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  origin = `http://localhost:${server.address().port}`;
  return { origin, close: () => server.close() };
}
