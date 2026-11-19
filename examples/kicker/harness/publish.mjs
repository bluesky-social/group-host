// `just publish <handle>`: Kicker on a group host that is running for
// real, as the founder makes it with the `opensocial` command. The first
// time, the place itself, as seed.mjs makes it (`make`): the group
// `handle`, its host role, its room and calendar, and its label. Every
// time, the site as built (`publish`), in place of the one there. Nobody
// else is made, and nothing is written in its spaces: its members do that.
//
// The command signs in as OPENSOCIAL_AS (a handle) with OPENSOCIAL_PASSWORD
// at PDS_URL, on the host OPENSOCIAL_HOST, as it does anywhere
// (docs/reference/cli.md).

import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { make, publish } from './seed.mjs';
import { groupHost } from './world.mjs';

const handle = process.argv[2];
const host = process.env.OPENSOCIAL_HOST;
if (!handle || !host) {
  console.error(
    'usage: OPENSOCIAL_HOST=… PDS_URL=… OPENSOCIAL_AS=… OPENSOCIAL_PASSWORD=… just publish <handle>'
  );
  process.exit(2);
}
const run = (...args) =>
  execFileSync(join(groupHost, 'target/debug/opensocial'), args, {
    stdio: ['ignore', 'inherit', 'inherit']
  });

const found = await fetch(`${host}/xrpc/com.atproto.identity.resolveHandle?handle=${handle}`);
if (!found.ok) {
  console.log(`→ making ${handle}`);
  make(run, handle);
}
console.log(`→ publishing its site`);
publish(run, handle);
console.log(`\n${handle} is at https://${handle}`);
