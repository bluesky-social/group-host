// The lexicons, as the Lexicon Style Guide and CLAUDE.md ("Lexicons") ask:
// goat's lint; what that lint does
// not see; and that each one only grows from what `base` has, since a
// schema already published is evolved, never changed, with records and
// clients written to it out there.
//
//   node harness/lexicons.mjs [base]    base: a git revision; where this
//                                       branch left origin/main if that
//                                       exists, else none
//
// Prints each problem as `nsid#def.path: what`, failing if there is any.

import { execFileSync } from 'node:child_process'
import { readdirSync, readFileSync } from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const dir = join(root, 'lexicons')

/// Every lexicon file under `dir`, by its path from the repository root.
const files = (d) =>
  readdirSync(d, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(d, e.name)) : e.name.endsWith('.json') ? [join(d, e.name)] : [],
  )

const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] })

// What is published is main's. A branch is judged from where it left
// main, on what it changed: a feature branch behind main has not taken
// back what main added since. A pull into main is checked merged with it,
// so that is main itself.
const base = process.argv[2] ?? (() => {
  try {
    return git('merge-base', 'HEAD', 'origin/main').trim()
  } catch {
    return null
  }
})()

// What was taken back on purpose before any record used it, each with why.
// Once the base has none of it, an entry matches nothing and can go.
// - What the host never answered or read, taken out before any client
//   was written to it: actorView's profile fields (it answers a DID and a
//   handle, from the DID document), and listSpaceRecords' `rev` and
//   `actors`.
const retracted = new Set([
  ...['displayName', 'description', 'avatar', 'pronouns', 'links', 'bot'].map((f) => `fyi.opensocial.defs#actorView.${f}`),
  'fyi.opensocial.defs#actorLink',
  'fyi.opensocial.listSpaceRecords#main.output.rev',
  'fyi.opensocial.listSpaceRecords#main.output.actors',
  'fyi.opensocial.invite#main.record.by',
  'fyi.opensocial.getActorInvites#actorInvite.by',
  ...['createGroup#main.input', 'updateProfile#main.input', 'profile#main.record'].map((at) => `fyi.opensocial.${at}.joinPolicy`),
  'fyi.opensocial.defs#viewerState.request',
  'fyi.opensocial.getGroup#main.output.joinRequests[]',
  'fyi.opensocial.getJoinRequest#main.output.request',
  'fyi.opensocial.listJoinRequests#main.output.requests[]',
  'fyi.opensocial.defs#subjectView',
  'fyi.opensocial.defs#subjectView.subject',
  'fyi.opensocial.defs#subjectView.status',
  'fyi.opensocial.defs#subjectView.reportCount',
  'fyi.opensocial.defs#subjectView.lastReportedAt',
  'fyi.opensocial.getSubjectHistory#main.output.subject',
  'fyi.opensocial.getSubjectHistory#main.output.events[]',
  'fyi.opensocial.labeler#main.record.policies',
  'fyi.opensocial.labeler#main.record.subjectCollections[]',
  'fyi.opensocial.listSubjects#main.parameters.limit',
  'fyi.opensocial.resolveSubject#main.input',
  'fyi.opensocial.resolveSubject#main.input.subject',
  'fyi.opensocial.resolveSubject#main.input.action',
  'fyi.opensocial.resolveSubject#main.output',
  ...['readableBy', 'credentialScopes', 'createdAt'].map((f) => `fyi.opensocial.access#main.record.${f}`),
  ...['readableBy', 'credentialScopes', 'writableBy', 'spaceType'].map((f) => `fyi.opensocial.createSpace#main.input.${f}`),
  'fyi.opensocial.createSpace#main.output.uri',
  ...['readableBy', 'credentialScopes', 'writableBy'].map((f) => `fyi.opensocial.updateSpace#main.input.${f}`),
  ...['readableBy', 'credentialScopes'].map((f) => `fyi.opensocial.defs#spaceView.${f}`),
])

