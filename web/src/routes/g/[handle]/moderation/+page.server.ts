import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getActor from '#lib/lexicons/fyi/opensocial/getActor.ts';
import type * as getGroup from '#lib/lexicons/fyi/opensocial/getGroup.ts';
import type * as getModSubjectHistory from '#lib/lexicons/fyi/opensocial/getModSubjectHistory.ts';
import type * as listModSubjects from '#lib/lexicons/fyi/opensocial/listModSubjects.ts';
import type * as listSpaceRecords from '#lib/lexicons/fyi/opensocial/listSpaceRecords.ts';
import type { Actions, PageServerLoad } from './$types';
import { fail, redirect } from '@sveltejs/kit';
import { applyAsGroup, asGroup } from '#lib/asGroup.ts';
import { act, xrpc } from '#lib/data.ts';
import { besideOf, type Event as Happened, type Reviewed, type Subject } from '#lib/moderation.ts';
import { m } from '#lib/paraglide/messages.js';

type Handle = `${string}.${string}`;
type Did = `did:${string}:${string}`;
type Event = { fetch: typeof fetch; params: { handle?: string } };
type Status = 'open' | 'escalated' | 'resolved';
const STATUSES: string[] = ['open', 'escalated', 'resolved'];
/** The reasons a moderator's report gives: a new group's labeler's, but an appeal, only its subject's. */
const REASONS = ['spam', 'misleading', 'rude', 'nsfw', 'violation', 'other'];

async function groupOf(event: Event) {
  const handle = (event.params.handle ?? '') as Handle;
  return (
    await xrpc<typeof resolveHandle.main>(event, 'com.atproto.identity.resolveHandle', { handle })
  ).did;
}

/**
 * What happened to `key`, an account's DID or a record's URI, in `group`'s
 * queue, oldest first; and a record itself, as a moderator sees it, taken
 * down or not.
 */
async function history(event: Event, group: Did, key: `${string}:${string}`) {
  const { events, record } = await xrpc<typeof getModSubjectHistory.main>(
    event,
    'fyi.opensocial.getModSubjectHistory',
    { group, subject: key }
  );
  return { events: events as Happened[], record: record as Reviewed | undefined };
}

/**
 * A group's queue of one status, open unless asked, each subject with its
 * history, for one who may read it; and, with `review`, the account asked
 * for, to review whether or not it is in the list.
 */
export const load: PageServerLoad = async (event) => {
  const group = await groupOf(event);
  const { group: view } = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', {
    group
  });
  const actions = view.viewer?.actions ?? [];
  const asked = event.url.searchParams.get('status') ?? '';
  const status = (STATUSES.includes(asked) ? asked : 'open') as Status;
  if (!actions.includes('mod.read')) return { group: view, actions, status, subjects: null };
  const { subjects } = await xrpc<typeof listModSubjects.main>(
    event,
    'fyi.opensocial.listModSubjects',
    { group, status }
  );
  const listed: Subject[] = await Promise.all(
    subjects.map(async (s) => {
      const key = s.subject as `${string}:${string}`;
      return {
        key,
        actor: s.actor ?? { did: key },
        status: s.status,
        reportCount: s.reportCount,
        ...(await history(event, group, key))
      };
    })
  );
  // Someone or something looked up: the row it has, or who it is and what
  // the queue holds of it, on whichever status it is. An account by its
  // DID; a record by its URI, its author the account in it.
  const sought = event.url.searchParams.get('review');
  let found: Subject | null = null;
  const known = sought?.startsWith('did:') || sought?.startsWith('at://');
  const review = known ? (sought as `${string}:${string}`) : null;
  if (review && !listed.some((s) => s.key === review)) {
    const did = authorOf(review);
    const actor = await xrpc<typeof getActor.main>(
      event,
      'fyi.opensocial.getActor',
      { actor: did },
      true
    );
    const { events, record } = await history(event, group, review);
    const reports = events.filter((e) => e.event.$type?.endsWith('#reportEvent')).length;
    found = {
      key: review,
      actor: actor ?? { did },
      status: events.length ? 'in the queue' : undefined,
      reportCount: reports,
      events,
      record
    };
  }
  // The rules a report may cite, by their record key, and by their URI.
  const meta = `at://${group}/space/fyi.opensocial.meta/self/${group}/fyi.opensocial.rule`;
  const rules = Object.fromEntries((view.rules ?? []).map((r) => [r.rkey, r.title]));
  const ruleList = (view.rules ?? []).map((r) => ({ uri: `${meta}/${r.rkey}`, title: r.title }));
  return {
    group: view,
    actions,
    status,
    subjects: listed,
    found,
    reviewing: review,
    rules,
    ruleList,
    reasons: REASONS
  };
};

