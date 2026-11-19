// views.js, tried on records as `os.records` answers them: node --test.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  cardsByWriter,
  labelsOn,
  newestFirst,
  recordAt,
  rsvps,
  stamped,
  thread,
  topicsByActivity,
  under
} from './views.js';

const GROUP = 'did:plc:group';
const SPACE = `at://${GROUP}/space/com.example.place.room/self`;
let n = 0;
/** A record of `collection` by `repo`, made at minute `at`, with `value`. */
const rec = (repo, collection, at, value = {}, rkey = `k${String(n++).padStart(4, '0')}`) => ({
  uri: `${SPACE}/${repo}/${collection}/${rkey}`,
  repo,
  collection,
  rkey,
  cid: 'bafy',
  value: {
    ...value,
    createdAt: new Date(Date.UTC(2026, 0, 1, 0, at)).toISOString()
  }
});
const NOTE = 'com.example.place.note';
const TOPIC = 'com.example.place.topic';
const REPLY = 'com.example.place.reply';
const RSVP = 'com.example.place.rsvp';
const LABEL = 'com.atproto.label.label';
const C = { topic: TOPIC, reply: REPLY };

test('notes newest first, ties by record key', () => {
  const a = rec('did:a', NOTE, 1, {}, 'b');
  const b = rec('did:b', NOTE, 5);
  const c = rec('did:c', NOTE, 1, {}, 'a');
  assert.deepEqual(
    newestFirst([a, b, c]).map((r) => r.uri),
    [b.uri, a.uri, c.uri]
  );
});

test('labels: only the group’s, by the URI labelled', () => {
  const note = rec('did:a', NOTE, 1);
  const on = labelsOn(
    [
      rec(GROUP, LABEL, 2, { uri: note.uri, val: '!hide' }),
      rec(GROUP, LABEL, 3, { uri: note.uri, val: 'off-topic' }),
      rec('did:a', LABEL, 4, { uri: note.uri, val: 'fake' })
    ],
    GROUP
  );
  assert.deepEqual(on.get(note.uri), ['!hide', 'off-topic']);
});

test('cards by writer', () => {
  const cards = cardsByWriter(
    [rec('did:a', 'com.example.place.card', 1, { bio: 'a' }, 'self'), rec('did:b', NOTE, 1)],
    'com.example.place.card'
  );
  assert.equal(cards.get('did:a').bio, 'a');
  assert.equal(cards.size, 1);
});

test('rsvps counted by subject, each writer once, the viewer’s own', () => {
  const ev = `at://${GROUP}/space/com.example.place.calendar/self/did:h/com.example.place.event/e1`;
  const { to, mine } = rsvps(
    [
      rec('did:a', RSVP, 1, { subject: ev, status: 'going' }),
      rec('did:b', RSVP, 1, {
        subject: { uri: ev, cid: 'x' },
        status: 'interested'
      }),
      rec('did:c', RSVP, 1, { subject: ev, status: 'interested' }),
      rec('did:c', RSVP, 2, { subject: ev, status: 'going' })
    ],
    RSVP,
    'did:b'
  );
  assert.equal(to.get(ev).going.length, 2);
  assert.equal(to.get(ev).interested.length, 1);
  assert.equal(mine.get(ev), 'interested');
});

test('topics by latest reply, counted; a hidden reply neither counts nor moves', () => {
  const old = rec('did:a', TOPIC, 1);
  const fresh = rec('did:b', TOPIC, 10);
  const quiet = rec('did:c', TOPIC, 5);
  const r1 = rec('did:c', REPLY, 20, { root: old.uri });
  const hiddenReply = rec('did:c', REPLY, 30, { root: quiet.uri });
  const records = [old, fresh, quiet, r1, hiddenReply];
  const hides = (uri) => uri === hiddenReply.uri;
  const { topics, replies } = topicsByActivity(records, C, hides);
  assert.deepEqual(
    topics.map((t) => t.uri),
    [old.uri, fresh.uri, quiet.uri]
  );
  assert.deepEqual(replies.get(old.uri), {
    count: 1,
    latestAt: r1.value.createdAt
  });
  assert.equal(replies.get(quiet.uri), undefined);
  const all = topicsByActivity(records, C);
  assert.equal(all.topics[0].uri, quiet.uri);
  assert.equal(all.replies.get(quiet.uri).count, 1);
});

test('a thread oldest first, nested by parent', () => {
  const topic = rec('did:a', TOPIC, 1);
  const r2 = rec('did:b', REPLY, 3, { root: topic.uri });
  const r1 = rec('did:c', REPLY, 2, { root: topic.uri });
  const r3 = rec('did:a', REPLY, 4, { root: topic.uri, parent: r1.uri });
  const other = rec('did:a', REPLY, 1, { root: 'at://elsewhere' });
  const t = thread([r3, r2, other, topic, r1], C, topic.uri);
  assert.equal(t.topic.uri, topic.uri);
  assert.deepEqual(
    t.replies.map((r) => r.uri),
    [r1.uri, r2.uri, r3.uri]
  );
  const by = under(t.replies);
  assert.deepEqual(
    by.get(topic.uri).map((r) => r.uri),
    [r1.uri, r2.uri]
  );
  assert.deepEqual(
    by.get(r1.uri).map((r) => r.uri),
    [r3.uri]
  );
  assert.equal(thread([], C, topic.uri).topic, null);
});

test('a put keeps the held record’s createdAt; a new one is made now', () => {
  const card = rec('did:a', 'com.example.place.card', 3, { bio: 'old' }, 'self');
  const was = recordAt([rec('did:b', NOTE, 1), card], 'did:a', 'com.example.place.card', 'self');
  const now = '2026-11-18T00:00:00.000Z';
  assert.deepEqual(stamped({ bio: 'new' }, was, now), {
    bio: 'new',
    createdAt: card.value.createdAt
  });
  assert.equal(recordAt([card], 'did:b', 'com.example.place.card', 'self'), null);
  assert.equal(stamped({ bio: 'new' }, null, now).createdAt, now);
});
