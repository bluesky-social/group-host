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

import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const dir = join(root, 'lexicons');

/// Every lexicon file under `dir`, by its path from the repository root.
const files = (d) =>
  readdirSync(d, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(d, e.name)) : e.name.endsWith('.json') ? [join(d, e.name)] : []
  );

const git = (...args) =>
  execFileSync('git', args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });

// What is published is main's. A branch is judged from where it left
// main, on what it changed: a feature branch behind main has not taken
// back what main added since. A pull into main is checked merged with it,
// so that is main itself.
const base =
  process.argv[2] ??
  (() => {
    try {
      return git('merge-base', 'HEAD', 'origin/main').trim();
    } catch {
      return null;
    }
  })();

// Paths whose change from the base is let through. Once the base has none
// of it, an entry matches nothing and can go.
const retracted = new Set([
  'fyi.opensocial.membership#main.record.member',
  'fyi.opensocial.defs#actorLink',
  'fyi.opensocial.invite#main.record.by',
  'fyi.opensocial.profile#main.record.joinPolicy',
  'fyi.opensocial.listJoinRequests#main.output.requests[]',
  'fyi.opensocial.labeler#main.record.policies',
  'fyi.opensocial.labeler#main.record.subjectCollections[]',
  'fyi.opensocial.space#main.record.name',
  'fyi.opensocial.labeler#main.record.createdAt',
  'fyi.opensocial.profile#main.record.location',
  ...[
    'defs#joinRequest.actor',
    'defs#modSubjectView.actor',
    'defs#moderationEvent.creator',
    'getModSubjectHistory#main.output.record'
  ].map((at) => `fyi.opensocial.${at}`)
]);

// Where the opensocial proposal's shape, which the host takes, differs
// from the style above: each finding, exactly as said. A space policy's
// grant's scopes, and a role's scopes and assignable roles, are bare
// strings, as OAuth scopes and record keys are. A membership's roles are
// bare role keys, at most 100; a moderation subject's status is a closed
// set; and a report's and an event's `id` is the proposal's name for it.
// A rule's title is as long as the proposal's. A moderation subject is a
// URI, an account's DID or a record's, beside its version, not a union;
// and a report's reason, a labeler's reasons and its subject types, are
// plain words, not atproto's moderation defs.
const proposal = new Set([
  ...['maxLength went from 1000 to 640', 'maxGraphemes went from 100 to 64'].map(
    (m) => `fyi.opensocial.rule#main.record.title: ${m}: what was valid must stay so`
  ),
  'fyi.opensocial.policy#grant.scopes: an array holds objects, which can grow, not bare values',
  ...['scopes', 'assignable'].map(
    (f) =>
      `fyi.opensocial.role#main.record.${f}: an array holds objects, which can grow, not bare values`
  ),
  'fyi.opensocial.membership#main.record.roles: maxLength went from none to 100: what was valid must stay so',
  'fyi.opensocial.membership#main.record.roles[]: was {"type":"ref","ref":"fyi.opensocial.defs#roleId"}, now {"type":"string","format":"record-key"}: a field is never retyped',
  ...['createReport', 'defs'].map(
    (n) => `fyi.opensocial.${n}: field-name-syntax: name is a commonly reserved keyword: id`
  ),
  ...[
    [
      'createReport#main.input.subject',
      '{"type":"union","refs":["fyi.opensocial.defs#accountSubject","fyi.opensocial.defs#recordSubject"]}',
      '{"type":"string","format":"uri"}'
    ],
    ...[
      'createReport#main.input.reasonType',
      'defs#reportEvent.reasonType',
      'labeler#main.record.reasonTypes[]'
    ].map((p) => [
      p,
      '{"type":"ref","ref":"com.atproto.moderation.defs#reasonType"}',
      '{"type":"string"}'
    ]),
    [
      'labeler#main.record.subjectTypes[]',
      '{"type":"ref","ref":"com.atproto.moderation.defs#subjectType"}',
      '{"type":"string"}'
    ],
    [
      'defs#statusEvent.status',
      '{"type":"ref","ref":"fyi.opensocial.defs#subjectStatus"}',
      '{"type":"string"}'
    ]
  ].map(([p, was, now]) => `fyi.opensocial.${p}: was ${was}, now ${now}: a field is never retyped`),
  ...[
    'defs#modSubjectView.status',
    'defs#statusEvent.status',
    'listModSubjects#main.parameters.status',
    'resolveModSubject#main.input.status'
  ].map((p) => `fyi.opensocial.${p}: knownValues, not enum: an enum can never grow`)
]);