// Where the opensocial proposal's shape, which the host takes, differs
// from the style above: each finding, exactly as said. A space grant's
// scopes are bare strings, as OAuth scopes are; createSpace names a
// space's `type` so; and the access record and createSpace require what
// the proposal's do. A membership's roles are bare role ids, at most 100;
// a moderation subject's status is a closed set; and a report's and an
// event's `id` is the proposal's name for it.
const proposal = new Set([
  'fyi.opensocial.defs#spaceGrant.scopes: an array holds objects, which can grow, not bare values',
  'fyi.opensocial.createSpace: field-name-syntax: name is a commonly reserved keyword: type',
  'fyi.opensocial.access#main.record: grants is required now: only new optional fields',
  ...['type', 'access'].map((f) => `fyi.opensocial.createSpace#main.input: ${f} is required now: only new optional fields`),
  ...['space', 'access'].map((f) => `fyi.opensocial.createSpace#main.output: ${f} is required now: only new optional fields`),
  'fyi.opensocial.membership#main.record.roles: maxLength went from none to 100: what was valid must stay so',
  'fyi.opensocial.membership#main.record.roles[]: was {"type":"ref","ref":"fyi.opensocial.defs#roleRef"}, now {"type":"ref","ref":"fyi.opensocial.defs#roleId"}: a field is never retyped',
  ...['createReport', 'defs'].map((n) => `fyi.opensocial.${n}: field-name-syntax: name is a commonly reserved keyword: id`),
  ...['defs#subjectStatus', 'listSubjects#main.parameters.status', 'resolveSubject#main.input.status'].map(
    (p) => `fyi.opensocial.${p}: knownValues, not enum: an enum can never grow`,
  ),
])

const problems = []
const say = (at, what) => retracted.has(at) || proposal.has(`${at}: ${what}`) || problems.push(`${at}: ${what}`)

// goat (github.com/bluesky-social/goat), one finding a line.
// The schemas held to the guide are this host's own. Those under
// lexicons/upstream and community.lexicon.* are others', copied as they
// are for what ours refer to, and are only checked to grow from main's.
const ours = (id) => typeof id === 'string' && id.startsWith('fyi.opensocial.')

// What the space alpha brings, which goat does not know yet: a space's
// reference (format `space-ref`), and a permission over a space
// (`resource: space`). Both are the alpha's spec, not a style to fix.
const alpha = (found) =>
  found.message === 'unknown string format: space-ref' || found.message === 'unsupported permission resource: space'
let linted
try {
  linted = execFileSync('goat', ['lex', 'lint', '--json', 'lexicons/'], { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] })
} catch (e) {
  if (e.code === 'ENOENT') {
    console.log('goat is not installed: brew install goat, or a release from github.com/bluesky-social/goat')
    process.exit(1)
  }
  linted = e.stdout
}
// goat 0.2.6 knows no space lexicon (`"type": "space"`, from the
// permissioned-data alpha) and refuses the whole file, so those files are
// checked below instead, against what the alpha's @atproto/lex-document
// takes. Only that refusal is let go, and only for a file whose main is a
// space: any other finding of it stands.
const spaces = new Set(
  files(dir).filter((f) => JSON.parse(readFileSync(f, 'utf8')).defs?.main?.type === 'space').map((f) => relative(root, f)),
)
const unknownSpace = (found) =>
  found['lint-name'] === 'schema-json-parse' && found.message === 'unexpected schema type: space' && spaces.has(found['file-path'])
for (const line of linted.split('\n').filter(Boolean)) {
  const found = JSON.parse(line)
  if (unknownSpace(found) || alpha(found) || !ours(found.nsid)) continue
  say(found.nsid, `${found['lint-name']}: ${found.message}`)
}

/// Walks every schema in `def`, calling `visit(schema, path)`.
const walk = (schema, path, visit) => {
  if (!schema || typeof schema !== 'object') return
  visit(schema, path)
  if (schema.record) walk(schema.record, `${path}.record`, visit)
  for (const [k, v] of Object.entries(schema.properties ?? {})) walk(v, `${path}.${k}`, visit)
  if (schema.items) walk(schema.items, `${path}[]`, visit)
  for (const part of ['parameters', 'input', 'output', 'message']) {
    const p = schema[part]
    if (p) walk(p.schema ?? p, `${path}.${part}`, visit)
  }
}

