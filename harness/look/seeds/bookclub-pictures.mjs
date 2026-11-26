// Book club's avatar, put in through its settings as alice would, for the
// guide to starting a group (docs/guides): Book club is started earlier in
// that guide, by its form.
export default async ({ world, page, picture }) => {
  const g = 'bookclub.groups.test';
  const p = await page('alice');
  await p.goto(`${world.base}/g/${g}/settings`, { waitUntil: 'networkidle' });
  await p.getByRole('button', { name: /^(Add|Change) the avatar$/ }).click();
  await p.getByLabel('Avatar', { exact: true }).setInputFiles({
    name: 'picture.png',
    mimeType: 'image/png',
    buffer: picture(256, 256, [43, 88, 184], [246, 241, 234])
  });
  await p
    .getByRole('dialog')
    .filter({ visible: true })
    .getByRole('button', { name: /the avatar$/ })
    .click();
  // Done once its row offers to remove it: the upload and the write
  // behind it can outlast the page's own load on a slow machine.
  await p.getByRole('button', { name: 'Remove the avatar' }).waitFor({ timeout: 30_000 });
  await p.context().close();
};