/**
 * Whose `key` is: an account itself; a record in a space, the repo it is
 * in (`at://<group>/space/<type>/<skey>/<repo>/…`); else a public record's
 * authority.
 */
function authorOf(key: string): Did {
  if (key.startsWith('did:')) return key as Did;
  const parts = key.slice('at://'.length).split('/');
  return (parts[1] === 'space' ? parts[4] : parts[0]) as Did;
}

const LABEL = 'com.atproto.label.record';

/** What a form names a label's subject by: an account's DID, or a record's URI and version. */
const subjectOf = (form: FormData) =>
  form.has('uri')
    ? { uri: String(form.get('uri')), cid: String(form.get('cid')) }
    : { uri: String(form.get('did')) };

export const actions: Actions = {
  resolve: async (event) => {
    const group = await groupOf(event);
    const form = await event.request.formData();
    const subject = String(form.get('subject'));
    const note = String(form.get('note') ?? '').trim();
    return act(event, 'fyi.opensocial.resolveModSubject', {
      group,
      subject,
      status: String(form.get('status')),
      ...(note ? { note } : {})
    });
  },
  label: async (event) => {
    const group = await groupOf(event);
    const form = await event.request.formData();
    const val = String(form.get('val') ?? '').trim();
    if (!val) return fail(400, { failed: m.route_group_moderation_label_needed() });
    const subject = subjectOf(form);
    return asGroup(event, group, besideOf(group, subject.uri), 'com.atproto.space.createRecord', {
      collection: LABEL,
      record: { $type: LABEL, ...subject, val, cts: new Date().toISOString() }
    });
  },
  unlabel: async (event) => {
    const group = await groupOf(event);
    const form = await event.request.formData();
    const val = String(form.get('val') ?? '');
    const { uri } = subjectOf(form);
    const space = besideOf(group, uri);
    // A label is taken back by deleting the group's records of it on the subject.
    const { records } = await xrpc<typeof listSpaceRecords.main>(
      event,
      'fyi.opensocial.listSpaceRecords',
      { group, space, collection: LABEL, field: 'uri', equals: uri, limit: 100 }
    );
    const rkeys = records
      .filter((r) => r.repo === group && (r.value as { val?: string }).val === val)
      .map((r) => r.uri.split('/').pop() ?? '');
    if (!rkeys.length) return fail(400, { failed: m.route_group_moderation_label_gone({ val }) });
    const writes = rkeys.map((rkey) => ({ action: 'delete' as const, collection: LABEL, rkey }));
    return applyAsGroup(event, group, space, writes);
  },
  report: async (event) => {
    const group = await groupOf(event);
    const form = await event.request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const rule = String(form.get('rule') ?? '');
    return act(event, 'fyi.opensocial.createReport', {
      group,
      subject: String(form.get('did')),
      reasonType: String(form.get('reasonType')),
      ...(reason ? { reason } : {}),
      ...(rule ? { rule } : {})
    });
  },
  find: async (event) => {
    const form = await event.request.formData();
    const handle = String(form.get('handle') ?? '')
      .trim()
      .replace(/^@/, '')
      .toLowerCase();
    const found = await event.fetch(
      `/xrpc/com.atproto.identity.resolveHandle?handle=${encodeURIComponent(handle)}`
    );
    if (!found.ok)
      return fail(400, { failed: m.route_group_moderation_find_not_found({ handle }) });
    const { did } = await found.json();
    redirect(303, `?review=${encodeURIComponent(did)}`);
  }
};