// The style CLAUDE.md asks for, of every schema now.
const now = new Map()
for (const file of files(dir)) {
  const doc = JSON.parse(readFileSync(file, 'utf8'))
  now.set(relative(root, file), doc)
  if (!ours(doc.id)) continue
  // What people write: records, the shapes beside them, and what is sent
  // in. What the server answers it bounds itself, as it does a view, which
  // only an answer holds, wherever it is defined.
  const written = doc.defs?.main?.type === 'record' || doc.defs?.main?.type === undefined
  for (const [name, def] of Object.entries(doc.defs ?? {})) {
    const at = `${doc.id}#${name}`
    if (name === 'main' && !def.description) say(at, 'a main def needs a description')
    const view = /View(Basic|Detailed)?$/.test(name)
    walk(def, at, (s, path) => {
      const kept = (written && !view) || path.includes('.input')
      if (s.enum) say(path, 'knownValues, not enum: an enum can never grow')
      if (kept && s.type === 'string' && !s.format && !s.enum && !s.knownValues && s.maxLength === undefined && s.const === undefined)
        say(path, 'a string without a format needs a maxLength')
      if (s.type === 'union' && s.closed) say(path, 'unions stay open')
      // An account is named by its DID. Its own handle, as `handle`, beside
      // its DID or as the name being chosen, is the one place a handle goes.
      if (s.type === 'string' && s.format === 'handle' && !path.endsWith('.handle'))
        say(path, 'accounts by DID, not handle')
    })
  }
}

// A space lexicon, as the alpha's lexiconSpaceSchema takes it: a key, a
// name of 1 to 64 characters for consent screens, and its collections,
// each a record lexicon here. It is the file's only def, as goat, which
// skips the file, lints none of it.
const nsids = new Set([...now.values()].filter((d) => d.defs?.main?.type === 'record').map((d) => d.id))
for (const file of spaces) {
  const doc = now.get(file)
  const at = `${doc.id}#main`
  const def = doc.defs.main
  if (Object.keys(doc.defs).length > 1) say(doc.id, 'a space lexicon has its main def alone')
  if (!/^(any|nsid|tid|literal:.+)$/.test(def.key ?? '')) say(at, 'a space needs a key: any, nsid, tid or literal:<value>')
  if (typeof def.name !== 'string' || def.name.length < 1 || def.name.length > 64) say(at, 'a space needs a name of 1 to 64 characters')
  if (!Array.isArray(def.collections)) say(at, 'a space lists its collections')
  for (const c of def.collections ?? []) if (!nsids.has(c)) say(`${at}.collections`, `${c} is not a record lexicon here`)
}

// What each schema in `base` was, against what it is now.
const shape = (s) => JSON.stringify({ type: s.type, format: s.format, ref: s.ref, refs: s.refs?.slice().sort() })
const loosened = (before, after, path) => {
  if (!before || !after) return
  if (shape(before) !== shape(after) && !(before.type === 'union' && after.type === 'union')) {
    say(path, `was ${shape(before)}, now ${shape(after)}: a field is never retyped`)
    return
  }
  if (before.type === 'union') {
    for (const r of before.refs ?? []) if (!(after.refs ?? []).includes(r)) say(path, `the union lost ${r}`)
  }
  for (const k of ['maxLength', 'maxGraphemes', 'maxSize', 'maximum']) {
    if (after[k] !== undefined && (before[k] === undefined || after[k] < before[k]))
      say(path, `${k} went from ${before[k] ?? 'none'} to ${after[k]}: what was valid must stay so`)
  }
  for (const k of ['minLength', 'minGraphemes', 'minimum']) {
    if (after[k] !== undefined && (before[k] === undefined || after[k] > before[k]))
      say(path, `${k} went from ${before[k] ?? 'none'} to ${after[k]}: what was valid must stay so`)
  }
  if (before.enum) for (const v of before.enum) if (!(after.enum ?? [v]).includes(v)) say(path, `the enum lost ${v}`)
  if (before.const !== undefined && after.const !== before.const) say(path, 'a const never changes')
  const was = new Set(before.required ?? [])
  for (const r of after.required ?? []) if (!was.has(r)) say(path, `${r} is required now: only new optional fields`)
  for (const [k, v] of Object.entries(before.properties ?? {})) {
    if (!after.properties?.[k]) {
      if (!mayGo(path, k)) say(`${path}.${k}`, 'a field is never removed or renamed')
    }
    else loosened(v, after.properties[k], `${path}.${k}`)
  }
  loosened(before.items, after.items, `${path}[]`)
  loosened(before.record, after.record, `${path}.record`)
  for (const part of ['parameters', 'input', 'output', 'message']) {
    const b = before[part]?.schema ?? before[part]
    const a = after[part]?.schema ?? after[part]
    if (b && !a) say(`${path}.${part}`, 'removed')
    else if (b?.type) loosened(b, a, `${path}.${part}`)
  }
}

