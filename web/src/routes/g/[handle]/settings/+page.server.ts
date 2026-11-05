import type * as resolveHandle from '#lib/lexicons/com/atproto/identity/resolveHandle.ts';
import type * as getGroup from '#lib/lexicons/fyi/opensocial/getGroup.ts';
import type * as listAppPasswords from '#lib/lexicons/fyi/opensocial/listAppPasswords.ts';
import type * as listGroupTokens from '#lib/lexicons/fyi/opensocial/listGroupTokens.ts';
import type * as listRecoveryKeys from '#lib/lexicons/fyi/opensocial/listRecoveryKeys.ts';
import type { Actions, PageServerLoad } from './$types';
import { fail } from '@sveltejs/kit';
import { act, xrpc, type Answer } from '#lib/data.ts';
import { applyAsGroup, membersOf, withGroup, type Done, type GroupSession, type Write } from '#lib/asGroup.ts';
import { SCOPES } from '#lib/actions.ts';

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
	// Listed while anyone may read its profile: its meta space is public.
	const meta = view.spaces?.find((s) => s.uri === view.group.meta);
	const listed = meta?.public ?? false;
	return { ...view, configures, tokens, recovery, passwords: passwords?.passwords ?? null, listed };
};

async function asked(event: Event & { request: Request }) {
	return { group: await groupOf(event), form: await event.request.formData() };
}

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();

const PROFILE = 'fyi.opensocial.profile';
const RULE = 'fyi.opensocial.rule';

type Did = `did:${string}:${string}`;
type Bindings = Answer<typeof getGroup.main>['bindings'];

/** The grants of the group's permissions record, as getGroup shows them. */
const grantsOf = (bindings: Bindings) =>
	(bindings ?? []).map((b) => ({ role: b.role, scopes: b.scopes?.map((s) => s.scope) ?? [], assignable: b.assignable.map((a) => a.role) }));

/** The permissions record holding `grants`, written in place of the one there. */
const permissions = (grants: ReturnType<typeof grantsOf>): Write => ({
	action: 'update',
	collection: 'fyi.opensocial.permissions',
	rkey: 'self',
	value: { $type: 'fyi.opensocial.permissions', grants, createdAt: new Date().toISOString() }
});

/**
 * A picture the profile names by its CID, as the record holds it: its
 * type and size as the host serves it.
 */
async function picture(event: Event, did: string, cid?: string) {
	if (!cid) return [];
	const res = await event.fetch(`/img/${did}/${cid}`);
	if (!res.ok) return [];
	const size = (await res.arrayBuffer()).byteLength;
	return [{ $type: 'blob', ref: { $link: cid }, mimeType: res.headers.get('content-type'), size }];
}

/**
 * The group's meta space, and its profile there as it is now, to put back
 * whole with what is changed: what the form does not show is kept.
 */
async function profileNow(event: Event, group: string) {
	const { group: now } = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', { group: group as `did:${string}:${string}` });
	const kept = Object.fromEntries(['description', 'url', 'links', 'location', 'online', 'topics', 'languages'].flatMap((k) => (k in now ? [[k, now[k as keyof typeof now]]] : [])));
	const [avatar] = await picture(event, now.did, now.avatar);
	const [banner] = await picture(event, now.did, now.banner);
	const pictures = { ...(avatar ? { avatar } : {}), ...(banner ? { banner } : {}) };
	const profile: Record<string, unknown> = { ...kept, ...pictures, displayName: now.displayName ?? now.handle, joinPolicy: now.joinPolicy ?? 'approval' };
	// Shown only to someone signed in, as everyone here is.
	return { meta: now.meta ?? '', profile };
}

type Profile = Record<string, unknown>;

/** What `write` comes to, given the group's session in its meta space and its profile now: or why not. */
async function asMeta(event: Event, group: string, write: (session: GroupSession, profile: Profile) => Promise<Done>) {
	const { meta, profile } = await profileNow(event, group);
	return withGroup(event, group, meta, (session) => write(session, profile));
}

/** The profile put back whole, as the group. */
const putProfile = (session: GroupSession, profile: Profile) => session.write('com.atproto.space.putRecord', { collection: PROFILE, rkey: 'self', record: { $type: PROFILE, ...profile } });

