// What each room shows, worked out in the browser from the records a space
// holds (`os.records`: GroupRecord[], each `{uri, repo, collection, rkey,
// cid, value}`). Pure functions, nothing read or written here; tried by
// views.test.js (`npm test`).

/** A record's time: its `createdAt`, as its writer said; 0 if it gives none. */
export const timeOf = (r) => Date.parse(r.value?.createdAt) || 0;

/** A link field's URI, written as a URI or as a strong ref `{uri, cid}`. */
export const linkOf = (field) => (typeof field === 'string' ? field : field?.uri);

/** The records of `collection`. */
export const ofCollection = (records, collection) =>
  records.filter((r) => r.collection === collection);

/** The record `repo` holds at `rkey` in `collection`, or none. */
export const recordAt = (records, repo, collection, rkey) =>
  records.find((r) => r.repo === repo && r.collection === collection && r.rkey === rkey) ?? null;

/** `value` as written over `was` (a held record, or none): `was`'s `createdAt` kept, else `now`. */
export const stamped = (value, was, now) => ({
  ...value,
  createdAt: was?.value?.createdAt ?? now
});

/** Older first by time; equal times by record key, then writer, so ties never shuffle. */
const older = (a, b) =>
  timeOf(a) - timeOf(b) ||
  (a.rkey < b.rkey ? -1 : a.rkey > b.rkey ? 1 : 0) ||
  (a.repo < b.repo ? -1 : a.repo > b.repo ? 1 : 0);

/** `records`, oldest first. */
export const oldestFirst = (records) => [...records].sort(older);

/** `records`, newest first. */
export const newestFirst = (records) => [...records].sort((a, b) => older(b, a));

/** The labels `group` put on records, from its own label records: a Map of the URI labelled to its values. */
export function labelsOn(records, group, collection = 'com.atproto.label.record') {
  const on = new Map();
  for (const r of ofCollection(records, collection)) {
    if (r.repo !== group) continue;
    on.set(r.value.uri, [...(on.get(r.value.uri) ?? []), r.value.val]);
  }
  return on;
}

/** Each writer's card, by their DID: the record at `self`, or their newest. */
export function cardsByWriter(records, collection) {
  const by = new Map();
  for (const r of oldestFirst(ofCollection(records, collection))) {
    if (!by.has(r.repo) || by.get(r.repo).rkey !== 'self') by.set(r.repo, r);
  }
  return new Map([...by].map(([did, r]) => [did, r.value]));
}

/**
 * The answers to each event, by its URI (the answer's `subject`): who is
 * `going` and `interested`, as records, newest first, each writer once
 * (their newest answer); and `mine`, a Map of event URI to the status
 * `viewer` gave it.
 */
export function rsvps(records, collection, viewer) {
  const to = new Map();
  const mine = new Map();
  const seen = new Set();
  for (const r of newestFirst(ofCollection(records, collection))) {
    const subject = linkOf(r.value.subject);
    if (!subject || seen.has(`${r.repo} ${subject}`)) continue;
    seen.add(`${r.repo} ${subject}`);
    if (!to.has(subject)) to.set(subject, { going: [], interested: [] });
    to.get(subject)[r.value.status]?.push(r);
    if (r.repo === viewer) mine.set(subject, r.value.status);
  }
  return { to, mine };
}

/**
 * The discussion's list: its topics, the most lately active first (a
 * topic's activity the later of its own time and its newest reply's), and
 * `replies`, a Map of topic URI to `{count, latestAt}`. A reply `hides`
 * says is hidden (`hides(uri)`) is neither counted nor moves its topic.
 */
export function topicsByActivity(records, { topic, reply }, hides = () => false) {
  const replies = new Map();
  for (const r of ofCollection(records, reply)) {
    const root = linkOf(r.value.root);
    if (!root || hides(r.uri)) continue;
    const was = replies.get(root) ?? { count: 0, latestAt: null, at: 0 };
    const at = timeOf(r);
    replies.set(
      root,
      at >= was.at
        ? { count: was.count + 1, latestAt: r.value.createdAt, at }
        : { ...was, count: was.count + 1 }
    );
  }
  const active = (t) => Math.max(timeOf(t), replies.get(t.uri)?.at ?? 0);
  const topics = newestFirst(ofCollection(records, topic)).sort((a, b) => active(b) - active(a));
  return {
    topics,
    replies: new Map([...replies].map(([uri, { count, latestAt }]) => [uri, { count, latestAt }]))
  };
}

/**
 * A discussion opened: its topic (the record at `root`, or none) and every
 * reply in it, oldest first. Each reply's `parent` (else `root`) says what
 * it answers; `under` groups them so.
 */
export function thread(records, { reply }, root) {
  return {
    topic: records.find((r) => r.uri === root) ?? null,
    replies: oldestFirst(ofCollection(records, reply).filter((r) => linkOf(r.value.root) === root))
  };
}

/** Replies by what each answers (its `parent`, else its `root`), in the order given. */
export function under(replies) {
  const by = new Map();
  for (const r of replies) {
    const parent = linkOf(r.value.parent) ?? linkOf(r.value.root);
    by.set(parent, [...(by.get(parent) ?? []), r]);
  }
  return by;
}
