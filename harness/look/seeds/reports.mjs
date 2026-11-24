// Lantern Hikers' moderation queue, after --seed full: dan reported twice,
// once citing a rule; a post of frank's in its talk space reported; erin
// reported and escalated; bob reported and resolved, with a note; and two
// labels of the group's own defined.

import { serviceAuth } from '../../world.mjs';

export default async ({ world, who, opensocial }) => {
  const g = 'lanterns.groups.test';
  const call = async (name, lxm, body, query) => {
    const token = await serviceAuth(world.pds, who[name], world.did, lxm);
    const res = await fetch(`${world.host}/xrpc/${lxm}${query ? `?${query}` : ''}`, {
      method: body ? 'POST' : 'GET',
      headers: {
        authorization: `Bearer ${token}`,
        ...(body ? { 'content-type': 'application/json' } : {})
      },
      ...(body ? { body: JSON.stringify(body) } : {})
    });
    const said = await res.json();
    if (!res.ok) throw new Error(`${name} ${lxm}: ${JSON.stringify(said)}`);
    return said;
  };
  const { did } = await fetch(
    `${world.host}/xrpc/com.atproto.identity.resolveHandle?handle=${g}`
  ).then((r) => r.json());
  const { group } = await call('alice', 'fyi.opensocial.internal.getGroup', null, `group=${did}`);
  const [rule] = group.rules ?? [];
  const space = `at://${did}/space/com.example.forum/talk`;
  opensocial(
    'alice',
    'space',
    'create',
    g,
    'com.example.forum',
    '--skey',
    'talk',
    '--policy',
    JSON.stringify({ grants: [{ role: 'member', read: true }] })
  );
  // Two labels of the group's own, for moderators to choose.
  for (const [label, name, description, shown] of [
    ['off-topic', 'Off-topic', 'Not about walking, lanterns or the ridge.', 'warn'],
    ['selling', 'Selling', 'Selling something, outside the swap thread.', 'hide']
  ])
    opensocial(
      'alice',
      'label',
      'define',
      g,
      label,
      '--name',
      name,
      '--description',
      description,
      '--default-setting',
      shown
    );
  // What a report is about: its subject, and the version of a record.
  const report = (name, about, reasonType, words, cites) =>
    call(name, 'fyi.opensocial.createReport', {
      group: did,
      ...about,
      reasonType,
      ...(words ? { reason: words } : {}),
      ...(cites ? { rule: cites } : {})
    });
  const account = (name) => ({ subject: who[name].did });
  await report('bob', account('dan'), 'spam', 'Posts the same shop link in every thread.');
  await report(
    'erin',
    account('dan'),
    'rude',
    'Told a new member to stay home if they cannot keep up.',
    rule
      ? `at://${did}/space/fyi.opensocial.meta/self/${did}/fyi.opensocial.rule/${rule.rkey}`
      : null
  );
  await report(
    'carol',
    {
      subject: `${space}/${who.frank.did}/com.example.forum.post/3mzoijrblxc2j`,
      // The version reported, as an app names it.
      cid: 'bafyreie5737gdxlw5i64vzichcalba3z2v5n6icifvx5xytvske7mr3hpm'
    },
    'misleading',
    'Says the north ridge trail is open. It has been closed since the rockfall in September.'
  );
  await report('dan', account('erin'), 'other');
  await report('frank', account('bob'), 'spam', 'Sells headlamps.');
  const resolve = (name, key, status, note) =>
    call(name, 'fyi.opensocial.resolveModSubject', {
      group: did,
      ...account(key),
      status,
      ...(note ? { note } : {})
    });
  await resolve('carol', 'erin', 'escalated', 'Not sure what this is about. Alice, can you look?');
  await resolve('carol', 'bob', 'resolved', 'He organises the group order. Not spam.');
};
