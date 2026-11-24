// Garden share (gardens.groups.test), dan's, with something to moderate:
// frank (its moderator) and maximilianaquinces in it, a swaps board its
// members read, maximilianaquinces reporting a post on the board, and
// frank reporting maximilianaquinces's account, as members report, by
// fyi.opensocial.createReport, which no command or page of the console
// sends.

import { serviceAuth } from '../../world.mjs';

export default async ({ world, opensocial, ask, page, who, exists }) => {
  const g = 'gardens.groups.test';
  if (!(await exists(g)))
    opensocial(
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
  await dialog.getByRole('textbox', { name: 'Its key' }).fill('moderator');
  await dialog.getByRole('textbox', { name: 'Its name' }).fill('Moderator');
  for (const action of ['mod.read', 'mod.resolve', 'label'])
    await dialog.getByRole('checkbox', { name: action, exact: true }).check();
  await dialog.getByRole('button', { name: 'Create', exact: true }).click();
  await p.getByRole('button', { name: 'Delete Moderator' }).waitFor();
  await p.context().close();
  for (const name of ['frank', 'maximilianaquinces']) {
    await ask(name, g);
    opensocial(
      'dan',
      'member',
      'assign',
      g,
      who[name].handle,
      name === 'frank' ? 'member,moderator' : 'member'
    );
  }
  // A board its members read, and a post on it to report.
  const policy = { grants: [{ role: 'member', read: true }] };
  opensocial(
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
  const post = `at://${did}/space/com.example.board/swaps/${who.frank.did}/com.example.post/3mzjdd6rnfise`;
  const report = async (name, subject, reasonType, reason) => {
    const lxm = 'fyi.opensocial.createReport';
    const token = await serviceAuth(world.pds, who[name], world.did, lxm);
    const res = await fetch(`${world.host}/xrpc/${lxm}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization: `Bearer ${token}` },
      body: JSON.stringify({ group: did, subject, reasonType, reason })
    });
    if (!res.ok) throw new Error(`${name} reporting: ${await res.text()}`);
  };
  await report('maximilianaquinces', post, 'rude', 'Calls people names under every swap.');
  await report(
    'frank',
    who.maximilianaquinces.did,
    'spam',
    'Posts the same seed shop link in every thread.'
  );
};
