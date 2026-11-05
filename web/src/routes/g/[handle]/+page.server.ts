import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getGroup from '#lib/lexicons/fyi/opensocial/getGroup.ts';
import type * as listConnectedApps from '#lib/lexicons/fyi/opensocial/listConnectedApps.ts';
import type * as listInvites from '#lib/lexicons/fyi/opensocial/listInvites.ts';
import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { act, xrpc } from '#lib/data.ts';
import { asGroup, membersOf } from '#lib/asGroup.ts';

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

const MEMBERSHIP = 'fyi.opensocial.membership';

/**
 * Writes `who`'s membership of `group`, holding `roles`, as the group:
 * admitting one who asked, or changing a member's roles.
 */
async function membership(event: { fetch: typeof fetch }, group: string, who: string, roles: string[], action: 'create' | 'update') {
	const record = { $type: MEMBERSHIP, member: who, roles, createdAt: new Date().toISOString() };
	const nsid = action === 'create' ? 'com.atproto.space.createRecord' : 'com.atproto.space.putRecord';
	return asGroup(event, group, membersOf(group), nsid, { collection: MEMBERSHIP, rkey: who, record });
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
		// With the roles every new member gets, and member, which everyone holds.
		const { roles } = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', { group: group as `did:${string}:${string}` });
		const given = (roles ?? []).filter((r) => r.default).map((r) => r.key);
		return membership(event, group, who, [...new Set(['member', ...given])], 'create');
	},
	deny: async (event) => {
		const { group, who } = await asked(event);
		return act(event, 'fyi.opensocial.denyJoinRequest', { group, member: who });
	},
	roles: async (event) => {
		const { group, who, form } = await asked(event);
		return membership(event, group, who, form.getAll('role').map(String), 'update');
	},
	eject: async (event) => {
		const { group, who } = await asked(event);
		return asGroup(event, group, membersOf(group), 'com.atproto.space.deleteRecord', { collection: MEMBERSHIP, rkey: who });
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
