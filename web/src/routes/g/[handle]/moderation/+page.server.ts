import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getActor from '#lib/lexicons/fyi/opensocial/getActor.ts';
import type * as getGroup from '#lib/lexicons/fyi/opensocial/getGroup.ts';
import type * as getSubjectHistory from '#lib/lexicons/fyi/opensocial/getSubjectHistory.ts';
import type * as listSubjects from '#lib/lexicons/fyi/opensocial/listSubjects.ts';
import type { Actions, PageServerLoad } from './$types';
import { fail, redirect } from '@sveltejs/kit';
import { act, xrpc } from '#lib/data.ts';
import type { Event as Happened, Subject } from '#lib/moderation.ts';

type Handle = `${string}.${string}`;
type Did = `did:${string}:${string}`;
type Event = { fetch: typeof fetch; params: { handle?: string } };
type Status = 'open' | 'escalated' | 'resolved';
const STATUSES: string[] = ['open', 'escalated', 'resolved'];
/** The reasons a report gives, as a new group's labeler takes them. */
const REASONS = ['Spam', 'Misleading', 'Rude', 'Sexual', 'Violation', 'Other'].map(
  (r) => `com.atproto.moderation.defs#reason${r}`
);

async function groupOf(event: Event) {
  const handle = (event.params.handle ?? '') as Handle;
  return (
    await xrpc<typeof resolveHandle.main>(event, 'com.atproto.identity.resolveHandle', { handle })
  ).did;
}

/** A subject as the queue names it: an account's DID, or a record's URI. */
function keyOf(subject: object): `${string}:${string}` {
  const { did, uri } = subject as { did?: `did:${string}:${string}`; uri?: `at://${string}` };
  return did ?? uri ?? 'did:none:none';
}

/** What happened to `key` in `group`'s queue, oldest first. */
async function history(event: Event, group: Did, key: `${string}:${string}`) {
  const { events } = await xrpc<typeof getSubjectHistory.main>(
    event,
    'fyi.opensocial.getSubjectHistory',
    { group, subject: key }
  );
  return events as Happened[];
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
  const { subjects } = await xrpc<typeof listSubjects.main>(event, 'fyi.opensocial.listSubjects', {
    group,
    status
  });
  const listed: Subject[] = await Promise.all(
    subjects.map(async (s) => {
      const key = keyOf(s.subject);
      return {
        key,
        actor: s.actor ?? { did: key },
        status: s.status,
        reportCount: s.reportCount,
        events: await history(event, group, key)
      };
    })
  );
  // Someone looked up: the row they have, or who they are and what the
  // queue holds of them, on whichever status they are.
  const review = event.url.searchParams.get('review');
  let found: Subject | null = null;
  if (review?.startsWith('did:') && !listed.some((s) => s.key === review)) {
    const did = review as Did;
    const actor = await xrpc<typeof getActor.main>(
      event,
      'fyi.opensocial.getActor',
      { actor: did },
      true
    );
    const events = await history(event, group, did);
    const reports = events.filter((e) => e.event.$type?.endsWith('#reportEvent')).length;
    found = {
      key: did,
      actor: actor ?? { did },
      status: events.length ? 'in the queue' : undefined,
      reportCount: reports,
      events
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

/** The account a form names, as a label's subject. */
const account = (form: FormData) => ({
  $type: 'com.atproto.admin.defs#repoRef',
  did: String(form.get('did'))
});

export const actions: Actions = {
  resolve: async (event) => {
    const group = await groupOf(event);
    const form = await event.request.formData();
    const key = String(form.get('subject'));
    const note = String(form.get('note') ?? '').trim();
    const subject = key.startsWith('did:')
      ? { $type: 'fyi.opensocial.defs#accountSubject', did: key }
      : { $type: 'fyi.opensocial.defs#recordSubject', uri: key };
    return act(event, 'fyi.opensocial.resolveSubject', {
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
    if (!val) return fail(400, { failed: 'Choose or type a label.' });
    return act(event, 'fyi.opensocial.applyLabel', { group, subject: account(form), val });
  },
  unlabel: async (event) => {
    const group = await groupOf(event);
    const form = await event.request.formData();
    const val = String(form.get('val') ?? '');
    return act(event, 'fyi.opensocial.negateLabel', { group, subject: account(form), val });
  },
  report: async (event) => {
    const group = await groupOf(event);
    const form = await event.request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const rule = String(form.get('rule') ?? '');
    return act(event, 'fyi.opensocial.createReport', {
      group,
      subject: { $type: 'fyi.opensocial.defs#accountSubject', did: String(form.get('did')) },
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
    if (!found.ok) return fail(400, { failed: `We can't find ${handle}.` });
    const { did } = await found.json();
    redirect(303, `?review=${encodeURIComponent(did)}`);
  }
};