// Schemas taken out on purpose, each named here as it goes, which a
// change may remove: what is said of each is why.
// - com.atproto.moderation.createReport: reports come to the group's own
//   fyi.opensocial.createReport, as the opensocial proposal has them.
const RETIRED = new Set(['com.atproto.moderation.createReport'])

// None of ours is published yet, so nothing out there was written to it.
// Until one is, a schema, a def or a field may go when nothing here refers
// to it: no lexicon refs it, and the code never names it. One still used
// stays, as it would once published. Set this once the first is published.
const PUBLISHED = false

/// What the code reads: the host, the command, the console and the harness.
const code = git('ls-files', '--', 'crates', 'web/src', 'harness')
  .split('\n')
  .filter((f) => /\.(rs|ts|svelte|mjs|js)$/.test(f) && !f.startsWith('web/src/lib/lexicons') && f !== 'harness/lexicons.mjs')
  .map((f) => readFileSync(join(root, f), 'utf8'))
  .join('\n')

/// Whether a def (`nsid#name`, or a schema's `nsid`) is referred to: by a
/// ref in any lexicon, `#name` within its own, or by name in the code.
const referred = (id) => {
  const [nsid, name] = id.split('#')
  for (const doc of now.values()) {
    const text = JSON.stringify(doc)
    if (doc.id === nsid ? name && text.includes(`"#${name}"`) : text.includes(`"${id}`)) return true
  }
  return code.includes(id)
}

/// Whether a removal goes unremarked: of one of ours, nothing published,
/// and nothing referring to it. A field is referred to by its name, quoted.
const mayGo = (id, field) => ours(id.split('#')[0]) && !PUBLISHED && (field ? !code.includes(`"${field}"`) : !referred(id))

if (base) {
  const listed = git('ls-tree', '-r', '--name-only', base, '--', 'lexicons').split('\n').filter((f) => f.endsWith('.json'))
  for (const file of listed) {
    const before = JSON.parse(git('show', `${base}:${file}`))
    const after = now.get(file)
    if (!after && (RETIRED.has(before.id) || mayGo(before.id))) continue
    if (!after) {
      say(before.id, `removed since ${base}`)
      continue
    }
    for (const [name, def] of Object.entries(before.defs ?? {})) {
      const at = `${before.id}#${name}`
      if (!after.defs?.[name]) {
        if (!mayGo(at)) say(at, `removed since ${base}`)
      }
      else loosened(def, after.defs[name], at)
      // A scope naming a space type alone is granted the collections its
      // lexicon lists, so taking one out takes writes from those logins.
      if (def.type === 'space')
        for (const c of def.collections ?? [])
          if (!(after.defs?.[name]?.collections ?? []).includes(c)) say(`${at}.collections`, `${c} was taken out`)
    }
  }
}

// Arrays hold objects, which can grow fields, rather than bare values,
// which cannot: of arrays new since `base`, as those before it are kept.
// A query's parameters are the exception: XRPC lets them hold only values.
if (base) {
  const published = new Set()
  const listed = git('ls-tree', '-r', '--name-only', base, '--', 'lexicons').split('\n').filter((f) => f.endsWith('.json'))
  for (const file of listed) {
    const before = JSON.parse(git('show', `${base}:${file}`))
    for (const [name, def] of Object.entries(before.defs ?? {})) walk(def, `${before.id}#${name}`, (_, path) => published.add(path))
  }
  for (const doc of now.values()) {
    if (!ours(doc.id)) continue
    for (const [name, def] of Object.entries(doc.defs ?? {})) {
      walk(def, `${doc.id}#${name}`, (s, path) => {
        const param = path.includes('.parameters.')
        const bare = s.type === 'array' && !param && !['object', 'ref', 'union'].includes(s.items?.type)
        if (bare && !published.has(path)) say(path, 'an array holds objects, which can grow, not bare values')
      })
    }
  }
}

for (const p of problems) console.log(p)
if (problems.length) process.exit(1)
console.log(`lexicons: as the guide asks${base ? `, and only grown since ${base}` : ''}`)
