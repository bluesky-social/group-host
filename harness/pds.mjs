// Runs the reference PDS (@atproto/pds, through @atproto/dev-env) and a PLC
// directory on free ports, makes test accounts, prints one JSON line naming
// them, and stops when stdin closes. As knotless's harness/pds.mjs, less what
// only knotless needs.
//
//   node harness/pds.mjs [--accounts alice,bob]
//
// GROUP_HOST_DEV_ENV names a built dev-env package to run instead of the npm
// one: an atproto checkout's packages/dev-env on the spaces alpha, as CI
// builds it (.knotless/workflows/test.yml). The alpha's PDS migrates an
// account's database each time it opens it; here only the first time.

import { fileURLToPath, pathToFileURL } from 'node:url'
import { dirname, join, resolve } from 'node:path'
import { readFileSync, realpathSync } from 'node:fs'
import { createRequire } from 'node:module'
import { AsyncLocalStorage } from 'node:async_hooks'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
// The PDS's better-sqlite3 is built for the Node .node-version names.
const pinned = readFileSync(join(root, '.node-version'), 'utf8').trim()
if (process.versions.node.split('.')[0] !== pinned.split('.')[0]) {
  console.error(`harness/pds.mjs runs on Node ${pinned}, as .node-version says, not ${process.version}`)
  process.exit(1)
}
const devEnv = process.env.GROUP_HOST_DEV_ENV
const { TestNetworkNoAppView } = await import(
  devEnv ? pathToFileURL(resolve(root, devEnv, 'dist/index.js')).href : '@atproto/dev-env'
)
if (devEnv) await migrateOnce(resolve(root, devEnv))

/// Has the dev-env's PDS migrate each account's database on its first
/// opening alone, rather than on every one.
async function migrateOnce(dir) {
  const index = createRequire(join(dir, 'package.json')).resolve('@atproto/pds')
  const dist = dirname(realpathSync(index))
  const load = (file) => import(pathToFileURL(join(dist, file)).href)
  const { ActorStore } = await load('actor-store/actor-store.js')
  const { Migrator } = await load('db/migrator.js')
  const opened = new Set()
  const again = new AsyncLocalStorage()
  const open = ActorStore.prototype.openDb
  ActorStore.prototype.openDb = async function (did) {
    const { dbLocation } = await this.getLocation(did)
    const db = await again.run(opened.has(dbLocation), () => open.call(this, did))
    opened.add(dbLocation)
    return db
  }
  const migrate = Migrator.prototype.migrateToLatestOrThrow
  Migrator.prototype.migrateToLatestOrThrow = function () {
    return again.getStore() ? Promise.resolve([]) : migrate.call(this)
  }
}

const at = process.argv.indexOf('--accounts')
const names = (at === -1 ? 'alice,bob' : process.argv[at + 1]).split(',')

const net = await TestNetworkNoAppView.create({ pds: { hostname: 'localhost' } })

const accounts = []
for (const name of names) {
  const password = `${name}-password`
  const res = await fetch(`${net.pds.url}/xrpc/com.atproto.server.createAccount`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ email: `${name}@test.com`, handle: `${name}.test`, password }),
  })
  if (!res.ok) throw new Error(`createAccount ${name}: ${await res.text()}`)
  const { did, handle } = await res.json()
  accounts.push({ handle, did, password })
}

const described = await fetch(`${net.pds.url}/xrpc/com.atproto.server.describeServer`).then((r) => r.json())
console.log(JSON.stringify({ pds: net.pds.url, plc: net.plc.url, spaces: described.spacesAlpha === true, accounts }))

const shutdown = async () => {
  await net.close()
  process.exit(0)
}
process.stdin.on('end', shutdown)
process.stdin.on('close', shutdown)
process.stdin.resume()
process.on('SIGTERM', shutdown)
process.on('SIGINT', shutdown)