const problems = [];
const say = (at, what) =>
  retracted.has(at) || proposal.has(`${at}: ${what}`) || problems.push(`${at}: ${what}`);

// goat (github.com/bluesky-social/goat), one finding a line.
// The schemas held to the guide are this host's own. Those under
// lexicons/upstream are others', copied as they are for what ours refer
// to, and are only checked to grow from main's.
const ours = (id) => typeof id === 'string' && id.startsWith('fyi.opensocial.');

// What the space alpha brings, which goat does not know yet: a space's
// reference (format `space-ref`), and a permission over a space
// (`resource: space`). Both are the alpha's spec, not a style to fix.
const alpha = (found) =>
  found.message === 'unknown string format: space-ref' ||
  found.message === 'unsupported permission resource: space';
let linted;
try {
  linted = execFileSync('goat', ['lex', 'lint', '--json', 'lexicons/'], {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'ignore']
  });
} catch (e) {
  if (e.code === 'ENOENT') {
    console.log(
      'goat is not installed: brew install goat, or a release from github.com/bluesky-social/goat'
    );
    process.exit(1);
  }
  linted = e.stdout;
}
// goat 0.2.6 knows no space lexicon (`"type": "space"`, from the
// permissioned-data alpha) and refuses the whole file, so those files are
// checked below instead, against what the alpha's @atproto/lex-document
// takes. Only that refusal is let go, and only for a file whose main is a
// space: any other finding of it stands.
const spaces = new Set(
  files(dir)
    .filter((f) => JSON.parse(readFileSync(f, 'utf8')).defs?.main?.type === 'space')
    .map((f) => relative(root, f))
);
const unknownSpace = (found) =>
  found['lint-name'] === 'schema-json-parse' &&
  found.message === 'unexpected schema type: space' &&
  spaces.has(found['file-path']);
for (const line of linted.split('\n').filter(Boolean)) {
  const found = JSON.parse(line);
  if (unknownSpace(found) || alpha(found) || !ours(found.nsid)) continue;
  say(found.nsid, `${found['lint-name']}: ${found.message}`);
}

/// Walks every schema in `def`, calling `visit(schema, path)`.
const walk = (schema, path, visit) => {
  if (!schema || typeof schema !== 'object') return;
  visit(schema, path);
  if (schema.record) walk(schema.record, `${path}.record`, visit);
  for (const [k, v] of Object.entries(schema.properties ?? {})) walk(v, `${path}.${k}`, visit);
  if (schema.items) walk(schema.items, `${path}[]`, visit);
  for (const part of ['parameters', 'input', 'output', 'message']) {
    const p = schema[part];
    if (p) walk(p.schema ?? p, `${path}.${part}`, visit);
  }
};

// The style CLAUDE.md asks for, of every schema now.
const now = new Map();
for (const file of files(dir)) {
  const doc = JSON.parse(readFileSync(file, 'utf8'));
  now.set(relative(root, file), doc);
  if (!ours(doc.id)) continue;
  // What people write: records, the shapes beside them, and what is sent
  // in. What the server answers it bounds itself, as it does a view, which
  // only an answer holds, wherever it is defined.
  const written = doc.defs?.main?.type === 'record' || doc.defs?.main?.type === undefined;
  for (const [name, def] of Object.entries(doc.defs ?? {})) {
    const at = `${doc.id}#${name}`;
    if (name === 'main' && !def.description) say(at, 'a main def needs a description');
    const view = /View(Basic|Detailed)?$/.test(name);
    walk(def, at, (s, path) => {
      const kept = (written && !view) || path.includes('.input');
      if (s.enum) say(path, 'knownValues, not enum: an enum can never grow');
      if (
        kept &&
        s.type === 'string' &&
        !s.format &&
        !s.enum &&
        !s.knownValues &&
        s.maxLength === undefined &&
        s.const === undefined
      )
        say(path, 'a string without a format needs a maxLength');
      if (s.type === 'union' && s.closed) say(path, 'unions stay open');
      // An account is named by its DID. Its own handle, as `handle`, beside
      // its DID or as the name being chosen, is the one place a handle goes.
      if (s.type === 'string' && s.format === 'handle' && !path.endsWith('.handle'))
        say(path, 'accounts by DID, not handle');
    });
  }
}

