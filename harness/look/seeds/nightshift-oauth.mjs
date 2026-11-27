// Night shift with apps signed in with OAuth, for the guide to signing in
// as a group: bob signs Darkroom in as Night shift from Darkroom's own
// site, approving it in the console, and Darkroom writes its profile for
// the group, as Grain does; then he connects Darkroom's other client,
// which only reads the group's profile, members and requests to join;
// then, in the console, he makes Night shift members-only. So settings'
// Apps tab has two apps by one name, one with its icon and one without,
// told apart by what each may do, and what was done as the group by an
// app and in the console. Night shift is started, if the guide has not
// started it already, as bob starts it there.

export default async ({ world, page, atgrp, exists }) => {
  const g = 'nightshift.groups.test';
  if (!(await exists(g))) atgrp('bob', 'group', 'create', g, '--display-name', 'Night shift');
  const p = await page('bob');
  for (const app of ['darkroom', 'darkroom-groups']) {
    await p.goto(`${world.apps}/${app}/sign-in?hint=${g}`, { waitUntil: 'networkidle' });
    await p.getByRole('button', { name: 'Sign it in as Night shift' }).click();
    await p.getByText('Darkroom is signed in.').waitFor();
  }
  await p.goto(`${world.base}/g/${g}/settings`, { waitUntil: 'networkidle' });
  await p.getByRole('radio', { name: 'Members only. Others see just its handle.' }).check();
  await p.getByRole('button', { name: 'Save who sees it' }).click();
  await p.getByText('Saved', { exact: true }).waitFor();
  await p.context().close();
};
