import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getGroup from '#lib/lexicons/fyi/opensocial/getGroup.ts';
import type * as getSubjectHistory from '#lib/lexicons/fyi/opensocial/getSubjectHistory.ts';
import type * as listSubjects from '#lib/lexicons/fyi/opensocial/listSubjects.ts';
import type { Actions, PageServerLoad } from './$types';
import { fail } from '@sveltejs/kit';
import { act, xrpc } from '#lib/data.ts';

type Handle = `${string}.${string}`;
type Event = { fetch: typeof fetch; params: { handle?: string } };

async function groupOf(event: Event) {
	const handle = (event.params.handle ?? '') as Handle;
	return (await xrpc<typeof resolveHandle.main>(event, 'com.atproto.identity.resolveHandle', { handle })).did;
}

/** A group's queue, by status, each subject with its history, for one who may read it. */
export const load: PageServerLoad = async (event) => {
	const group = await groupOf(event);
	const { group: view } = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', { group });
	const actions = view.viewer?.actions ?? [];
	if (!actions.includes('mod.read')) return { group: view, actions, subjects: null };
	const status = event.url.searchParams.get('status') ?? undefined;
	const { subjects } = await xrpc<typeof listSubjects.main>(event, 'fyi.opensocial.listSubjects', { group, ...(status ? { status } : {}) });
	const withEvents = await Promise.all(
		subjects.map(async (s) => {
			const { events } = await xrpc<typeof getSubjectHistory.main>(event, 'fyi.opensocial.getSubjectHistory', { group, subject: s.subject });
			return { ...s, events };
		})
	);
	return { group: view, actions, subjects: withEvents };
};

export const actions: Actions = {
	resolve: async (event) => {
		const group = await groupOf(event);
		const form = await event.request.formData();
		const note = String(form.get('note') ?? '').trim();
		return act(event, 'fyi.opensocial.resolveSubject', {
			group,
			subject: String(form.get('subject')),
			action: String(form.get('action')),
			...(note ? { note } : {})
		});
	},
	label: async (event) => {
		const group = await groupOf(event);
		const form = await event.request.formData();
		const handle = String(form.get('handle') ?? '').trim().replace(/^@/, '').toLowerCase();
		const found = await event.fetch(`/xrpc/com.atproto.identity.resolveHandle?handle=${encodeURIComponent(handle)}`);
		if (!found.ok) return fail(400, { failed: `We cannot find ${handle}.` });
		const { did } = await found.json();
		return act(event, 'fyi.opensocial.applyLabel', { group, subject: { $type: 'com.atproto.admin.defs#repoRef', did }, val: String(form.get('val') ?? '').trim() });
	}
};
