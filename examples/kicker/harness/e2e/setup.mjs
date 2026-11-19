// Before the flows: a world (../world.mjs), the place made in it
// (../seed.mjs), and its site behind an https front at a name beside the
// console's, as the PDS reaches it to read the site's OAuth client. Where
// everything is goes to the flows in E2E_WORLD. Returns the teardown.

import { siteFront, start, tlsFront } from '../world.mjs';
import { seed } from '../seed.mjs';

export default async function setup() {
  const w = await start({
    accounts: ['alice', 'bob', 'carol', 'dan', 'erin'],
    front: tlsFront
  });
  try {
    const { place, handle } = await seed(w);
    const site = await siteFront(w.net, new URL(w.host).port, 'kicker.group-host.internal', handle);
    process.env.E2E_WORLD = JSON.stringify({
      pds: w.net.pds,
      host: w.host,
      hostDid: w.did,
      console: w.base,
      place,
      accounts: w.net.accounts,
      site: site.url
    });
    return () => {
      site.proxy.kill('SIGTERM');
      w.stop();
    };
  } catch (e) {
    w.stop();
    throw e;
  }
}
