// Night shift with an app that has written as it, for the guide to
// signing in as a group: bob's app password "Phone", made from the
// console as the guide's reader makes it, an app signed in to Night shift
// with it, and two posts and one taken down; so settings' Apps tab has an
// app signed in, its password, and what it did as the group.
//
// The guide makes "Phone" in a picture before this runs, and a password
// is shown only once: so this revokes that one, if it is there, and makes
// "Phone" again, taking the password the dialog shows. The tab then reads
// as the guide does: one password, "Phone", by bob, and the app named for it.

export default async ({ world, page }) => {
  const g = 'nightshift.groups.test';
  const { did } = await fetch(
    `${world.host}/xrpc/com.atproto.identity.resolveHandle?handle=${g}`
  ).then((r) => r.json());
  const p = await page('bob');
  await p.goto(`${world.base}/g/${g}/settings/apps`, { waitUntil: 'networkidle' });
  const made = p.getByRole('button', { name: 'Revoke “Phone”' });
  if (await made.count()) {
    await made.click();
    await p.locator(':popover-open').getByRole('button', { name: 'Revoke', exact: true }).click();
    await made.waitFor({ state: 'detached' });
  }
  await p.getByRole('button', { name: 'Create an app password' }).click();
  await p.getByRole('textbox', { name: 'Where it is used' }).fill('Phone');
  await p
    .getByRole('dialog', { name: 'Create an app password' })
    .getByRole('button', { name: 'Create', exact: true })
    .click();
  const password = (await p.locator('code.password').textContent()).trim();
  await p.context().close();
  // The app, signed in with it as the reader's would be, writing as Night shift.
  const post = (method, body, jwt) =>
    fetch(`${world.host}/xrpc/${method}`, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        ...(jwt ? { authorization: `Bearer ${jwt}` } : {})
      },
      body: JSON.stringify(body)
    }).then(async (r) => {
      if (!r.ok) throw new Error(`${method}: ${await r.text()}`);
      return r.json();
    });
  const { accessJwt } = await post('com.atproto.server.createSession', { identifier: g, password });
  const write = (text) =>
    post(
      'com.atproto.repo.createRecord',
      {
        repo: did,
        collection: 'app.bsky.feed.post',
        record: { $type: 'app.bsky.feed.post', text, createdAt: new Date().toISOString() }
      },
      accessJwt
    );
  await write('Doors open at 10pm tonight. Bring a thermos.');
  const gone = await write('Doors open at 11pm tonight.');
  await post(
    'com.atproto.repo.deleteRecord',
    { repo: did, collection: 'app.bsky.feed.post', rkey: gone.uri.split('/').pop() },
    accessJwt
  );
};
