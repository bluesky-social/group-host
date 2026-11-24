// A group in full: Lantern Hikers (lanterns.groups.test), alice's, with a
// banner and a picture, words, two rules
// and a moderator role; bob, carol (its moderator), dan, erin and frank in
// it, more than a page of members; harbor asking to join with a note; and an invite waiting
// for maximilianaquinces.

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
  // Its pictures, through its settings, as only the console uploads them.
  const p = await page('alice');
  for (const [slot, png] of [
    ['Avatar', picture(256, 256)],
    ['Banner', picture(1500, 500, [47, 122, 75], [246, 241, 234])]
  ]) {
    await p.goto(`${world.base}/g/${g}/settings`, { waitUntil: 'networkidle' });
    await p
      .getByRole('button', { name: new RegExp(`^(Add|Change) the ${slot.toLowerCase()}$`) })
      .click();
    await p
      .getByLabel(slot, { exact: true })
      .setInputFiles({ name: 'picture.png', mimeType: 'image/png', buffer: png });
    await p
      .getByRole('dialog')
      .filter({ visible: true })
      .getByRole('button', { name: new RegExp(`the ${slot.toLowerCase()}$`) })
      .click();
    await p.waitForLoadState('networkidle');
  }
  await p.context().close();
};
