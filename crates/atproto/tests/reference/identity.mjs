// Records what the reference implementations make of fixed keys, into
// identity.json beside this: PLC operations and DIDs (@did-plc/lib), DID
// documents, and a service-auth JWT (@atproto/xrpc-server). Its signatures
// are RFC 6979's, the same each run, but for the JWT's random jti.
//
//   node identity.mjs <a package depending on @did-plc/lib, @atproto/crypto,
//                      @atproto/common and @atproto/xrpc-server> > identity.json
import { createRequire } from 'node:module'
const r = createRequire(process.argv[2] + '/package.json')
const plc = r('@did-plc/lib')
const crypto = r('@atproto/crypto')
const { createServiceJwt } = r('@atproto/xrpc-server')

const hex = (h) => Uint8Array.from(Buffer.from(h, 'hex'))
const ROTATION = 'a1b2c3d4e5f60718293a4b5c6d7e8f90112233445566778899aabbccddeeff01'
const SIGNING = '0f1e2d3c4b5a69788796a5b4c3d2e1f00112233445566778899aabbccddeeff0'
const P256_ROTATION = '1111111111111111111111111111111111111111111111111111111111111111'

const rotation = await crypto.Secp256k1Keypair.import(hex(ROTATION))
const signing = await crypto.Secp256k1Keypair.import(hex(SIGNING))
const p256 = await crypto.P256Keypair.import(hex(P256_ROTATION))

const out = { keys: { rotation: ROTATION, signing: SIGNING, p256Rotation: P256_ROTATION }, plc: [] }
for (const [name, key, extra] of [
  ['k256 rotation', rotation, []],
  ['p256 rotation, a second key', p256, [rotation.did()]],
]) {
  const created = await plc.createOp({
    signingKey: signing.did(),
    handle: 'riders.groups.example.com',
    pds: 'https://groups.example.com',
    rotationKeys: [key.did(), ...extra],
    signer: key,
  })
  const handle = await plc.updateHandleOp(created.op, key, 'riders.example.org')
  const pds = await plc.updatePdsOp(handle, key, 'https://other.example.com')
  const { cidForCbor } = r('@atproto/common')
  const tomb = await plc.tombstoneOp(await cidForCbor(pds), key)
  const data = await plc.assureValidCreationOp(created.did, created.op)
  out.plc.push({
    name,
    did: created.did,
    genesis: created.op,
    genesisCid: (await cidForCbor(created.op)).toString(),
    updateHandle: handle,
    updatePds: pds,
    tombstone: tomb,
    document: plc.formatDidDoc({ did: created.did, ...data }),
  })
}

const jwt = await createServiceJwt({
  iss: out.plc[0].did,
  aud: 'did:web:groups.example.com',
  lxm: 'fyi.opensocial.getGroup',
  keypair: signing,
  exp: 4102444800, // 2100-01-01
})
out.serviceJwt = { token: jwt, iss: out.plc[0].did, aud: 'did:web:groups.example.com', lxm: 'fyi.opensocial.getGroup', signingDidKey: signing.did() }
console.log(JSON.stringify(out, null, 2))
