// A group in full: Lantern Hikers (lanterns.groups.test), alice's, with a
// picture, words, two rules and a moderator role; bob, carol (its
// moderator), dan, erin and frank in it, more than a page of members;
// harbor asking to join with a note; and an invite waiting for
// maximilianaquinces.

export default async ({ world, atgrp, ask, page, picture, who, exists }) => {
  const g = 'lanterns.groups.test';
  if (!(await exists(g))) atgrp('alice', 'group', 'seed', g);
  const about =
    'Evening walks on the ridge trails, lanterns lit, every Thursday from April to October.';
  atgrp('alice', 'profile', 'set', g, '--display-name', 'Lantern Hikers', '--description', about);
  for (const name of ['bob', 'carol', 'dan', 'erin', 'frank']) {
    await ask(name, g);
    atgrp(
      'alice',
      'member',
      'assign',
      g,
      who[name].handle,
      name === 'carol' ? 'member,moderator' : 'member'
    );
  }
  await ask('harbor', g, 'I walk the north ridge most weeks and would love company.');
  atgrp(
    'alice',
    'invite',
    'send',
    g,
    who.maximilianaquinces.handle,
    '--message',
    'Come walk with us.'
  );
  // Its picture, through its settings, as only the console uploads it.
  const p = await page('alice');
  await p.goto(`${world.base}/g/${g}/settings`, { waitUntil: 'networkidle' });
  await p.getByRole('button', { name: /^(Add|Change) the avatar$/ }).click();
  await p
    .getByLabel('Avatar', { exact: true })
    .setInputFiles({ name: 'picture.png', mimeType: 'image/png', buffer: picture(256, 256) });
  await p
    .getByRole('dialog')
    .filter({ visible: true })
    .getByRole('button', { name: /the avatar$/ })
    .click();
  await p.waitForLoadState('networkidle');
  await p.context().close();
};