// A space lexicon, as the alpha's lexiconSpaceSchema takes it: a key, a
// title of 1 to 64 characters for consent screens, and its collections,
// each a record lexicon here. It is the file's only def, as goat, which
// skips the file, lints none of it.
const nsids = new Set(
  [...now.values()].filter((d) => d.defs?.main?.type === 'record').map((d) => d.id)
);
for (const file of spaces) {
  const doc = now.get(file);
  const at = `${doc.id}#main`;
  const def = doc.defs.main;
  if (Object.keys(doc.defs).length > 1) say(doc.id, 'a space lexicon has its main def alone');
  if (!/^(any|nsid|tid|literal:.+)$/.test(def.key ?? ''))
    say(at, 'a space needs a key: any, nsid, tid or literal:<value>');
  if (typeof def.title !== 'string' || def.title.length < 1 || def.title.length > 64)
    say(at, 'a space needs a title of 1 to 64 characters');
  if (!Array.isArray(def.collections)) say(at, 'a space lists its collections');
  for (const c of def.collections ?? [])
    if (!nsids.has(c)) say(`${at}.collections`, `${c} is not a record lexicon here`);
}

// What each schema in `base` was, against what it is now.
const shape = (s) =>
  JSON.stringify({ type: s.type, format: s.format, ref: s.ref, refs: s.refs?.slice().sort() });
const loosened = (before, after, path) => {
  if (!before || !after) return;
  if (shape(before) !== shape(after) && !(before.type === 'union' && after.type === 'union')) {
    say(path, `was ${shape(before)}, now ${shape(after)}: a field is never retyped`);
    return;
  }
  if (before.type === 'union') {
    for (const r of before.refs ?? [])
      if (!(after.refs ?? []).includes(r)) say(path, `the union lost ${r}`);
  }
  for (const k of ['maxLength', 'maxGraphemes', 'maxSize', 'maximum']) {
    if (after[k] !== undefined && (before[k] === undefined || after[k] < before[k]))
      say(
        path,
        `${k} went from ${before[k] ?? 'none'} to ${after[k]}: what was valid must stay so`
      );
  }
  for (const k of ['minLength', 'minGraphemes', 'minimum']) {
    if (after[k] !== undefined && (before[k] === undefined || after[k] > before[k]))
      say(
        path,
        `${k} went from ${before[k] ?? 'none'} to ${after[k]}: what was valid must stay so`
      );
  }
  if (before.enum)
    for (const v of before.enum)
      if (!(after.enum ?? [v]).includes(v)) say(path, `the enum lost ${v}`);
  if (before.const !== undefined && after.const !== before.const)
    say(path, 'a const never changes');
  const was = new Set(before.required ?? []);
  for (const r of after.required ?? [])
    if (!was.has(r)) say(path, `${r} is required now: only new optional fields`);
  for (const [k, v] of Object.entries(before.properties ?? {})) {
    if (!after.properties?.[k]) {
      if (!mayGo(path, k)) say(`${path}.${k}`, 'a field is never removed or renamed');
    } else loosened(v, after.properties[k], `${path}.${k}`);
  }
  loosened(before.items, after.items, `${path}[]`);
  loosened(before.record, after.record, `${path}.record`);
  for (const part of ['parameters', 'input', 'output', 'message']) {
    const b = before[part]?.schema ?? before[part];
    const a = after[part]?.schema ?? after[part];
    if (b && !a) say(`${path}.${part}`, 'removed');
    else if (b?.type) loosened(b, a, `${path}.${part}`);
  }
};

// Schemas taken out on purpose, each named here as it goes, which a
// change may remove, and a space type's collections may drop: what is
// said of each is why.
// - com.atproto.moderation.createReport: reports come to the group's own
//   fyi.opensocial.createReport, as the opensocial proposal has them.
// - com.atproto.managedspace.checkAccess: an inbox is a memberspace its
//   owner's app makes, which asks no one.
// - com.atproto.label.label: atproto names the label record
//   com.atproto.label.record, as the opensocial proposal does.
// - com.atproto.moderation.defs: a report's reason and a labeler's are
//   plain words, as the opensocial proposal has them, and nothing else
//   here refers to it.
// - fyi.opensocial.permissions: each role record carries its own
//   group-wide scopes and assignable roles, as the opensocial proposal's
//   do, so the members space keeps no record of them all.
// - fyi.opensocial.access: each space's policy (fyi.opensocial.policy),
//   kept by the host and published as com.atproto.space.policy, took its
//   place, as the opensocial proposal has it.
// - fyi.opensocial.configure, manageSpaces, manageMembers and moderate:
//   tiered as authManageProfile, within authModerate, within
//   authManageGroup, and participate named authParticipate, as the
//   opensocial proposal's permission sets are.
// - community.lexicon.location.address, geo, fsq and hthree: a group's
//   profile no longer says where it meets, so nothing here refers to them.
const RETIRED = new Set([
  'com.atproto.moderation.createReport',
  'com.atproto.managedspace.checkAccess',
  'com.atproto.label.label',
  'com.atproto.moderation.defs',
  'fyi.opensocial.permissions',
  'fyi.opensocial.access',
  'fyi.opensocial.configure',
  'fyi.opensocial.manageSpaces',
  'fyi.opensocial.manageMembers',
  'fyi.opensocial.moderate',
  'fyi.opensocial.participate',
  'community.lexicon.location.address',
  'community.lexicon.location.geo',
  'community.lexicon.location.fsq',
  'community.lexicon.location.hthree'
]);

