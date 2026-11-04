import type * as getViewer from '#lib/lexicons/fyi/opensocial/getViewer.ts';
import type * as describeServer from '#lib/lexicons/com/atproto/server/describeServer.ts';
import type { Actions, PageServerLoad } from './$types';
import { fail, redirect } from '@sveltejs/kit';
import { xrpc } from '#lib/data.ts';
import { finished } from '#lib/handles.ts';

type Event = Parameters<PageServerLoad>[0];

/** The domain this host names its groups under. */
async function domainOf(event: Pick<Event, 'fetch'>) {
	const described = await xrpc<typeof describeServer.main>(event, 'com.atproto.server.describeServer', {});
	return described.availableUserDomains[0];
}

/** Who is starting a group, whether this host lets them, and the domain its address is under. */
export const load: PageServerLoad = async (event) => {
	const { actor } = await xrpc<typeof getViewer.main>(event, 'fyi.opensocial.getViewer', {});
	return { actor: actor ?? null, domain: await domainOf(event) };
};

/** Starts the group, its founder the person signed in, and goes to its page. */
export const actions: Actions = {
	default: async (event) => {
		const form = await event.request.formData();
		const text = (k: string) => String(form.get(k) ?? '').trim();
		const input = {
			// As the form made it, and again for a page drawn without script.
			handle: `${finished(text('label'))}${await domainOf(event)}`,
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
