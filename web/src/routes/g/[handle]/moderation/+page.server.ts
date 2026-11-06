import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getGroup from '#lib/lexicons/fyi/opensocial/getGroup.ts';
import type * as getSubjectHistory from '#lib/lexicons/fyi/opensocial/getSubjectHistory.ts';
import type * as listSubjects from '#lib/lexicons/fyi/opensocial/listSubjects.ts';
import type { Actions, PageServerLoad } from './$types';
import { fail } from '@sveltejs/kit';
import { act, xrpc } from '#lib/data.ts';

type Handle = `${string}.${string}`;
type Event = { fetch: typeof fetch; params: { handle?: string } };
type Status = 'open' | 'escalated' | 'resolved';
const STATUSES: string[] = ['open', 'escalated', 'resolved'];

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

/** A group's queue of one status, open unless asked, each subject with its history, for one who may read it. */
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
  const withEvents = await Promise.all(
    subjects.map(async (s) => {
      const key = keyOf(s.subject);
      const { events } = await xrpc<typeof getSubjectHistory.main>(
        event,
        'fyi.opensocial.getSubjectHistory',
        { group, subject: key }
      );
      return {
        key,
        actor: s.actor ?? { did: key },
        status: s.status,
        reportCount: s.reportCount,
        events
      };
    })
  );
  // The rules a report may cite, by their record key.
  const rules = Object.fromEntries((view.rules ?? []).map((r) => [r.rkey, r.title]));
  return { group: view, actions, status, subjects: withEvents, rules };
};

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
    const handle = String(form.get('handle') ?? '')
      .trim()
      .replace(/^@/, '')
      .toLowerCase();
    const found = await event.fetch(
      `/xrpc/com.atproto.identity.resolveHandle?handle=${encodeURIComponent(handle)}`
    );
    if (!found.ok) return fail(400, { failed: `We can't find ${handle}.` });
    const { did } = await found.json();
    return act(event, 'fyi.opensocial.applyLabel', {
      group,
      subject: { $type: 'com.atproto.admin.defs#repoRef', did },
      val: String(form.get('val') ?? '').trim()
    });
  }
};