// None of ours is published yet, so nothing out there was written to it.
// Until one is, a schema, a def or a field may go when nothing here refers
// to it: no lexicon refs it, and the code never names it. One still used
// stays, as it would once published. Set this once the first is published.
const PUBLISHED = false;

/// What the code reads: the host, the command, the console and the harness.
const code = git('ls-files', '--', 'crates', 'web/src', 'harness')
  .split('\n')
  .filter(
    (f) =>
      /\.(rs|ts|svelte|mjs|js)$/.test(f) &&
      !f.startsWith('web/src/lib/lexicons') &&
      f !== 'harness/lexicons.mjs'
  )
  .map((f) => readFileSync(join(root, f), 'utf8'))
  .join('\n');

/// Whether `text` names `id` whole: not as the start of a longer name, as
/// `fyi.opensocial.labeler` starts with `fyi.opensocial.label`.
const names = (text, id, before = '') =>
  new RegExp(`${before}${id.replace(/[.#]/g, '\\$&')}(?![\\w.])`).test(text);

/// Whether a def (`nsid#name`, or a schema's `nsid`) is referred to: by a
/// ref in any lexicon, `#name` within its own, or by name in the code.
const referred = (id) => {
  const [nsid, name] = id.split('#');
  for (const doc of now.values()) {
    const text = JSON.stringify(doc);
    if (doc.id === nsid ? name && text.includes(`"#${name}"`) : names(text, id, '"')) return true;
  }
  return names(code, id);
};

/// Whether a removal goes unremarked: of one of ours, nothing published,
/// and nothing referring to it. A field is referred to by its name, quoted.
const mayGo = (id, field) =>
  ours(id.split('#')[0]) && !PUBLISHED && (field ? !code.includes(`"${field}"`) : !referred(id));

if (base) {
  const listed = git('ls-tree', '-r', '--name-only', base, '--', 'lexicons')
    .split('\n')
    .filter((f) => f.endsWith('.json'));
  for (const file of listed) {
    const before = JSON.parse(git('show', `${base}:${file}`));
    const after = now.get(file);
    if (!after && (RETIRED.has(before.id) || mayGo(before.id))) continue;
    if (!after) {
      say(before.id, `removed since ${base}`);
      continue;
    }
    for (const [name, def] of Object.entries(before.defs ?? {})) {
      const at = `${before.id}#${name}`;
      if (!after.defs?.[name]) {
        if (!mayGo(at)) say(at, `removed since ${base}`);
      } else loosened(def, after.defs[name], at);
      // A scope naming a space type alone is granted the collections its
      // lexicon lists, so taking one out takes writes from those logins.
      if (def.type === 'space')
        for (const c of def.collections ?? [])
          if (!RETIRED.has(c) && !(after.defs?.[name]?.collections ?? []).includes(c))
            say(`${at}.collections`, `${c} was taken out`);
    }
  }
}

// Arrays hold objects, which can grow fields, rather than bare values,
// which cannot: of arrays new since `base`, as those before it are kept.
// A query's parameters are the exception: XRPC lets them hold only values.
if (base) {
  const published = new Set();
  const listed = git('ls-tree', '-r', '--name-only', base, '--', 'lexicons')
    .split('\n')
    .filter((f) => f.endsWith('.json'));
  for (const file of listed) {
    const before = JSON.parse(git('show', `${base}:${file}`));
    for (const [name, def] of Object.entries(before.defs ?? {}))
      walk(def, `${before.id}#${name}`, (_, path) => published.add(path));
  }
  for (const doc of now.values()) {
    if (!ours(doc.id)) continue;
    for (const [name, def] of Object.entries(doc.defs ?? {})) {
      walk(def, `${doc.id}#${name}`, (s, path) => {
        const param = path.includes('.parameters.');
        const bare =
          s.type === 'array' && !param && !['object', 'ref', 'union'].includes(s.items?.type);
        if (bare && !published.has(path))
          say(path, 'an array holds objects, which can grow, not bare values');
      });
    }
  }
}

for (const p of problems) console.log(p);
if (problems.length) process.exit(1);
console.log(`lexicons: as the guide asks${base ? `, and only grown since ${base}` : ''}`);
