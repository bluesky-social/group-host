// Writing as a group from a form's action: a group token from
// getGroupAuth for one space, bound to a DPoP key made for it, and the
// write made with it through com.atproto.space.*, as any app writes as a
// group.

import { createHash, generateKeyPairSync, randomBytes, sign, type KeyObject } from 'node:crypto';
import { fail } from '@sveltejs/kit';
import type { LexErrorData } from '@atproto/lex';

type Load = { fetch: typeof fetch };

const b64 = (bytes: Buffer | Uint8Array | string) => Buffer.from(bytes).toString('base64url');

/** A key to prove a group token's requests with, and the host's nonce as it last gave it. */
class Prover {
	keys = generateKeyPairSync('ec', { namedCurve: 'P-256' });
	nonce: string | undefined;

	constructor(readonly host: string) {}

	/** A DPoP proof of a POST to `nsid`, for `token` if it carries one. */
	proof(nsid: string, token?: string): string {
		const jwk = (this.keys.publicKey as KeyObject).export({ format: 'jwk' });
		const header = { typ: 'dpop+jwt', alg: 'ES256', jwk };
		const claims: Record<string, unknown> = { jti: b64(randomBytes(12)), htm: 'POST', htu: `${this.host}/xrpc/${nsid}`, iat: Math.floor(Date.now() / 1000) };
		if (this.nonce) claims.nonce = this.nonce;
		if (token) claims.ath = b64(createHash('sha256').update(token).digest());
		const body = `${b64(JSON.stringify(header))}.${b64(JSON.stringify(claims))}`;
		const signature = sign('sha256', Buffer.from(body), { key: this.keys.privateKey, dsaEncoding: 'ieee-p1363' });
		return `${body}.${b64(signature)}`;
	}

	/** POSTs `input` to `nsid` with a fresh proof, once more with the nonce if the host asks for one. */
	async post({ fetch }: Load, nsid: string, input: object, token?: string): Promise<Response> {
		for (let tries = 0; ; tries++) {
			const headers: Record<string, string> = { 'content-type': 'application/json', dpop: this.proof(nsid, token) };
			if (token) headers.authorization = `DPoP ${token}`;
			const res = await fetch(`/xrpc/${nsid}`, { method: 'POST', headers, body: JSON.stringify(input) });
			this.nonce = res.headers.get('dpop-nonce') ?? this.nonce;
			const stale = res.headers.get('www-authenticate')?.includes('use_dpop_nonce');
			if (!stale || tries > 0) return res;
		}
	}
}

/** Why the host refused, as a form says it. */
async function refused(res: Response) {
	const failure: Partial<LexErrorData> | null = await res.json().catch(() => null);
	return fail(res.status, { failed: failure?.message ?? res.statusText });
}

/**
 * Makes the write `nsid` (com.atproto.space.createRecord, and the rest)
 * with `input` as `group`, in `space`, from a form's action, as the person
 * signed in: with a group token for that space, as far as their roles let
 * them write there. If the host refuses, the form says why.
 */
export async function asGroup(load: Load, group: string, space: string, nsid: string, input: object) {
	// A proof names the host as it names itself.
	const { resource } = await (await load.fetch('/.well-known/oauth-protected-resource')).json();
	const prover = new Prover(resource);
	const given = await prover.post(load, 'fyi.opensocial.getGroupAuth', { group, space });
	if (!given.ok) return refused(given);
	const { accessJwt } = await given.json();
	const res = await prover.post(load, nsid, { repo: group, space, ...input }, accessJwt);
	if (!res.ok) return refused(res);
}
