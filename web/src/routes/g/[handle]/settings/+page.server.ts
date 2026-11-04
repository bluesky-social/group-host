import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getGroup from '#lib/lexicons/fyi/opensocial/getGroup.ts';
import type * as listAppPasswords from '#lib/lexicons/fyi/opensocial/listAppPasswords.ts';
import type * as listGroupTokens from '#lib/lexicons/fyi/opensocial/listGroupTokens.ts';
import type * as listRecoveryKeys from '#lib/lexicons/fyi/opensocial/listRecoveryKeys.ts';
import type { Actions, PageServerLoad } from './$types';
import { fail } from '@sveltejs/kit';
import { act, xrpc } from '#lib/data.ts';

type Handle = `${string}.${string}`;
type Event = { fetch: typeof fetch; params: { handle?: string } };

/** The group the page's address names, by its handle: its DID. */
async function groupOf(event: Event) {
	const handle = (event.params.handle ?? '') as Handle;
	return (await xrpc<typeof resolveHandle.main>(event, 'com.atproto.identity.resolveHandle', { handle })).did;
}

/**
 * A group's settings, each part for whoever the group lets: its profile,
 * rules and roles, and the apps acting as it, to its configurers; its app
 * passwords, to them and to whoever may make one.
 */
export const load: PageServerLoad = async (event) => {
	const group = await groupOf(event);
	const view = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', { group });
	const configures = view.group.viewer?.actions?.includes('group.configure') ?? false;
	const tokens = configures ? await xrpc<typeof listGroupTokens.main>(event, 'fyi.opensocial.listGroupTokens', { group }) : null;
	const recovery = configures ? await xrpc<typeof listRecoveryKeys.main>(event, 'fyi.opensocial.listRecoveryKeys', { group }).catch(() => null) : null;
	const passwords = await xrpc<typeof listAppPasswords.main>(event, 'fyi.opensocial.listAppPasswords', { group }).catch(() => null);
	return { ...view, configures, tokens, recovery, passwords: passwords?.passwords ?? null };
};

async function asked(event: Event & { request: Request }) {
	return { group: await groupOf(event), form: await event.request.formData() };
}

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();

/** What a group's settings change: each drawn again after, or the host's refusal said. */
export const actions: Actions = {
	profile: async (event) => {
		const { group, form } = await asked(event);
		// Replaced whole: what the form does not show is kept as it is.
		const { group: now } = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', { group });
		const kept = Object.fromEntries(['location', 'online', 'topics', 'languages'].flatMap((k) => (k in now ? [[k, now[k as keyof typeof now]]] : [])));
		return act(event, 'fyi.opensocial.updateProfile', {
			...kept,
			group,
			displayName: text(form, 'displayName'),
			description: text(form, 'description') || undefined,
			joinPolicy: text(form, 'joinPolicy') || undefined,
			url: text(form, 'url') || undefined
		});
	},
	rule: async (event) => {
		const { group, form } = await asked(event);
		return act(event, 'fyi.opensocial.putRule', { group, title: text(form, 'title'), text: text(form, 'text') || undefined });
	},
	unrule: async (event) => {
		const { group, form } = await asked(event);
		return act(event, 'fyi.opensocial.deleteRule', { group, rkey: text(form, 'rkey') });
	},
	role: async (event) => {
		const { group, form } = await asked(event);
		return act(event, 'fyi.opensocial.putRole', {
			group,
			role: text(form, 'role'),
			name: text(form, 'name'),
			actions: form.getAll('action').map(String),
			assignable: form.getAll('assignable').map((r) => ({ role: String(r) })),
			default: form.get('default') === 'on'
		});
	},
	unrole: async (event) => {
		const { group, form } = await asked(event);
		return act(event, 'fyi.opensocial.deleteRole', { group, role: text(form, 'role') });
	},
	revoke: async (event) => {
		const { group, form } = await asked(event);
		return act(event, 'fyi.opensocial.revokeGroupToken', { group, tokenId: text(form, 'tokenId') });
	},
	password: async (event) => {
		const { group, form } = await asked(event);
		const res = await event.fetch('/xrpc/fyi.opensocial.createAppPassword', {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify({ group, name: text(form, 'name'), privileged: form.get('privileged') === 'on' })
		});
		const answer = await res.json().catch(() => null);
		if (!res.ok) return fail(res.status, { failed: answer?.message ?? res.statusText });
		return { made: { name: answer.name as string, password: answer.password as string } };
	},
	key: async (event) => {
		const { group, form } = await asked(event);
		return act(event, 'fyi.opensocial.addRecoveryKey', { group, key: text(form, 'key') });
	},
	unkey: async (event) => {
		const { group, form } = await asked(event);
		return act(event, 'fyi.opensocial.removeRecoveryKey', { group, key: text(form, 'key') });
	},
	unpassword: async (event) => {
		const { group, form } = await asked(event);
		return act(event, 'fyi.opensocial.revokeAppPassword', { group, name: text(form, 'name') });
	}
};
