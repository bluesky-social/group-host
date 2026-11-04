import type * as getViewer from '#lib/lexicons/fyi/opensocial/getViewer.ts';
import type { Actions, PageServerLoad } from './$types';
import { fail, redirect } from '@sveltejs/kit';
import { xrpc } from '#lib/data.ts';

/** Who is starting a group, and whether this host lets them. */
export const load: PageServerLoad = async (event) => {
	const { actor } = await xrpc<typeof getViewer.main>(event, 'fyi.opensocial.getViewer', {});
	return { actor: actor ?? null };
};

/** Starts the group, its founder the person signed in, and goes to its page. */
export const actions: Actions = {
	default: async (event) => {
		const form = await event.request.formData();
		const text = (k: string) => String(form.get(k) ?? '').trim();
		const input = {
			handle: text('handle').toLowerCase(),
			displayName: text('displayName'),
			description: text('description') || undefined,
			joinPolicy: text('joinPolicy') || undefined
		};
		const res = await event.fetch('/xrpc/fyi.opensocial.createGroup', {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(input)
		});
		if (!res.ok) {
			const failure = await res.json().catch(() => null);
			return fail(res.status, { failed: failure?.message ?? res.statusText });
		}
		redirect(303, `/g/${input.handle}`);
	}
};
