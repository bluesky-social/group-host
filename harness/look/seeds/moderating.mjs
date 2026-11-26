// Garden share (gardens.groups.test), dan's, with something to moderate:
// frank (its moderator) and maximilianaquinces in it, a swaps board its
// members read and write, a post of frank's there with a picture,
// maximilianaquinces reporting that post, and frank reporting
// maximilianaquinces's account, as members report, by
// org.atgrp.mod.submitReport, which no command or page of the console
// sends.

import { serviceAuth } from '../../world.mjs';

export default async ({ world, atgrp, ask, page, who, exists, picture }) => {
  const g = 'gardens.groups.test';
  if (!(await exists(g)))
    atgrp(
      'dan',
      'group',
      'create',
      g,
      '--display-name',
      'Garden share',
      '--description',
      'Seeds, cuttings and spare tools, swapped between neighbours.',
      '--join-policy',
      'approval'
    );
  const { did } = await fetch(
    `${world.host}/xrpc/com.atproto.identity.resolveHandle?handle=${g}`
  ).then((r) => r.json());
  // A moderator role, which reads the queue, resolves and labels: made
  // in the console, as only there are those who run the group given it.
  const p = await page('dan');
  await p.goto(`${world.base}/g/${g}/settings/roles`, { waitUntil: 'networkidle' });
  await p.getByRole('button', { name: 'Create a role' }).click();
  const dialog = p.getByRole('dialog', { name: 'Create a role' });
  await dialog.getByRole('textbox', { name: 'Its ID' }).fill('moderator');
  await dialog.getByRole('textbox', { name: 'Its name' }).fill('Moderator');
  for (const action of ['mod.read', 'mod.resolve', 'label'])
    await dialog.getByRole('checkbox', { name: action, exact: true }).check();
  await dialog.getByRole('button', { name: 'Create', exact: true }).click();
  await p.locator('.roles > li strong', { hasText: 'Moderator' }).waitFor();
  await p.context().close();
  for (const name of ['frank', 'maximilianaquinces']) {
    await ask(name, g);
    atgrp(
      'dan',
      'member',
      'assign',
      g,
      who[name].handle,
      name === 'frank' ? 'member,moderator' : 'member'
    );
  }
  // A board its members read and write, and a post on it to report.
  const policy = { grants: [{ role: 'member', read: true, write: true }] };
  atgrp(
    'dan',
    'space',
    'create',
    g,
    'com.example.board',
    '--policy',
    JSON.stringify(policy),
    '--skey',
    'swaps'
  );
  // Frank's post, written in his own repo at his own PDS, as an app
  // would, with a picture of what he offers.
  const session = await fetch(`${world.pds}/xrpc/com.atproto.server.createSession`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ identifier: who.frank.handle, password: who.frank.password })
  }).then((r) => r.json());
  const asFrank = async (lxm, body, type = 'application/json') => {
    const res = await fetch(`${world.pds}/xrpc/${lxm}`, {
      method: 'POST',
      headers: { 'content-type': type, authorization: `Bearer ${session.accessJwt}` },
      body
    });
    if (!res.ok) throw new Error(`frank ${lxm}: ${await res.text()}`);
    return res.json();
  };
  const { blob } = await asFrank(
    'com.atproto.repo.uploadBlob',
    picture(640, 400, [62, 122, 64], [233, 226, 196]),
    'image/png'
  );
  const written = await asFrank(
    'com.atproto.space.putRecord',
    JSON.stringify({
      repo: who.frank.did,
      space: `at:space/${did}/com.example.board/swaps`,
      collection: 'com.example.post',
      rkey: '3mzjdd6rnfise',
      record: {
        $type: 'com.example.post',
        text: 'Six tomato seedlings for a spare trowel. Not you, Max. You never give anything back, you clown.',
        image: blob,
        createdAt: '2026-11-20T09:30:00.000Z'
      }
    })
  );
  const post = { subject: written.uri, cid: written.cid };
  const report = async (name, subject, reasonType, reason) => {
    const lxm = 'org.atgrp.mod.submitReport';
    const token = await serviceAuth(world.pds, who[name], world.did, lxm);
    const res = await fetch(`${world.host}/xrpc/${lxm}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization: `Bearer ${token}` },
      body: JSON.stringify({ group: did, ...subject, reasonType, reason })
    });
    if (!res.ok) throw new Error(`${name} reporting: ${await res.text()}`);
  };
  await report(
    'maximilianaquinces',
    post,
    'org.atgrp.mod.defs#reasonRude',
    'Calls people names under every swap.'
  );
  await report(
    'frank',
    { subject: who.maximilianaquinces.did },
    'org.atgrp.mod.defs#reasonSpam',
    'Posts the same seed shop link in every thread.'
  );
};
