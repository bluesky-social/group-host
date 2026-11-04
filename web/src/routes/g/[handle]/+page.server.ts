import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getGroup from '#lib/lexicons/fyi/opensocial/getGroup.ts';
import type { Actions, PageServerLoad } from './$types';
import { call, xrpc } from '#lib/data.ts';

type Handle = `${string}.${string}`;

/** The group the page's address names, by its handle: its DID. */
async function groupOf(event: { fetch: typeof fetch; params: { handle?: string } }) {
	const handle = (event.params.handle ?? '') as Handle;
	const answer = await xrpc<typeof resolveHandle.main>(event, 'com.atproto.identity.resolveHandle', { handle });
	return answer.did;
}

/** A group, by its handle, as the person looking may see it. */
export const load: PageServerLoad = async (event) => {
	return xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', { group: await groupOf(event) });
};

/** The group a form on its page is about, and the DID a form names. */
async function asked(event: { fetch: typeof fetch; params: { handle?: string }; request: Request }) {
	const form = await event.request.formData();
	return { group: await groupOf(event), who: String(form.get('did') ?? '') };
}

/** What a person does on a group's page: each drawn again after. */
export const actions: Actions = {
	join: async (event) => {
		const { group } = await asked(event);
		await call(event, 'fyi.opensocial.requestJoin', { group });
	},
	cancel: async (event) => {
		const { group } = await asked(event);
		await call(event, 'fyi.opensocial.cancelJoinRequest', { group });
	},
	leave: async (event) => {
		const { group } = await asked(event);
		await call(event, 'fyi.opensocial.leaveGroup', { group });
	},
	admit: async (event) => {
		const { group, who } = await asked(event);
		await call(event, 'fyi.opensocial.admitMember', { group, did: who });
	},
	deny: async (event) => {
		const { group, who } = await asked(event);
		await call(event, 'fyi.opensocial.denyJoinRequest', { group, member: who });
	}
};
