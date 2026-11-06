// Lantern Hikers' moderation queue, after --seed full: dan reported twice,
// once citing a rule; a post of frank's in its talk space reported; erin
// reported and escalated; bob reported and resolved, with a note.

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
  const { group } = await call('alice', 'fyi.opensocial.getGroup', null, `group=${did}`);
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
    '--access',
    JSON.stringify({ grants: [{ role: 'member', scopes: ['space:*?action=read'] }] })
  );
  const reason = (r) => `com.atproto.moderation.defs#${r}`;
  const report = (name, subject, why, words, cites) =>
    call(name, 'fyi.opensocial.createReport', {
      group: did,
      subject,
      reasonType: reason(why),
      ...(words ? { reason: words } : {}),
      ...(cites ? { rule: cites } : {})
    });
  const account = (name) => ({ $type: 'fyi.opensocial.defs#accountSubject', did: who[name].did });
  await report('bob', account('dan'), 'reasonSpam', 'Posts the same shop link in every thread.');
  await report(
    'erin',
    account('dan'),
    'reasonRude',
    'Told a new member to stay home if they cannot keep up.',
    rule
      ? `at://${did}/space/fyi.opensocial.meta/self/${did}/fyi.opensocial.rule/${rule.rkey}`
      : null
  );
  await report(
    'carol',
    {
      $type: 'fyi.opensocial.defs#recordSubject',
      uri: `${space}/${who.frank.did}/com.example.forum.post/3mzoijrblxc2j`
    },
    'reasonMisleading',
    'Says the north ridge trail is open. It has been closed since the rockfall in September.'
  );
  await report('dan', account('erin'), 'reasonOther');
  await report('frank', account('bob'), 'reasonSpam', 'Sells headlamps.');
  const resolve = (name, key, status, note) =>
    call(name, 'fyi.opensocial.resolveSubject', {
      group: did,
      subject: account(key),
      status,
      ...(note ? { note } : {})
    });
  await resolve('carol', 'erin', 'escalated', 'Not sure what this is about. Alice, can you look?');
  await resolve('carol', 'bob', 'resolved', 'He organises the group order. Not spam.');
};
