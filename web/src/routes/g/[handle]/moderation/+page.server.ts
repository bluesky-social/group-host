import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getActor from '#lib/lexicons/org/atgrp/internal/getActor.ts';
import type * as getGroup from '#lib/lexicons/org/atgrp/internal/getGroup.ts';
import type * as listSpaceRecords from '#lib/lexicons/org/atgrp/internal/listSpaceRecords.ts';
import type * as querySubjectHistory from '#lib/lexicons/org/atgrp/mod/querySubjectHistory.ts';
import type * as querySubjects from '#lib/lexicons/org/atgrp/mod/querySubjects.ts';
import type { Actions, PageServerLoad } from './$types';
import { fail, redirect } from '@sveltejs/kit';
import { applyAsGroup, asGroup } from '#lib/asGroup.ts';
import { act, xrpc } from '#lib/data.ts';
import {
  ESCALATED,
  besideOf,
  statusOf,
  tagsOf,
  type Event as Happened,
  type Reviewed,
  type Subject
} from '#lib/moderation.ts';
import { REASONS } from '#lib/reasons.ts';
import { m } from '#lib/paraglide/messages.js';

type Handle = `${string}.${string}`;
type Did = `did:${string}:${string}`;
type Event = { fetch: typeof fetch; params: { handle?: string } };
/** The queue's tabs: the open subjects, those of them escalated, and the resolved. */
type Shown = 'open' | 'escalated' | 'resolved';

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
  const { events, record } = await xrpc<typeof querySubjectHistory.main>(
    event,
    'org.atgrp.mod.querySubjectHistory',
    { group, subject: key, sort: 'oldest' }
  );
  return { events: events as Happened[], record: record as Reviewed | undefined };
}

/**
 * A group's queue as one of its tabs lists it, for one who may read it:
 * the subjects of a status (`?status=`, open unless asked), or the open
 * ones escalated (`?status=open&tags=escalated`), each with its history;
 * and, with `review`, the account asked for, to review whether or not it
 * is in the list.
 */
export const load: PageServerLoad = async (event) => {
  const group = await groupOf(event);
  const { group: view } = await xrpc<typeof getGroup.main>(event, 'org.atgrp.internal.getGroup', {
    group
  });
  const actions = view.viewer?.actions ?? [];
  const params = event.url.searchParams;
  const status = params.get('status') === 'resolved' ? 'resolved' : 'open';
  const shown: Shown = status === 'open' && params.get('tags') === ESCALATED ? 'escalated' : status;
  if (!actions.includes('mod.read')) return { group: view, actions, shown, subjects: null };
  const { subjects } = await xrpc<typeof querySubjects.main>(event, 'org.atgrp.mod.querySubjects', {
    group,
    status,
    ...(shown === 'escalated' ? { tags: [ESCALATED] } : {})
  });
  const listed: Subject[] = await Promise.all(
    subjects.map(async (s) => {
      const key = s.subject as `${string}:${string}`;
      return {
        key,
        actor: s.actor ?? { did: key },
        status: s.status,
        reportCount: s.reportCount ?? 0,
        tags: s.tags ?? [],
        ...(await history(event, group, key))
      };
    })
  );
  // Someone or something looked up: the row it has, or who it is and what
  // the queue holds of it, on whichever tab it is. An account by its DID;
  // a record by its URI, its author the account in it.
  const sought = params.get('review');
  let found: Subject | null = null;
  const known = sought?.startsWith('did:') || sought?.startsWith('at://');
  const review = known ? (sought as `${string}:${string}`) : null;
  if (review && !listed.some((s) => s.key === review)) {
    const did = authorOf(review);
    const actor = await xrpc<typeof getActor.main>(
      event,
      'org.atgrp.internal.getActor',
      { actor: did },
      true
    );
    const { events, record } = await history(event, group, review);
    const reports = events.filter((e) => e.event.$type?.endsWith('#reportEvent')).length;
    found = {
      key: review,
      actor: actor ?? { did },
      status: statusOf(events),
      reportCount: reports,
      tags: tagsOf(events),
      events,
      record
    };
  }
  // The rules a report may cite, by their record key, and by their URI.
  const at = `at://${group}/space/org.atgrp.members/self/${group}/org.atgrp.rule`;
  const rules = Object.fromEntries((view.rules ?? []).map((r) => [r.rkey, r.title]));
  const ruleList = (view.rules ?? []).map((r) => ({ uri: `${at}/${r.rkey}`, title: r.title }));
  return {
    group: view,
    actions,
    shown,
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
  /**
   * A moderator's decision on a subject, with their note if they wrote
   * one: resolving or reopening it (`status`), or escalating it or no
   * longer (`escalate`, `add` or `remove`, the tag).
   */
  decide: async (event) => {
    const group = await groupOf(event);
    const form = await event.request.formData();
    const note = String(form.get('note') ?? '').trim();
    const status = String(form.get('status') ?? '');
    const escalate = String(form.get('escalate') ?? '');
    return act(event, 'org.atgrp.mod.updateSubject', {
      group,
      subject: String(form.get('subject')),
      ...(status === 'open' || status === 'resolved' ? { status } : {}),
      ...(escalate === 'add' ? { addTags: [ESCALATED] } : {}),
      ...(escalate === 'remove' ? { removeTags: [ESCALATED] } : {}),
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
      'org.atgrp.internal.listSpaceRecords',
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
    return act(event, 'org.atgrp.mod.submitReport', {
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
