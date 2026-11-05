// Before the flows: a world (../world.mjs) whose console is behind the
// https front the PDS sends sign-ins back to (harness/tls-proxy.mjs), at
// the name harness/pds.mjs resolves for itself. Where everything is goes
// to the flows in E2E_WORLD. Returns the teardown.

import { spawn } from 'node:child_process'
import { join } from 'node:path'
import { line, root, start } from '../world.mjs'

/// The console's front: harness/tls-proxy.mjs, with the certificate the
/// PDS trusts, for the name only it resolves.
const front = async (consolePort, net, children) => {
  const proxy = spawn('node', [join(root, 'harness/tls-proxy.mjs'), net.console.cert, net.console.key, String(consolePort)], {
    stdio: ['pipe', 'pipe', 'inherit'],
  })
  children.push(proxy)
  return `https://${net.console.host}:${(await line(proxy, (l) => /^\d+$/.test(l.trim()))).trim()}`
}

export default async function setup() {
  // harbor is an account of its own, which alice moves here to become a group;
  // carol, dan, erin and frank fill a group past a page of its members.
  const w = await start({ accounts: ['alice', 'bob', 'harbor', 'carol', 'dan', 'erin', 'frank'], front })
  const { net } = w
  process.env.E2E_WORLD = JSON.stringify({ base: w.base, host: w.host, did: w.did, pds: net.pds, mail: net.mail, accounts: net.accounts, riders: w.riders })
  return w.stop
}
