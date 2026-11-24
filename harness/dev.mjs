// `just dev`: a world to use the console in (world.mjs), for a person to
// look at: the reference PDS with alice, bob and harbor on it, the host, the
// console at https://group-host.localtest.me, served by the dev server so a
// change to it shows at once, a group of alice's, and one of bob's that has
// invited her. Groups are named under groups.localtest.me, so a group's
// address, which sends a browser to its page here, opens in one too.
// Stops on ctrl-c, and forgets everything: the PDS, the directory and the
// host start empty each time.
//
// The console is the host's OAuth client, so the PDS fetches its metadata
// from an https name, and the browser trusts that name's certificate:
// portless gives both (https://github.com/vercel-labs/portless), its proxy
// on port 443 for every name under localtest.me, which public DNS resolves
// to 127.0.0.1, with a local CA in the system's trust store. The PDS is
// told to trust that CA too.

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { root, start } from './world.mjs';

const NAME = 'group-host';
const DOMAIN = 'localtest.me';
/// What groups are named under, by an alias of this name: its wildcard
/// sends each group's address to the host, as a deployment's does.
const GROUPS = 'groups';
const groups = `${GROUPS}.${DOMAIN}`;
const portless = join(root, 'harness/node_modules/.bin/portless');
const state = process.env.PORTLESS_STATE_DIR ?? join(homedir(), '.portless');

// portless's proxy, on 443 for both TLDs: started once a boot, with sudo.
const port = (() => {
  try {
    return readFileSync(join(state, 'proxy.port'), 'utf8').trim();
  } catch {
    return '';
  }
})();
if (port !== '443') {
  console.error(`portless's https proxy is not on port 443. Start it, once a boot (it asks for your password):

  cd harness && PORTLESS_TLD=localhost,${DOMAIN} PORTLESS_WILDCARD=1 PORTLESS_SYNC_HOSTS=0 npx portless proxy start -p 443

and \`npx portless trust\` once, if your browser does not trust its certificate.`);
  process.exit(1);
}

/// The console behind portless, at https://group-host.localtest.me.
const front = async (consolePort) => {
  execFileSync(portless, ['alias', NAME, String(consolePort), '--force'], { stdio: 'ignore' });
  return `https://${NAME}.${DOMAIN}`;
};

const w = await start({
  accounts: ['alice', 'bob', 'harbor'],
  front,
  pdsEnv: { NODE_EXTRA_CA_CERTS: join(state, 'ca.pem') },
  live: true,
  handleDomain: `.${groups}`
});
execFileSync(portless, ['alias', GROUPS, new URL(w.host).port, '--force'], { stdio: 'ignore' });

/// Takes both names back from portless.
const unalias = () => {
  for (const name of [NAME, GROUPS])
    try {
      execFileSync(portless, ['alias', '--remove', name], { stdio: 'ignore' });
    } catch {}
};

// Hikers, bob's, inviting alice: an invite waiting for her when she signs
// in, as the header, its menu and the home page mark one.
const as = (account, ...args) =>
  execFileSync(join(root, 'target/debug/atgrp'), args, {
    stdio: 'ignore',
    env: {
      ...process.env,
      ATGRP_HOST: w.host,
      ATGRP_HOST_DID: w.did,
      PDS_URL: w.net.pds,
      ATGRP_AS: account.handle,
      ATGRP_PASSWORD: account.password
    }
  });
const [alice, bob] = ['alice.test', 'bob.test'].map((h) =>
  w.net.accounts.find((a) => a.handle === h)
);
try {
  as(bob, 'group', 'create', `hikers.${groups}`, '--display-name', 'Hikers');
  as(
    bob,
    'invite',
    'send',
    `hikers.${groups}`,
    alice.handle,
    '--message',
    'Walk with us on Sundays.'
  );
} catch (e) {
  unalias();
  w.stop();
  throw e;
}

const stop = () => {
  try {
    unalias();
  } finally {
    w.stop();
    process.exit(0);
  }
};
process.on('SIGINT', stop);
process.on('SIGTERM', stop);

const people = w.net.accounts.map((a) => `  ${a.handle.padEnd(14)} ${a.password}`).join('\n');
console.log(`
The console   ${w.base}
The host      ${w.host}  (${w.did})
The PDS       ${w.net.pds}
The directory ${w.net.plc}

Sign in as any of them, with their password at the PDS's own page:
${people}

alice runs Riders (riders.${groups}). bob runs Hikers (hikers.${groups}),
which has invited alice: her invite waits on the home page. harbor is an account of its own, to
move here from ${w.base}/move; the code its server emails is at
  ${w.net.mail}?to=harbor@test.com

A group's address sends a browser to its page here: https://riders.${groups}

The command, as alice:
  ATGRP_HOST=${w.host} PDS_URL=${w.net.pds} ATGRP_AS=alice.test ATGRP_PASSWORD=password just atgrp group show riders.${groups}

ctrl-c stops it all.`);
