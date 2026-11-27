// A world to use the console in: the reference PDS and PLC
// (harness/pds.mjs), the host over them, the console's build in front of
// the host, and a group of alice's, made with service auth from her PDS, as
// a member's own client would. The e2e flows run over one
// (e2e/setup.mjs), and `just look` over another (look/look.mjs), each
// behind the https front here (`tlsFront`); `just dev` runs one for a
// person to look at (dev.mjs), behind portless.

import { serveApps } from './oauth-app.mjs';
import { spawn } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

export const root = resolve(fileURLToPath(new URL('.', import.meta.url)), '..');

/// The first line `child` prints that `wanted` takes.
export const line = async (child, wanted) => {
  for await (const l of createInterface({ input: child.stdout })) if (wanted(l)) return l;
  throw new Error(`${child.spawnfile} stopped before it was ready`);
};

/// A port nothing listens on now.
export const free = () =>
  new Promise((done) => {
    const server = createServer().listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => done(port));
    });
  });

/// Until `url` answers.
const up = async (url) => {
  for (let i = 0; i < 100; i++) {
    if (
      await fetch(url).then(
        () => true,
        () => false
      )
    )
      return;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`${url} never answered`);
};

/// The console's front for a browser the harness drives, harness/tls-proxy.mjs
/// with the certificate the PDS trusts, for the name only it resolves
/// (harness/pds.mjs), which the browser is told (`resolving`).
export const tlsFront = async (consolePort, net, children) => {
  const args = [join(root, 'harness/tls-proxy.mjs'), net.console.cert, net.console.key];
  const proxy = spawn('node', [...args, String(consolePort)], {
    stdio: ['pipe', 'pipe', 'inherit']
  });
  children.push(proxy);
  const port = (await line(proxy, (l) => /^\d+$/.test(l.trim()))).trim();
  return `https://${net.console.host}:${port}`;
};

/// Chromium's argument that sends the console's name, and every name
/// beside it, to this machine.
export const resolving = '--host-resolver-rules=MAP *.group-host.internal 127.0.0.1';

/// `account`'s service auth for `aud` and `lxm`, from their own PDS.
export const serviceAuth = async (pds, account, aud, lxm) => {
  const session = await fetch(`${pds}/xrpc/com.atproto.server.createSession`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ identifier: account.handle, password: account.password })
  }).then((r) => r.json());
  const asked = `${pds}/xrpc/com.atproto.server.getServiceAuth?aud=${encodeURIComponent(aud)}&lxm=${lxm}`;
  const { token } = await fetch(asked, {
    headers: { authorization: `Bearer ${session.accessJwt}` }
  }).then((r) => r.json());
  return token;
};

/// Starts a world with `accounts` on the reference PDS, its console
/// served at the port `front` is given and reached at the https address
/// `front` answers with (`front(consolePort, net, children)`): built, or
/// with `live`, by the dev server, redrawn as its source changes. `pdsEnv`
/// is the PDS's environment beyond this one's; `handleDomain`, the domain
/// its groups' handles and addresses are under. Its parts, and `stop`.
export async function start({
  accounts,
  front,
  pdsEnv = {},
  live = false,
  handleDomain = '.groups.test'
}) {
  const children = [];
  const stop = () => children.reverse().forEach((c) => c.kill('SIGTERM'));
  try {
    const pds = spawn('node', [join(root, 'harness/pds.mjs'), '--accounts', accounts.join(',')], {
      stdio: ['pipe', 'pipe', 'inherit'],
      env: { ...process.env, ...pdsEnv }
    });
    children.push(pds);
    const net = JSON.parse(await line(pds, (l) => l.startsWith('{"pds"')));

    const [hostPort, consolePort] = [await free(), await free()];
    const base = await front(consolePort, net, children);
    const host = `http://localhost:${hostPort}`;
    const did = `did:web:localhost%3A${hostPort}`;
    // Apps on the web to sign in as a group (oauth-app.mjs), and the
    // host's DNS-over-HTTPS, which resolves no other app's lexicon here.
    const apps = await serveApps(host);
    children.push({ kill: apps.close });

    const server = spawn(join(root, 'target/debug/group-host'), [], {
      stdio: ['ignore', 'pipe', 'inherit'],
      env: {
        ...process.env,
        PORT: String(hostPort),
        PUBLIC_URL: host,
        CONSOLE_URL: base,
        DATA_DIR: mkdtempSync(join(tmpdir(), 'group-host-world-')),
        PLC_URL: net.plc,
        DID_WEB_SCHEME: 'http',
        HANDLE_DOMAIN: handleDomain,
        HANDLE_RESOLVERS: net.pds,
        DNS_OVER_HTTPS: `${apps.origin}/dns-query`,
        // Every browser, page and command here is one address, asking far
        // more than any one person would.
        RATE_LIMITS: 'off'
      }
    });
    children.push(server);
    await line(server, (l) => l.startsWith('listening on'));

    const serve = live
      ? [
          'npx',
          ['vite', 'dev', '--port', String(consolePort), '--strictPort', '--host', '127.0.0.1']
        ]
      : ['node', ['build']];
    const web = spawn(serve[0], serve[1], {
      cwd: join(root, 'web'),
      stdio: ['ignore', 'inherit', 'inherit'],
      // A body as large as the host takes, past the Node adapter's 512K.
      env: {
        ...process.env,
        PORT: String(consolePort),
        HOST: '127.0.0.1',
        GROUP_HOST: `http://127.0.0.1:${hostPort}`,
        ORIGIN: base,
        BODY_SIZE_LIMIT: '64M'
      }
    });
    children.push(web);
    await up(`http://127.0.0.1:${consolePort}/`);

    // A group of alice's, as her own client would make one.
    const alice = net.accounts.find((a) => a.handle === 'alice.test');
    const lxm = 'org.atgrp.internal.createGroup';
    const made = await fetch(`${host}/xrpc/${lxm}`, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        authorization: `Bearer ${await serviceAuth(net.pds, alice, did, lxm)}`
      },
      body: JSON.stringify({ handle: `riders${handleDomain}`, displayName: 'Riders' })
    });
    if (!made.ok) throw new Error(`createGroup: ${await made.text()}`);
    const riders = await made.json();

    return { base, host, did, net, riders: riders.did, apps: apps.origin, stop };
  } catch (e) {
    stop();
    throw e;
  }
}
