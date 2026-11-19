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
  // Asked for the alpha (as CI asks), the flows that write in a space run
  // on it, never skipped for a build that turns out to speak none.
  if (process.env.GROUP_HOST_DEV_ENV && !net.spaces) {
    w.stop();
    throw new Error(`GROUP_HOST_DEV_ENV names no space alpha: ${process.env.GROUP_HOST_DEV_ENV}`);
  }
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
    // Whether the PDS speaks the space alpha (GROUP_HOST_DEV_ENV): a
    // member's own write in a space needs it.
    spaces: net.spaces,
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
