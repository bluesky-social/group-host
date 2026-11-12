// Before the flows: a world (../world.mjs) whose console is behind the
// https front the PDS sends sign-ins back to (harness/tls-proxy.mjs), at
// the name harness/pds.mjs resolves for itself; and Hikers' and Riders'
// sites, which flows start, behind fronts of their own at names beside
// the console's.
// Where everything is goes to the flows in E2E_WORLD. Returns the
// teardown.

import { siteFront, start, tlsFront as front } from '../world.mjs';

export default async function setup() {
  // harbor is an account of its own, which alice moves here to become a group;
  // carol, dan, erin and frank fill a group past a page of its members.
  const w = await start({
    accounts: ['alice', 'bob', 'harbor', 'carol', 'dan', 'erin', 'frank'],
    front
  });
  const { net } = w;
  const hikers = await siteFront(
    net,
    new URL(w.host).port,
    'hikers.group-host.internal',
    'hikers.groups.test'
  );
  const riders = await siteFront(
    net,
    new URL(w.host).port,
    'riders.group-host.internal',
    'riders.groups.test'
  );
  process.env.E2E_WORLD = JSON.stringify({
    base: w.base,
    host: w.host,
    did: w.did,
    pds: net.pds,
    mail: net.mail,
    accounts: net.accounts,
    riders: w.riders,
    hikersSite: hikers.url,
    ridersSite: riders.url
  });
  return () => {
    hikers.proxy.kill('SIGTERM');
    riders.proxy.kill('SIGTERM');
    w.stop();
  };
}