/** What a group's settings change: each drawn again after, or the host's refusal said. */
export const actions: Actions = {
	picture: async (event) => {
		const { group, form } = await asked(event);
		const slot = text(form, 'slot') === 'banner' ? 'banner' : 'avatar';
		const file = form.get('file');
		if (!(file instanceof File) || file.size === 0) return fail(400, { failed: 'Choose a picture to upload.' });
		const bytes = await file.arrayBuffer();
		return asMeta(event, group, async (session, profile) => {
			const up = await session.upload(bytes, file.type || 'application/octet-stream');
			if (!('blob' in up)) return up;
			return putProfile(session, { ...profile, [slot]: up.blob });
		});
	},
	unpicture: async (event) => {
		const { group, form } = await asked(event);
		const slot = text(form, 'slot') === 'banner' ? 'banner' : 'avatar';
		return asMeta(event, group, async (session, profile) => {
			const kept = { ...profile };
			delete kept[slot];
			return putProfile(session, kept);
		});
	},
	visibility: async (event) => {
		const { group, form } = await asked(event);
		const { group: now, spaces } = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', { group });
		const meta = spaces?.find((s) => s.uri === now.meta);
		// Members read it either way: as anyone signed in, or by their grant.
		const read = 'space:fyi.opensocial.meta?action=read';
		const member = meta?.grants?.find((g) => g.role === 'member');
		const grants = [...(meta?.grants ?? []).filter((g) => g !== member), { role: 'member', scopes: [...new Set([...(member?.scopes ?? []), read])] }];
		const access = { public: text(form, 'seen') === 'public', grants, writableBy: meta?.writableBy };
		return act(event, 'fyi.opensocial.updateSpace', { group, space: now.meta, access });
	},
	profile: async (event) => {
		const { group, form } = await asked(event);
		return asMeta(event, group, async (session, now) => {
			// The form's own fields replace what is there: one left empty goes.
			const profile: Record<string, unknown> = { ...now, displayName: text(form, 'displayName'), joinPolicy: text(form, 'joinPolicy') || now.joinPolicy };
			for (const k of ['description', 'url']) {
				if (text(form, k)) profile[k] = text(form, k);
				else delete profile[k];
			}
			// Each link with an address, with its title if it has one.
			const titles = form.getAll('linkTitle').map((t) => String(t).trim());
			const links = form
				.getAll('linkUrl')
				.map((u, i) => ({ url: String(u).trim(), title: titles[i] ?? '' }))
				.filter((l) => l.url)
				.map((l) => (l.title ? l : { url: l.url }));
			if (links.length) profile.links = links;
			else delete profile.links;
			return putProfile(session, profile);
		});
	},
	rule: async (event) => {
		const { group, form } = await asked(event);
		const record = { $type: RULE, title: text(form, 'title'), text: text(form, 'text') };
		return asMeta(event, group, (session) => session.write('com.atproto.space.createRecord', { collection: RULE, record }));
	},
	unrule: async (event) => {
		const { group, form } = await asked(event);
		const rkey = text(form, 'rkey');
		return asMeta(event, group, (session) => session.write('com.atproto.space.deleteRecord', { collection: RULE, rkey }));
	},
	role: async (event) => {
		const { group, form } = await asked(event);
		const key = text(form, 'role');
		const { group: now, roles, bindings } = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', { group: group as Did });
		const was = roles?.find((r) => r.key === key);
		// What the role's record does not show here stays as it was.
		const record = { ...(was?.description ? { description: was.description } : {}), ...(was?.bot ? { bot: true } : {}) };
		const role = { $type: 'fyi.opensocial.role', ...record, displayName: text(form, 'name'), default: form.get('default') === 'on', createdAt: new Date().toISOString() };
		// Its grant: the actions as their scopes, beside any other scope it held.
		const named = new Set(Object.values(SCOPES));
		const kept = grantsOf(bindings).find((g) => g.role === key)?.scopes.filter((s) => !named.has(s)) ?? [];
		const actions = form.getAll('action').map((a) => SCOPES[String(a) as keyof typeof SCOPES]);
		const grant = { role: key, scopes: [...kept, ...actions], assignable: form.getAll('assignable').map(String) };
		// A new role is given by those who run the group, as they run it.
		const mine = now.viewer?.roles.map((r) => r.role) ?? [];
		const grants = grantsOf(bindings)
			.filter((g) => g.role !== key)
			.map((g) => (!was && mine.includes(g.role) && g.scopes.includes(SCOPES['group.configure']) ? { ...g, assignable: [...g.assignable, key] } : g));
		return applyAsGroup(event, group, membersOf(group), [
			{ action: was ? 'update' : 'create', collection: 'fyi.opensocial.role', rkey: key, value: role },
			permissions([...grants, grant])
		]);
	},
	unrole: async (event) => {
		const { group, form } = await asked(event);
		const key = text(form, 'role');
		const { bindings } = await xrpc<typeof getGroup.main>(event, 'fyi.opensocial.getGroup', { group: group as Did });
		// The role and its grant go together.
		return applyAsGroup(event, group, membersOf(group), [
			{ action: 'delete', collection: 'fyi.opensocial.role', rkey: key },
			permissions(grantsOf(bindings).filter((g) => g.role !== key))
		]);
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
