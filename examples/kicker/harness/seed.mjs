// The place the site is for, made over a world (world.mjs) as its people
// would make it: the group, its host role and its spaces, and its site,
// with the `opensocial` command as alice; then who is in it, and what they
// wrote, each at their own PDS.
//
// - Kicker, alice's skate club, joined by approval. bob hosts its
//   sessions and events; carol and dan are members; erin is asking to join.
// - Its room: notes on the board, answers to events, cards, and two
//   discussions with replies under replies. Its calendar: events, written
//   by its hosts, each with a poster uploaded to its host's own PDS
//   (posters.mjs).
// - Its moderation, the group host's: its own label, off-topic, a note
//   bob reported that alice marked so, a report of carol's waiting in the
//   queue, and bob's ad, which dan reported and alice hid with !hide.
//
// It takes the accounts alice to erin.

import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { portrait, poster } from './posters.mjs';
import { groupHost, root, serviceAuth } from './world.mjs';

const NS = 'com.example.place';
/// The host's space type for a group's site: the room and the calendar are
/// each a space of it, under their own key, so nothing needs publishing.
const SITE_SPACE = 'fyi.opensocial.siteSpace';
const DAY = 86_400_000;

/// `days` from now, at `hour` UTC on the hour.
const at = (days, hour) => {
  const d = new Date(Date.now() + days * DAY);
  d.setUTCHours(hour, 0, 0, 0);
  return d.toISOString();
};

/// The kinds of record the site's pages write, each a space, by its type
/// and key, and a collection.
export const WRITES = [
  `${SITE_SPACE}/room:${NS}.note`,
  `${SITE_SPACE}/room:${NS}.rsvp`,
  `${SITE_SPACE}/room:${NS}.card`,
  `${SITE_SPACE}/room:${NS}.topic`,
  `${SITE_SPACE}/room:${NS}.reply`,
  `${SITE_SPACE}/calendar:${NS}.event`
];

/**
 * Makes the place `handle`, as `run` (the `opensocial` command, as its
 * founder) makes it: the group, joined by approval; its host role, which
 * admins give; its room and calendar, each a space of the site's type
 * under its own key, every member reading each, any member writing in the
 * room and hosts and admins in the calendar; and its own label, which the
 * site shows folded behind a warning.
 */
export function make(run, handle) {
  run(
    'group',
    'create',
    handle,
    '--display-name',
    'Kicker',
    '--description',
    'Skate till the streetlights. Thursday to Sunday at the lot, noon to dark.',
    '--join-policy',
    'approval'
  );
  run(
    'role',
    'set',
    handle,
    'host',
    '--display-name',
    'Host',
    '--description',
    'Hosts sessions and events.'
  );
  run('role', 'grant', handle, 'admin', '--assignable', 'host');
  const read = (kind) => ({
    role: 'member',
    scopes: [`space:${SITE_SPACE}?skey=${kind}&action=read`]
  });
  for (const [kind, name, writableBy] of [
    ['room', 'Room', null],
    ['calendar', 'Calendar', ['host', 'admin']]
  ]) {
    const access = { grants: [read(kind)] };
    if (writableBy) access.writableBy = writableBy.map((role) => ({ role }));
    const args = ['--access', JSON.stringify(access), '--name', name, '--skey', kind];
    run('space', 'create', handle, SITE_SPACE, ...args);
  }
  run(
    'label',
    'define',
    handle,
    'off-topic',
    '--name',
    'Off-topic',
    '--description',
    'Not about skating, the spots, or the people who skate them.',
    '--severity',
    'inform',
    '--blurs',
    'content',
    '--default-setting',
    'warn'
  );
}

/**
 * Publishes the site as built (site/dist, `npm run build` there) as the
 * place `handle`'s, as `run` does it: every room's address answered by
 * its index.html; reading the group, its members, and its room and
 * calendar whole, reporting what members see, and writing each kind of
 * record in the visitor's own repo.
 */
export function publish(run, handle) {
  run(
    'site',
    'publish',
    handle,
    join(root, 'site/dist'),
    '--fallback',
    'index.html',
    '--reads',
    'getGroup,listMembers,getSpaceBlob',
    '--spaces',
    `${SITE_SPACE}/room,${SITE_SPACE}/calendar`,
    '--acts',
    'createReport,requestJoin,cancelJoinRequest,leaveGroup',
    '--writes',
    WRITES.join(',')
  );
}

