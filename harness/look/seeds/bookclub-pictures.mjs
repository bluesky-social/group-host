// Book club's avatar and banner, put in through its settings as alice
// would, for the guide to starting a group (docs/guides): Book club is
// started earlier in that guide, by its form.
export default async ({ world, page, picture }) => {
  const g = 'bookclub.groups.test';
  const p = await page('alice');
  for (const [slot, png] of [
    ['Avatar', picture(256, 256, [43, 88, 184], [246, 241, 234])],
    ['Banner', picture(1500, 500, [120, 60, 140], [246, 241, 234])]
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
    // Done once its row offers to remove it: the upload and the write
    // behind it can outlast the page's own load on a slow machine.
    await p
      .getByRole('button', { name: `Remove the ${slot.toLowerCase()}` })
      .waitFor({ timeout: 30_000 });
  }
  await p.context().close();
};
