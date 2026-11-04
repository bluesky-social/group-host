import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getGroup from '#lib/lexicons/fyi/opensocial/getGroup.ts';
import type * as listInvites from '#lib/lexicons/fyi/opensocial/listInvites.ts';
import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { act, xrpc } from '#lib/data.ts';

type Handle = `${string}.${string}`;

/** The group the page's address names, by its handle: its DID. */
async function groupOf(event: { fetch: typeof fetch; params: { handle?: string } }) {
	const handle = (event.params.handle ?? '') as Handle;
	const answer = await xrpc<typeof resolveHandle.main>(event, 'com.atproto.identity.resolveHandle', { handle });
	return answer.did;
}

/**
 * A group, by its handle, as the person looking may see it; and its
 * invites waiting, for one who may invite.
 */
export const load: PageServerLoad = async (event) => {
	const group = await groupOf(event);
	const view = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', { group });
	const invites = view.group.viewer?.actions?.includes('invite')
		? (await xrpc<typeof listInvites.main>(event, 'fyi.opensocial.listInvites', { group })).invites
		: null;
	return { ...view, invites };
};

/** The group a form on its page is about, and the DID a form names. */
async function asked(event: { fetch: typeof fetch; params: { handle?: string }; request: Request }) {
	const form = await event.request.formData();
	return { group: await groupOf(event), who: String(form.get('did') ?? ''), form };
}

/** What a person does on a group's page: each drawn again after. */
export const actions: Actions = {
	join: async (event) => {
		const { group } = await asked(event);
		return act(event, 'fyi.opensocial.requestJoin', { group });
	},
	cancel: async (event) => {
		const { group } = await asked(event);
		return act(event, 'fyi.opensocial.cancelJoinRequest', { group });
	},
	leave: async (event) => {
		const { group } = await asked(event);
		return act(event, 'fyi.opensocial.leaveGroup', { group });
	},
	admit: async (event) => {
		const { group, who } = await asked(event);
		return act(event, 'fyi.opensocial.admitMember', { group, did: who });
	},
	deny: async (event) => {
		const { group, who } = await asked(event);
		return act(event, 'fyi.opensocial.denyJoinRequest', { group, member: who });
	},
	roles: async (event) => {
		const { group, who, form } = await asked(event);
		const roles = form.getAll('role').map((r) => ({ role: String(r) }));
		return act(event, 'fyi.opensocial.assignRoles', { group, member: who, roles });
	},
	eject: async (event) => {
		const { group, who } = await asked(event);
		return act(event, 'fyi.opensocial.ejectMember', { group, member: who });
	},
	invite: async (event) => {
		const { group, form } = await asked(event);
		const handle = String(form.get('handle') ?? '').trim().replace(/^@/, '').toLowerCase() as Handle;
		const found = await event.fetch(`/xrpc/com.atproto.identity.resolveHandle?handle=${encodeURIComponent(handle)}`);
		if (!found.ok) return fail(400, { failed: `We cannot find ${handle}.` });
		const { did } = await found.json();
		return act(event, 'fyi.opensocial.createInvite', { group, invitee: did });
	},
	uninvite: async (event) => {
		const { group, form } = await asked(event);
		return act(event, 'fyi.opensocial.revokeInvite', { group, uri: String(form.get('uri')) });
	}
};
