// Records what the space alpha's @atproto/space makes of a space meta
// archive (exportSpaceMeta's, which an account's server serves as it
// moves), into space-meta.json beside this: two archives it writes, and
// archives it refuses, each with its refusal. The refused ones are its
// own tests' (space-meta-archive.test.ts, star.test.ts), hand-built as
// they build them.
//
//   node space-meta.mjs <a directory with @atproto/space installed> > space-meta.json
import { pathToFileURL } from 'node:url'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const dir = process.argv[2]
const load = (pkg) => import(pathToFileURL(join(dir, 'node_modules', pkg, 'dist/index.js')).href)
const space = await load('@atproto/space')
const { encode } = await load('@atproto/lex-cbor')
const version = JSON.parse(readFileSync(join(dir, 'node_modules/@atproto/space/package.json'))).version

const collect = async (stream) => {
  const chunks = []
  for await (const chunk of stream) chunks.push(chunk)
  return Buffer.concat(chunks)
}
const hex = (b) => Buffer.from(b).toString('hex')
const utf8 = (s) => new TextEncoder().encode(s)
const varint = (n) => {
  const out = []
  while (n >= 0x80) {
    out.push((n & 0x7f) | 0x80)
    n = Math.floor(n / 128)
  }
  out.push(n)
  return Buffer.from(out)
}

const SPACE = 'at://did:plc:harborcycling/space/com.example.forum/general'
const ALICE = 'did:plc:alice'
const LOCAL = 'did:web:localhost%3A2583'
const POLICY = { $type: 'com.atproto.memberspace.policy', publicRead: true, publicWrite: false }
const member = (rkey, read, write) => ({
  key: `com.atproto.memberspace.member/${rkey}`,
  value: { $type: 'com.atproto.memberspace.member', read, write },
})
const WRITER = { rev: '3kbcq3p7ad42b', data: new Uint8Array(32).fill(1), spaceRev: '3kbcq3p7ad42c' }

// What readSpaceMetaStar reads back, as JSON: bytes as hex.
const plain = (archive) => ({
  space: archive.space,
  policy: archive.policy,
  policyState: archive.policyState,
  writers: archive.writers.map((w) => ({ ...w, data: hex(w.data) })),
})

async function written(archive) {
  const bytes = await collect(space.serializeSpaceMeta(archive))
  return { hex: hex(bytes), read: plain(await space.readSpaceMetaStar([bytes])) }
}

// A member's key holds its DID as a record key: a localhost did:web's
// port unencoded (the PDS's didToRecordKey).
const full = await written({
  space: SPACE,
  policy: POLICY,
  policyState: [member('did:web:localhost:2583', true, false), member(ALICE, true, true)],
  writers: [{ did: LOCAL, ...WRITER }, { did: ALICE, ...WRITER }],
})
const empty = await written({
  space: SPACE,
  policy: { $type: 'com.atproto.personalspace.policy' },
  policyState: [],
  writers: [],
})

const META = { $type: 'atproto-space-meta', ver: 1, space: SPACE }
const header = (meta) => Buffer.concat([space.STAR_MAGIC, varint(meta.length), meta])
const pair = (key, value) => Buffer.concat([varint(key.length), key, varint(value.length), value])
// An archive of `pairs`, each key's value encoded, under `meta`.
const raw = (pairs, meta = {}) =>
  Buffer.concat([
    header(encode({ ...META, ...meta })),
    ...pairs.map(([k, v]) => pair(utf8(k), encode(v))),
  ])
const { spaceRev: _, ...noSpaceRev } = WRITER
const memberValue = member(ALICE, true, true).value

const refusals = {
  'bad magic': Buffer.concat([Buffer.from([0x3a, 0x6c, 0x01, 0x00]), raw([['policy', POLICY]]).subarray(4)]),
  'a non-canonical varint': (() => {
    const meta = encode(META)
    return Buffer.concat([space.STAR_MAGIC, Buffer.from([meta.length | 0x80, 0x00]), meta])
  })(),
  'metadata out of canonical order': header(
    Buffer.from([0xa2, 0x65, ...utf8('$type'), 0x61, 0x74, 0x61, 0x61, 0x01]),
  ),
  'metadata without a $type': header(encode({ a: 1 })),
  'bytes after the metadata': header(Buffer.concat([encode(META), Buffer.from([0])])),
  'a pair cut short': Buffer.concat([header(encode(META)), varint(1), Buffer.from([1])]),
  'a missing policy': raw([[`com.atproto.memberspace.member/${ALICE}`, memberValue]]),
  'state before the policy': raw([
    [`com.atproto.memberspace.member/${ALICE}`, memberValue],
    ['policy', POLICY],
  ]),
  'state after the writers': raw([
    ['policy', POLICY],
    [`writer/${ALICE}`, WRITER],
    [`com.atproto.memberspace.member/${ALICE}`, memberValue],
  ]),
  'a key twice': raw([
    ['policy', POLICY],
    [`com.atproto.memberspace.member/${ALICE}`, memberValue],
    [`com.atproto.memberspace.member/${ALICE}`, memberValue],
  ]),
  'the policy twice': raw([
    ['policy', POLICY],
    ['policy', POLICY],
  ]),
  'keys out of order': raw([
    ['policy', POLICY],
    ['writer/did:plc:carol', WRITER],
    ['writer/did:plc:bob', WRITER],
  ]),
  'a writer key without a DID': raw([
    ['policy', POLICY],
    ['writer/bob', WRITER],
  ]),
  'a writer without its space revision': raw([
    ['policy', POLICY],
    [`writer/${ALICE}`, noSpaceRev],
  ]),
  'a writer whose revision is no TID': raw([
    ['policy', POLICY],
    [`writer/${ALICE}`, { ...WRITER, rev: 'yesterday' }],
  ]),
  "a repo's archive": raw([['policy', POLICY]], { $type: 'atproto-space-repo' }),
  'a version to come': raw([['policy', POLICY]], { ver: 2 }),
  'no space': raw([['policy', POLICY]], { space: 'general' }),
  'a value that is no map': Buffer.concat([header(encode(META)), pair(utf8('policy'), encode('memberspace'))]),
}

const refused = []
for (const [why, bytes] of Object.entries(refusals)) {
  const error = await space.readSpaceMetaStar([bytes]).then(
    () => null,
    (e) => e.message,
  )
  if (!error) throw new Error(`the alpha reads ${why}`)
  refused.push({ why, hex: hex(bytes), error })
}

console.log(
  JSON.stringify(
    { generatedFrom: { repo: 'bluesky-social/atproto', package: `@atproto/space@${version}` }, full, empty, refused },
    null,
    2,
  ),
)
