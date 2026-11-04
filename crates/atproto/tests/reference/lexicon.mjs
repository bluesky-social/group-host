// Records what @atproto/lexicon, atproto's reference, makes of values
// checked against lexicons, into lexicon.json beside this: each case's
// lexicons, the def it is checked against (a record's or an object's, or a
// method's parameters, input or message), the value, and the reference's
// verdict. The cases are lexicon.js's (github.com/bigmoves/lexicon.js,
// test/inputs/{data,format,e2e}), and a few of this crate's own below.
//
//   node lexicon.mjs <a lexicon.js checkout> <a directory with @atproto/lexicon installed> > lexicon.json
import { pathToFileURL } from 'node:url'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const [inputsDir, refDir] = process.argv.slice(2)
const pkg = join(refDir, 'node_modules/@atproto/lexicon')
const { Lexicons, jsonToLex } = await import(pathToFileURL(join(pkg, 'dist/index.js')).href)
const version = JSON.parse(readFileSync(join(pkg, 'package.json'))).version

const inputs = async (kind) => {
  const mod = await import(pathToFileURL(join(inputsDir, 'test/inputs', kind, 'index.js')).href)
  return Object.values(mod).filter(Array.isArray).flat()
}

// Bytes a case holds as a Uint8Array are written as JSON writes them.
const toJson = (v) =>
  JSON.parse(
    JSON.stringify(v, (_, x) =>
      x instanceof Uint8Array ? { $bytes: Buffer.from(x).toString('base64').replace(/=+$/, '') } : x,
    ),
  )

// What the def a case names is checked as: a record or an object, or a
// method's parameters, input or message.
const kindOf = (lexicons, ref) => {
  const [nsid, name = 'main'] = ref.split('#')
  const def = lexicons.find((l) => l.id === nsid)?.defs?.[name]
  switch (def?.type) {
    case 'query':
      return 'params'
    case 'procedure':
      return def.input ? 'input' : 'params'
    case 'subscription':
      return 'params'
    default:
      return 'record'
  }
}

const verdict = (lexicons, ref, kind, value) => {
  try {
    const lex = new Lexicons()
    for (const doc of lexicons) lex.add(structuredClone(doc))
    const lexValue = jsonToLex(structuredClone(value))
    if (kind === 'params') lex.assertValidXrpcParams(ref, lexValue)
    else if (kind === 'input') lex.assertValidXrpcInput(ref, lexValue)
    else {
      const res = lex.validate(ref, lexValue)
      if (!res.success) return { valid: false, error: res.error.message }
    }
    return { valid: true }
  } catch (e) {
    return { valid: false, error: e.message }
  }
}

const doc = (id, main, defs = {}) => ({ lexicon: 1, id, defs: { main, ...defs } })
const record = (properties, extra = {}) => ({
  type: 'record',
  key: 'tid',
  record: { type: 'object', properties, ...extra },
})
const one = (name, field, value, extra) => ({
  name,
  lexicons: [doc('test.own.case', record({ v: field }, extra))],
  collection: 'test.own.case',
  record: value === undefined ? {} : { v: value },
})
const RAW = 'bafkreie5737gdxlw5i64vzichcalba3z2v5n6icifvx5xytvske7mr3hpm'
const CBOR = 'bafyreie5737gdxlw5i64vzichcalba3z2v5n6icifvx5xytvske7mr3hpm'

