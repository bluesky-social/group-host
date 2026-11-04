// What the flows share: the world they run over, signing in as a person
// does it, and an app signing in as a group, as an atproto client does.

import { expect } from '@playwright/test'
import { createHash, generateKeyPairSync, randomBytes, sign } from 'node:crypto'
import { createServer } from 'node:http'

export const world = JSON.parse(process.env.E2E_WORLD)
export const who = Object.fromEntries(world.accounts.map((a) => [a.handle.split('.')[0], a]))
export const url = (path) => world.base + path

/// Signs in as `name` on the PDS's own page `page` has come to, and
/// approves what was asked, as knotless's flows do.
export const approve = async (page, name) => {
  await page.waitForURL((u) => u.origin === new URL(world.pds).origin)
  await page.getByRole('textbox', { name: 'Password' }).fill(who[name].password)
  await page.getByRole('button', { name: 'Sign in' }).click()
  await page.getByRole('button', { name: /^(accept|authorize|allow)$/i }).click()
}

/// Signs in to the console as `name` from the header's dialog: their
/// handle, then their PDS's page, which sends the browser back.
export const signIn = async (page, name) => {
  const back = page.url()
  await page.getByRole('banner').getByRole('button', { name: 'Sign in' }).click()
  const dialog = page.getByRole('dialog', { name: 'Sign in with your Atmosphere account' })
  await dialog.getByRole('textbox', { name: 'Your handle' }).fill(who[name].handle)
  await dialog.getByRole('button', { name: 'Continue' }).click()
  await approve(page, name)
  await page.waitForURL(back)
}

/// Who the header says is signed in: nothing for no one.
export const signedInAs = (page) =>
  page
    .locator('.sign-out .mono')
    .textContent({ timeout: 1000 })
    .then((t) => t.trim(), () => '')

const b64 = (bytes) => Buffer.from(bytes).toString('base64url')
const sha256 = (text) => b64(createHash('sha256').update(text).digest())

/// An app on this machine, signing in as a group: the loopback client
/// asking for `scope`, with a DPoP key, PKCE, and the nonce the host last
/// gave, as atproto's OAuth clients do. It listens for the browser coming
/// back on a port of its own (`listen`), as a native app does: the
/// loopback client registers its redirect on any port.
export class App {
  static REDIRECT = 'http://127.0.0.1/callback'

  /// Listens for the browser coming back; `back` is where it came to.
  async listen() {
    this.back = new Promise((done) => {
      this.server = createServer((req, res) => {
        res.end('back at the app')
        done(`http://127.0.0.1:${this.server.address().port}${req.url}`)
      })
    })
    await new Promise((r) => this.server.listen(0, '127.0.0.1', r))
    this.redirect = `http://127.0.0.1:${this.server.address().port}/callback`
    return this
  }

  close() {
    this.server?.close()
  }

  constructor(scope) {
    const id = new URL('http://localhost')
    id.searchParams.set('redirect_uri', App.REDIRECT)
    id.searchParams.set('scope', scope)
    this.id = id.href
    this.keys = generateKeyPairSync('ec', { namedCurve: 'P-256' })
    this.verifier = b64(randomBytes(32))
    this.nonce = undefined
  }

  /// A DPoP proof of `method` to `url`, for `token` if it carries one.
  proof(method, url, token) {
    const jwk = this.keys.publicKey.export({ format: 'jwk' })
    const header = { typ: 'dpop+jwt', alg: 'ES256', jwk }
    const at = new URL(url)
    const claims = { jti: b64(randomBytes(12)), htm: method, htu: `${at.origin}${at.pathname}`, iat: Math.floor(Date.now() / 1000) }
    if (this.nonce) claims.nonce = this.nonce
    if (token) claims.ath = sha256(token)
    const body = `${b64(JSON.stringify(header))}.${b64(JSON.stringify(claims))}`
    const sig = sign('sha256', Buffer.from(body), { key: this.keys.privateKey, dsaEncoding: 'ieee-p1363' })
    return `${body}.${b64(sig)}`
  }

  /// `init` sent to `url` with a fresh proof, once more with the nonce the
  /// host asks for: its status and JSON.
  async send(url, init, token) {
    for (let i = 0; i < 2; i++) {
      const headers = { ...init.headers, dpop: this.proof(init.method ?? 'GET', url, token) }
      if (token) headers.authorization = `DPoP ${token}`
      const res = await fetch(url, { ...init, headers })
      this.nonce = res.headers.get('dpop-nonce') ?? this.nonce
      const body = await res.json().catch(() => null)
      const stale = body?.error === 'use_dpop_nonce' || res.headers.get('www-authenticate')?.includes('use_dpop_nonce')
      if (!stale) return { status: res.status, body }
    }
    throw new Error(`${url} asked for a nonce twice`)
  }

