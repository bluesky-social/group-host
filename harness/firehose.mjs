// Reads a host's firehose as atproto's own consumer does (@atproto/sync's
// Firehose, the one a Jetstream or an AppView reads with), and prints what
// it made of each event as a line of JSON: {seq, kind, op, uri} for a
// commit's op, {seq, kind} for another event, {error, cause} for one it
// could not read.
//
//   node harness/firehose.mjs <host url> <mode> <did>=<did:key> ...
//
// It reads from the start (cursor 0) until the firehose has been quiet for
// a second. <mode> is `authenticated`, checking each commit's signature
// and each op's proof against the key given for its repo, as a reader
// would resolve it from the repo's DID document; or `unauthenticated`,
// reading each commit's blocks and records alone, as a Jetstream does.
// GROUP_HOST_DEV_ENV names a built dev-env, whose @atproto/sync it uses.

import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const [url, mode, ...pairs] = process.argv.slice(2);
const keys = new Map(pairs.map((p) => p.split('=')));
const devEnv = process.env.GROUP_HOST_DEV_ENV;
const from = createRequire(devEnv ? join(resolve(devEnv), 'package.json') : import.meta.url);
const { Firehose } = await import(pathToFileURL(from.resolve('@atproto/sync')).href);

const idResolver = {
  did: {
    resolveAtprotoKey: async (did) => {
      const key = keys.get(did);
      if (!key) throw new Error(`no key for ${did}`);
      return key;
    },
    resolve: async () => null
  },
  handle: { resolve: async () => undefined }
};

let quiet;
const done = () => firehose.destroy().then(() => process.exit(0));
const say = (line) => {
  console.log(JSON.stringify(line));
  clearTimeout(quiet);
  quiet = setTimeout(done, 1000);
};
const firehose = new Firehose({
  idResolver,
  unauthenticatedCommits: mode === 'unauthenticated',
  unauthenticatedHandles: mode === 'unauthenticated',
  service: url.replace(/^http/, 'ws'),
  getCursor: () => 0,
  handleEvent: (evt) =>
    say(
      ['create', 'update', 'delete'].includes(evt.event)
        ? { seq: evt.seq, kind: 'commit', op: evt.event, uri: evt.uri.toString() }
        : { seq: evt.seq, kind: evt.event }
    ),
  onError: (err) => say({ error: err.message, cause: String(err.cause?.stack ?? err.cause) })
});
firehose.start();
quiet = setTimeout(done, 5000);
