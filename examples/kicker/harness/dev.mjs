// `just dev`: the site to use by hand, over a world of its own (world.mjs):
// the reference PDS with alice to erin, the group host and its console,
// and the place made in it (seed.mjs). Places are named under
// places.localtest.me, so Kicker's address, where its site is, opens in
// a browser: https://kicker.places.localtest.me. Stops on ctrl-c, and
// forgets everything.
//
// The site is an OAuth client of the PDS, which fetches its metadata from
// an https name the browser trusts: portless gives both
// (https://github.com/vercel-labs/portless), its proxy on port 443 for
// every name under localtest.me, with a local CA the PDS is told to trust.

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { root, start } from './world.mjs';
import { seed } from './seed.mjs';

const DOMAIN = 'localtest.me';
/// The console's name, and the one places are named under: its wildcard
/// sends each place's address to the host, as a deployment's does.
const [CONSOLE, PLACES] = ['places-console', 'places'];
const portless = join(root, 'harness/node_modules/.bin/portless');
const state = process.env.PORTLESS_STATE_DIR ?? join(homedir(), '.portless');
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

const alias = (name, port) =>
  execFileSync(portless, ['alias', name, String(port), '--force'], { stdio: 'ignore' });
const handleDomain = `.${PLACES}.${DOMAIN}`;
const w = await start({
  accounts: ['alice', 'bob', 'carol', 'dan', 'erin'],
  front: async (consolePort) => (alias(CONSOLE, consolePort), `https://${CONSOLE}.${DOMAIN}`),
  pdsEnv: { NODE_EXTRA_CA_CERTS: join(state, 'ca.pem') },
  handleDomain
});
alias(PLACES, new URL(w.host).port);
const stop = () => {
  try {
    for (const name of [CONSOLE, PLACES])
      execFileSync(portless, ['alias', '--remove', name], { stdio: 'ignore' });
  } finally {
    w.stop();
    process.exit(0);
  }
};
process.on('SIGINT', stop);
process.on('SIGTERM', stop);
try {
  const { handle } = await seed(w, { handleDomain });
  const people = w.net.accounts.map((a) => `  ${a.handle.padEnd(12)} ${a.password}`).join('\n');
  console.log(`
The site      https://${handle}
The console   ${w.base}  (the group host's)
The host      ${w.host}  (${w.did})
The PDS       ${w.net.pds}

Sign in on the site as any of them, at the PDS's own page:
${people}

alice runs Kicker; bob hosts its events; carol and dan are members;
erin is asking to join, which alice approves in the console.

ctrl-c stops it all.`);
} catch (e) {
  console.error(e);
  stop();
}