/** Makes the place on `world`, with its site; its DID and handle. */
export async function seed(world, { handleDomain = '.groups.test' } = {}) {
  const { net, host, did: hostDid } = world;
  const who = Object.fromEntries(net.accounts.map((a) => [a.handle.split('.')[0], a]));
  const opensocial = (account, ...args) =>
    execFileSync(join(groupHost, 'target/debug/opensocial'), args, {
      encoding: 'utf8',
      env: {
        ...process.env,
        OPENSOCIAL_HOST: host,
        PDS_URL: net.pds,
        OPENSOCIAL_HOST_DID: hostDid,
        OPENSOCIAL_AS: account.handle,
        OPENSOCIAL_PASSWORD: account.password
      }
    });
  const handle = `kicker${handleDomain}`;
  const asAlice = (...args) => opensocial(who.alice, ...args);
  make(asAlice, handle);
  publish(asAlice, handle);
  const place = await fetch(`${host}/xrpc/com.atproto.identity.resolveHandle?handle=${handle}`)
    .then((r) => r.json())
    .then((r) => r.did);
  const space = (kind) => `at://${place}/space/${SITE_SPACE}/${kind}`;

  // Who is in it: bob, carol and dan let in, bob a host; erin asking.
  const ask = async (account) => {
    const lxm = 'fyi.opensocial.requestJoin';
    const res = await fetch(`${host}/xrpc/${lxm}`, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        authorization: `Bearer ${await serviceAuth(net.pds, account, hostDid, lxm)}`
      },
      body: JSON.stringify({ group: place })
    });
    if (!res.ok) throw new Error(`requestJoin ${account.handle}: ${await res.text()}`);
  };
  for (const [name, roles] of [
    ['bob', 'member,host'],
    ['carol', 'member'],
    ['dan', 'member']
  ]) {
    await ask(who[name]);
    opensocial(who.alice, 'member', 'assign', handle, who[name].did, roles);
  }
  await ask(who.erin);

  // What they wrote, each in their own repo in the place's spaces, at
  // their own PDS, as the site's pages write it.
  const signIn = (account) =>
    fetch(`${net.pds}/xrpc/com.atproto.server.createSession`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ identifier: account.handle, password: account.password })
    }).then((r) => r.json());
  const write = async (account, kind, collection, record, rkey) => {
    const session = await signIn(account);
    const nsid = rkey ? 'putRecord' : 'createRecord';
    const res = await fetch(`${net.pds}/xrpc/com.atproto.space.${nsid}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization: `Bearer ${session.accessJwt}` },
      body: JSON.stringify({
        repo: account.did,
        space: space(kind),
        collection: `${NS}.${collection}`,
        ...(rkey ? { rkey } : {}),
        record: { $type: `${NS}.${collection}`, createdAt: new Date().toISOString(), ...record }
      })
    });
    if (!res.ok) throw new Error(`${collection} by ${account.handle}: ${await res.text()}`);
    return await res.json();
  };
  // A picture uploaded to `account`'s own PDS, for a record to point to.
  const upload = async (account, type, body) => {
    const session = await signIn(account);
    const res = await fetch(`${net.pds}/xrpc/com.atproto.repo.uploadBlob`, {
      method: 'POST',
      headers: { 'content-type': type, authorization: `Bearer ${session.accessJwt}` },
      body
    });
    if (!res.ok) throw new Error(`upload by ${account.handle}: ${await res.text()}`);
    return (await res.json()).blob;
  };
  const note = (account, text, kind) => write(account, 'room', 'note', { text, kind });
  await note(
    who.alice,
    'Session at the lot Saturday from two. The curb is waxed, the cones are out.',
    'notice'
  );
  await note(
    who.bob,
    'Anyone have an 8.25 deck I could borrow? Mine snapped on the bank yesterday.',
    'ask'
  );
  await note(
    who.dan,
    "Beginners' Sunday is back: I'll be at the flat bit by the fence from ten, teaching pushing and stopping.",
    'offer'
  );
  await note(
    who.carol,
    'Found: a set of white 54s with red bearings, by the bench. Ask me.',
    'notice'
  );

  // Moderation, the group host's, as each would do it: bob reports
  // carol's sale, which alice marks off-topic and resolves; carol reports
  // dan's sharp note, which waits in the queue for a moderator.
  const reportAs = async (account, record, reason, words) => {
    const lxm = 'fyi.opensocial.createReport';
    const res = await fetch(`${host}/xrpc/${lxm}`, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        authorization: `Bearer ${await serviceAuth(net.pds, account, hostDid, lxm)}`
      },
      body: JSON.stringify({
        group: place,
        subject: { $type: 'fyi.opensocial.defs#recordSubject', uri: record.uri, cid: record.cid },
        reasonType: `com.atproto.moderation.defs#${reason}`,
        reason: words
      })
    });
    if (!res.ok) throw new Error(`report by ${account.handle}: ${await res.text()}`);
  };
  const bike = await note(
    who.carol,
    'Selling my old road bike, 54cm, barely ridden. 200 dollars, message me.',
    'offer'
  );
  await reportAs(who.bob, bike, 'reasonOther', 'A sale, not about skating.');
  opensocial(who.alice, 'mod', 'label', handle, bike.uri, 'off-topic', '--cid', bike.cid);
  opensocial(
    who.alice,
    'mod',
    'resolve',
    handle,
    bike.uri,
    '--status',
    'resolved',
    '--note',
    'Marked off-topic: it can stay, folded.'
  );
  const scissors = await note(
    who.dan,
    'Whoever keeps walking off with the good skate tool: stop it. You know who you are.',
    'notice'
  );
  await reportAs(who.carol, scissors, 'reasonRude', 'Sharp, for a note to everyone.');
  // bob's ad, which dan reports as spam and alice hides from members.
  const ink = await note(who.bob, 'Cheap bearings, any brand, link in my bio.', 'offer');
  await reportAs(who.dan, ink, 'reasonSpam', 'An ad.');
  opensocial(who.alice, 'mod', 'label', handle, ink.uri, '!hide', '--cid', ink.cid);

  // Two discussions, their replies under what each answers.
  const topic = (account, title, text) => write(account, 'room', 'topic', { title, text });
  const replyTo = (account, root, parent, text) =>
    write(account, 'room', 'reply', {
      root: root.uri,
      ...(parent ? { parent: parent.uri } : {}),
      text
    });
  const ramp = await topic(
    who.carol,
    'Should we build a mini ramp in the back lot?',
    "The flat bit behind the fence is ours on weekends. Four foot, eight wide, steel coping. Who's in, and who has tools?"
  );
  const fund = await replyTo(who.dan, ramp, null, "Yes. I'll put in twenty for plywood.");
  await replyTo(who.bob, ramp, fund, 'Same, and I still have the coping from my old ramp.');
  await replyTo(
    who.alice,
    ramp,
    null,
    "Let's settle it at Thursday's session, then frame it Sunday."
  );
  const video = await topic(
    who.dan,
    'A video night at the end of the month',
    "Bring whatever you filmed at the lot this year; we'll put it all on the big screen."
  );
  const tapes = await replyTo(who.carol, video, null, 'Yes! Can we show the old tapes too?');
  await replyTo(who.dan, video, tapes, 'Of course. I still have a player.');

  // Each with its poster, in two inks of its own.
  const event = async (account, name, description, days, hours, inks, more = {}) => {
    const made = await write(account, 'calendar', 'event', {
      name,
      description,
      startsAt: at(days, 18),
      endsAt: at(days, 18 + hours),
      image: await upload(account, 'image/svg+xml', poster(name, ...inks)),
      imageAlt: `A poster for ${name}: two inks overprinted, and its name.`,
      ...more
    });
    return made.uri;
  };
  const jam = await event(
    who.alice,
    'Curb jam',
    'Slappies on the painted curb out back. Any board, any level; first-timers go first.',
    3,
    4,
    ['orange', 'teal']
  );
  const build = await event(
    who.bob,
    'Ramp build day',
    'Framing the flatbottom for the new mini ramp. The 2x4s are cut; bring a hammer if you have one.',
    10,
    5,
    ['yellow', 'pink']
  );
  await event(
    who.bob,
    'Night session',
    'Lights on at the lot until ten.',
    6,
    4,
    ['blue', 'yellow'],
    { cancelled: true }
  );
  await event(
    who.alice,
    'Opening day',
    'The lot opens: music, snacks, and the first session.',
    -14,
    4,
    ['pink', 'blue']
  );

  // Answers, at each event's own key, so answering again replaces them.
  const rsvp = (account, uri, status) =>
    write(account, 'room', 'rsvp', { subject: uri, status }, uri.split('/').at(-1));
  await rsvp(who.carol, jam, 'going');
  await rsvp(who.bob, jam, 'going');
  await rsvp(who.dan, jam, 'interested');
  await rsvp(who.alice, build, 'going');

  // Each card, and the picture on it, uploaded to its member's own PDS.
  const card = async (account, fields, inks) => {
    const avatar =
      inks && (await upload(account, 'image/svg+xml', portrait(account.handle, ...inks)));
    return write(account, 'room', 'card', avatar ? { ...fields, avatar } : fields, 'self');
  };
  await card(who.alice, {}, ['blue', 'yellow']);
  await card(
    who.bob,
    {
      about: 'Builds ramps. Mostly mini ramps, lately.',
      offers: 'Carpentry, coping, help with any build.'
    },
    ['orange', 'teal']
  );
  await card(who.dan, { about: 'Filmer, and a curb skater.', offers: 'Filming, fixing bearings.' });
  return { place, handle };
}
