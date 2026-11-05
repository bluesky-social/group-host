// Riders with an app that has written as it: alice makes the app password
// "Phone" for Riders, an app signs in to Riders with it, and posts twice
// and takes one post down; so settings' Apps tab has an app signed in, its
// password, and what it did as the group.

export default async ({ world, page }) => {
  const g = 'riders.groups.test';
  const { did } = await fetch(
    `${world.host}/xrpc/com.atproto.identity.resolveHandle?handle=${g}`
  ).then((r) => r.json());
  // The password, made from the console as alice, as only she may.
  const p = await page('alice');
  await p.goto(`${world.base}/g/${g}/settings/apps`, { waitUntil: 'networkidle' });
  await p.getByRole('button', { name: 'Create an app password' }).click();
  await p.getByRole('textbox', { name: 'Where it is used' }).fill('Phone');
  await p
    .getByRole('dialog', { name: 'Create an app password' })
    .getByRole('button', { name: 'Create', exact: true })
    .click();
  const password = (await p.locator('code.password').textContent()).trim();
  await p.context().close();
  // The app, signed in with it, writing as Riders.
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
  await write('Ride on Saturday, 9am from the bridge.');
  const gone = await write('Ride on Sunday too?');
  await post(
    'com.atproto.repo.deleteRecord',
    { repo: did, collection: 'app.bsky.feed.post', rkey: gone.uri.split('/').pop() },
    accessJwt
  );
};
