// Records what jose (panva's, which @atproto/oauth-provider checks DPoP
// proofs and client assertions with) makes of fixed keys, into jose.json
// beside this: each key's public JWK and RFC 7638 thumbprint, and a JWT it
// signs. ECDSA signatures are random, so the JWTs differ each run.
//
//   node jose.mjs <jose's directory, 5.x for ES256K> > jose.json
import { createECDH, createPrivateKey } from 'node:crypto'
const jose = await import(process.argv[2] + '/dist/node/esm/index.js')

const keys = {
  'P-256': ['ES256', '1111111111111111111111111111111111111111111111111111111111111111'],
  secp256k1: ['ES256K', 'a1b2c3d4e5f60718293a4b5c6d7e8f90112233445566778899aabbccddeeff01'],
}

// A private key from its scalar: ECDH finds its public point.
function privateKey(crv, hex) {
  const ecdh = createECDH(crv === 'P-256' ? 'prime256v1' : 'secp256k1')
  ecdh.setPrivateKey(Buffer.from(hex, 'hex'))
  const point = ecdh.getPublicKey()
  const b64 = (b) => Buffer.from(b).toString('base64url')
  const jwk = { kty: 'EC', crv, d: b64(Buffer.from(hex, 'hex')), x: b64(point.subarray(1, 33)), y: b64(point.subarray(33)) }
  return createPrivateKey({ key: jwk, format: 'jwk' })
}

const out = { generatedFrom: `jose ${process.argv[2].match(/jose@([^/]+)/)?.[1]}`, keys: [] }
for (const [crv, [alg, hex]] of Object.entries(keys)) {
  const key = privateKey(crv, hex)
  const { d: _, ...jwk } = key.export({ format: 'jwk' })
  const jwt = await new jose.SignJWT({ htm: 'POST', htu: 'https://host.test/oauth/token', jti: 'one' })
    .setProtectedHeader({ alg, typ: 'dpop+jwt', jwk })
    .setIssuedAt(1793736300)
    .sign(key)
  out.keys.push({ crv, alg, secret: hex, jwk, thumbprint: await jose.calculateJwkThumbprint(jwk, 'sha256'), jwt })
}
// RFC 9449's own example key (§ 6.1), with the thumbprint it gives.
const rfc = { kty: 'EC', x: 'l8tFrhx-34tV3hRICRDY9zCkDlpBhF42UQUfWVAWBFs', y: '9VE4jf_Ok_o64zbTTlcuNJajHmt6v9TDVrU0CdvGRDA', crv: 'P-256' }
out.rfc9449 = { jwk: rfc, thumbprint: await jose.calculateJwkThumbprint(rfc, 'sha256') }
console.log(JSON.stringify(out, null, 2))