// This crate's own: what the cases above leave out.
const own = [
  one('own-valid-utf8-bytes-within', { type: 'string', maxLength: 6 }, 'ééé'),
  one('own-invalid-utf8-bytes-over', { type: 'string', maxLength: 5 }, 'ééé'),
  one('own-invalid-utf8-bytes-under', { type: 'string', minLength: 4 }, 'éa'),
  one('own-valid-graphemes-emoji', { type: 'string', maxGraphemes: 2 }, '👨‍👩‍👧‍👦🏳️‍🌈'),
  one('own-invalid-graphemes-emoji', { type: 'string', maxGraphemes: 1 }, '👨‍👩‍👧‍👦🏳️‍🌈'),
  one('own-valid-graphemes-min', { type: 'string', minGraphemes: 2 }, 'éa'),
  one('own-invalid-graphemes-min-combining', { type: 'string', minGraphemes: 2 }, 'é'),
  one('own-valid-string-const', { type: 'string', const: 'x' }, 'x'),
  one('own-invalid-string-const', { type: 'string', const: 'x' }, 'y'),
  one('own-valid-integer-const', { type: 'integer', const: 3 }, 3),
  one('own-invalid-integer-const', { type: 'integer', const: 3 }, 4),
  one('own-valid-integer-enum', { type: 'integer', enum: [1, 2] }, 2),
  one('own-invalid-integer-enum', { type: 'integer', enum: [1, 2] }, 5),
  one('own-invalid-integer-above', { type: 'integer', maximum: 10 }, 11),
  one('own-invalid-integer-float', { type: 'integer' }, 1.5),
  one('own-valid-boolean-const', { type: 'boolean', const: true }, true),
  one('own-invalid-boolean-const', { type: 'boolean', const: true }, false),
  one('own-invalid-boolean-string', { type: 'boolean' }, 'true'),
  one('own-valid-known-values-not-enforced', { type: 'string', knownValues: ['a'] }, 'b'),
  one('own-invalid-array-too-long', { type: 'array', items: { type: 'integer' }, maxLength: 1 }, [1, 2]),
  one('own-invalid-array-too-short', { type: 'array', items: { type: 'integer' }, minLength: 2 }, [1]),
  one('own-invalid-array-item', { type: 'array', items: { type: 'integer' } }, [1, 'x']),
  one('own-valid-nullable', { type: 'string' }, null, { nullable: ['v'] }),
  one('own-invalid-null-not-nullable', { type: 'string' }, null),
  one('own-invalid-required-missing', { type: 'string' }, undefined, { required: ['v'] }),
  one('own-invalid-required-null', { type: 'string' }, null, { required: ['v'] }),
  one('own-valid-required-null-nullable', { type: 'string' }, null, { required: ['v'], nullable: ['v'] }),
  one('own-valid-unknown-array', { type: 'unknown' }, [1]),
  one('own-valid-unknown-bytes', { type: 'unknown' }, { $bytes: 'AAEC' }),
  one('own-valid-bytes', { type: 'bytes', maxLength: 3 }, { $bytes: 'AAEC' }),
  one('own-invalid-bytes-too-long', { type: 'bytes', maxLength: 2 }, { $bytes: 'AAEC' }),
  one('own-invalid-bytes-extra-field', { type: 'bytes' }, { $bytes: 'AAEC', x: 1 }),
  one('own-valid-bytes-padded', { type: 'bytes' }, { $bytes: 'AAE=' }),
  one('own-valid-cid-link-raw', { type: 'cid-link' }, { $link: RAW }),
  one('own-invalid-cid-link-extra-field', { type: 'cid-link' }, { $link: RAW, x: 1 }),
  one('own-invalid-cid-link-string', { type: 'cid-link' }, RAW),
  one('own-invalid-cid-link-truncated', { type: 'cid-link' }, { $link: RAW.slice(0, -4) }),
  one('own-valid-cid-link-v0', { type: 'cid-link' }, { $link: 'QmbWqxBEKC3P8tqsKc98xmWNzrzDtRLMiMPL8wBuTGsMnR' }),
  one('own-valid-blob', { type: 'blob' }, { $type: 'blob', ref: { $link: RAW }, mimeType: 'image/png', size: 10 }),
  one('own-valid-blob-cbor-cid', { type: 'blob' }, { $type: 'blob', ref: { $link: CBOR }, mimeType: 'image/png', size: 10 }),
  one('own-valid-blob-legacy', { type: 'blob' }, { cid: RAW, mimeType: 'image/png' }),
  one('own-invalid-blob-extra-field', { type: 'blob' }, { $type: 'blob', ref: { $link: RAW }, mimeType: 'image/png', size: 10, x: 1 }),
  one('own-invalid-blob-ref-string', { type: 'blob' }, { $type: 'blob', ref: RAW, mimeType: 'image/png', size: 10 }),
  one('own-invalid-blob-size-string', { type: 'blob' }, { $type: 'blob', ref: { $link: RAW }, mimeType: 'image/png', size: '10' }),
  one('own-invalid-token-field', { type: 'ref', ref: 'test.own.tokens#t' }, 'test.own.tokens#t'),
  {
    name: 'own-valid-open-union-unlisted-type',
    lexicons: [
      doc('test.own.union', record({ v: { type: 'union', refs: ['#a'] } }), {
        a: { type: 'object', properties: { x: { type: 'integer' } } },
      }),
    ],
    collection: 'test.own.union',
    record: { v: { $type: 'test.own.other#b', x: 'not checked' } },
  },
  {
    name: 'own-invalid-union-member',
    lexicons: [
      doc('test.own.union', record({ v: { type: 'union', refs: ['#a'] } }), {
        a: { type: 'object', properties: { x: { type: 'integer' } } },
      }),
    ],
    collection: 'test.own.union',
    record: { v: { $type: 'test.own.union#a', x: 'no' } },
  },
  {
    name: 'own-valid-union-main-by-nsid',
    lexicons: [
      doc('test.own.union', record({ v: { type: 'union', refs: ['test.own.member'] } })),
      doc('test.own.member', { type: 'object', properties: { x: { type: 'integer' } } }),
    ],
    collection: 'test.own.union',
    record: { v: { $type: 'test.own.member#main', x: 1 } },
  },
  {
    name: 'own-invalid-closed-union',
    lexicons: [
      doc('test.own.union', record({ v: { type: 'union', refs: ['#a'], closed: true } }), {
        a: { type: 'object', properties: {} },
      }),
    ],
    collection: 'test.own.union',
    record: { v: { $type: 'test.own.union#b' } },
  },
  {
    name: 'own-valid-object-def',
    lexicons: [doc('test.own.object', { type: 'object', required: ['x'], properties: { x: { type: 'integer' } } })],
    collection: 'test.own.object',
    record: { x: 1 },
  },
  {
    name: 'own-invalid-input-path',
    lexicons: [
      doc('test.own.call', {
        type: 'procedure',
        input: { encoding: 'application/json', schema: { type: 'ref', ref: '#body' } },
      }, {
        body: { type: 'object', properties: { roles: { type: 'array', items: { type: 'ref', ref: '#role' } } } },
        role: { type: 'object', required: ['role'], properties: { role: { type: 'string', maxLength: 3 } } },
      }),
    ],
    collection: 'test.own.call',
    record: { roles: [{ role: 'a' }, { role: 'b' }, { role: 'long' }] },
  },
  {
    name: 'own-invalid-params-array-item',
    lexicons: [
      doc('test.own.query', {
        type: 'query',
        parameters: { type: 'params', properties: { n: { type: 'array', items: { type: 'integer', maximum: 3 } } } },
      }),
    ],
    collection: 'test.own.query',
    record: { n: [1, 4] },
  },
]
own.forEach((c) => {
  if (c.name.startsWith('own-invalid-token')) {
    c.lexicons.push(doc('test.own.tokens', { type: 'token', description: 'unused' }, { t: { type: 'token' } }))
  }
})

const cases = [...(await inputs('data')), ...(await inputs('format')), ...(await inputs('e2e')), ...own]
const seen = new Set()
const vectors = []
for (const c of cases) {
  if (seen.has(c.name)) continue // a set exported twice
  seen.add(c.name)
  const value = toJson(c.record)
  const kind = kindOf(c.lexicons, c.collection)
  vectors.push({
    name: c.name,
    lexicons: c.lexicons,
    ref: c.collection,
    kind,
    value,
    ...verdict(c.lexicons, c.collection, kind, value),
  })
}
console.log(JSON.stringify({ generatedFrom: `@atproto/lexicon ${version}`, vectors }, null, 2))