  form(path, fields) {
    const body = new URLSearchParams({ client_id: this.id, ...fields })
    const headers = { 'content-type': 'application/x-www-form-urlencoded' }
    return this.send(`${world.host}${path}`, { method: 'POST', headers, body })
  }

  /// Pushes a request to sign in: where to send the browser.
  async push(hint) {
    const pushed = await this.form('/oauth/par', {
      response_type: 'code',
      code_challenge: sha256(this.verifier),
      code_challenge_method: 'S256',
      state: 'st4te',
      redirect_uri: this.redirect,
      ...(hint ? { login_hint: hint } : {}),
    })
    expect(pushed.status, JSON.stringify(pushed.body)).toBe(201)
    const to = new URL(`${world.host}/oauth/authorize`)
    to.searchParams.set('client_id', this.id)
    to.searchParams.set('request_uri', pushed.body.request_uri)
    return to.href
  }

  /// The tokens the code the browser came back with is worth.
  async exchange(back) {
    const code = new URL(back).searchParams.get('code')
    const said = await this.form('/oauth/token', {
      grant_type: 'authorization_code',
      code,
      redirect_uri: this.redirect,
      code_verifier: this.verifier,
    })
    expect(said.status, JSON.stringify(said.body)).toBe(200)
    return said.body
  }

  /// Calls the procedure `nsid` on the host with `token`.
  call(nsid, token, input) {
    const init = { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(input) }
    return this.send(`${world.host}/xrpc/${nsid}`, init, token)
  }
}

/// A new P-256 key, as a did:key: what a steward holds as a recovery key.
export const didKey = () => {
  const { publicKey } = generateKeyPairSync('ec', { namedCurve: 'P-256' })
  const point = publicKey.export({ format: 'der', type: 'spki' }).subarray(-65)
  const compressed = Buffer.concat([Buffer.from([point[64] & 1 ? 3 : 2]), point.subarray(1, 33)])
  const bytes = Buffer.concat([Buffer.from([0x80, 0x24]), compressed])
  const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
  let n = BigInt(`0x${bytes.toString('hex')}`)
  let out = ''
  while (n > 0n) {
    out = alphabet[Number(n % 58n)] + out
    n /= 58n
  }
  return `did:key:z${out}`
}

/// An assistant on this machine, as an MCP client signs in: it finds the
/// host's authorization server from the MCP server's metadata, registers
/// itself, and sends the browser to say yes, with PKCE; then calls tools
/// with the Bearer token it is given.
export class Assistant {
  async listen() {
    this.back = new Promise((done) => {
      this.server = createServer((req, res) => {
        res.end('back at the assistant')
        done(`http://127.0.0.1:${this.server.address().port}${req.url}`)
      })
    })
    await new Promise((r) => this.server.listen(0, '127.0.0.1', r))
    this.redirect = `http://127.0.0.1:${this.server.address().port}/callback`
    this.verifier = b64(randomBytes(32))
    return this
  }

  close() {
    this.server?.close()
  }

  /// Registers, and answers where to send the browser.
  async authorize(name) {
    const resource = await (await fetch(`${world.host}/.well-known/oauth-protected-resource/mcp`)).json()
    this.mcp = resource.resource
    const issuer = new URL(resource.authorization_servers[0])
    const meta = await (await fetch(`${issuer.origin}/.well-known/oauth-authorization-server${issuer.pathname}`)).json()
    this.token = meta.token_endpoint
    const registered = await fetch(meta.registration_endpoint, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ client_name: name, redirect_uris: [this.redirect] }),
    })
    expect(registered.status).toBe(201)
    this.id = (await registered.json()).client_id
    const to = new URL(meta.authorization_endpoint)
    for (const [k, v] of Object.entries({
      response_type: 'code',
      client_id: this.id,
      redirect_uri: this.redirect,
      code_challenge: sha256(this.verifier),
      code_challenge_method: 'S256',
      state: 'st4te',
      resource: this.mcp,
    }))
      to.searchParams.set(k, v)
    return to.href
  }

  /// The tokens the code the browser came back with is worth.
  async exchange(back) {
    const body = new URLSearchParams({
      grant_type: 'authorization_code',
      code: new URL(back).searchParams.get('code'),
      redirect_uri: this.redirect,
      code_verifier: this.verifier,
      client_id: this.id,
    })
    const res = await fetch(this.token, { method: 'POST', body })
    expect(res.status).toBe(200)
    return res.json()
  }

  /// Calls the tool `name` with `args`: the status, and its result.
  async tool(token, name, args) {
    const res = await fetch(this.mcp, {
      method: 'POST',
      headers: { 'content-type': 'application/json', accept: 'application/json, text/event-stream', authorization: `Bearer ${token}` },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name, arguments: args } }),
    })
    return { status: res.status, result: (await res.json().catch(() => null))?.result }
  }
}
