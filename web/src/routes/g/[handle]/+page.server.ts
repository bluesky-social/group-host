import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getGroup from '#lib/lexicons/fyi/opensocial/getGroup.ts';
import type * as listConnectedApps from '#lib/lexicons/fyi/opensocial/listConnectedApps.ts';
import type * as listInvites from '#lib/lexicons/fyi/opensocial/listInvites.ts';
import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { act, xrpc } from '#lib/data.ts';
import { asGroup } from '#lib/asGroup.ts';

type Handle = `${string}.${string}`;

/** Someone's inbox, where groups write their invites to them. */
const inboxOf = (did: string) => `at://${did}/space/fyi.opensocial.invites/self`;

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
	// The assistants a member connected here, and where they connect one.
	const apps = view.group.viewer?.member ? (await xrpc<typeof listConnectedApps.main>(event, 'fyi.opensocial.listConnectedApps', { group })).apps : null;
	return { ...view, invites, apps, mcp: `${event.url.origin}/mcp` };
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
		// Written as the group, in their own inbox.
		const record = { $type: 'fyi.opensocial.invite', createdAt: new Date().toISOString() };
		return asGroup(event, group, inboxOf(did), 'com.atproto.space.createRecord', { collection: 'fyi.opensocial.invite', record });
	},
	disconnect: async (event) => {
		const { group, form } = await asked(event);
		return act(event, 'fyi.opensocial.revokeConnectedApp', { group, clientId: String(form.get('clientId')) });
	},
	uninvite: async (event) => {
		const { group, form } = await asked(event);
		// Taken back by deleting it, as the group, from their inbox.
		const uri = String(form.get('uri') ?? '');
		const [inbox, rkey] = [uri.split(`/${group}/`)[0], uri.split('/').pop()];
		return asGroup(event, group, inbox, 'com.atproto.space.deleteRecord', { collection: 'fyi.opensocial.invite', rkey });
	}
};
