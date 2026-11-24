// Writing as a group from a form's action: a group token from
// getGroupAuth for one space, bound to a DPoP key made for it, the writes
// made with it through com.atproto.space.* (and the files they hold, with
// com.atproto.repo.uploadBlob), as any app writes as a group, and the
// token ended once they are made.

import { createHash, generateKeyPairSync, randomBytes, sign, type KeyObject } from 'node:crypto';
import { fail, type ActionFailure } from '@sveltejs/kit';
import { refused as said } from '#lib/errors.ts';

type Load = { fetch: typeof fetch };

/** The host's refusal, as a form says it. */
export type Failed = ActionFailure<{ failed: string }>;
/** What a write as the group comes to: nothing, or the host's refusal. */
export type Done = Failed | undefined;

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
    const claims: Record<string, unknown> = {
      jti: b64(randomBytes(12)),
      htm: 'POST',
      htu: `${this.host}/xrpc/${nsid}`,
      iat: Math.floor(Date.now() / 1000)
    };
    if (this.nonce) claims.nonce = this.nonce;
    if (token) claims.ath = b64(createHash('sha256').update(token).digest());
    const body = `${b64(JSON.stringify(header))}.${b64(JSON.stringify(claims))}`;
    const signature = sign('sha256', Buffer.from(body), {
      key: this.keys.privateKey,
      dsaEncoding: 'ieee-p1363'
    });
    return `${body}.${b64(signature)}`;
  }

  /**
   * POSTs `body`, of the type `type`, to `nsid` with a fresh proof, once
   * more with the nonce if the host asks for one.
   */
  async post(
    { fetch }: Load,
    nsid: string,
    body: BodyInit,
    type: string,
    token?: string
  ): Promise<Response> {
    for (let tries = 0; ; tries++) {
      const headers: Record<string, string> = {
        'content-type': type,
        dpop: this.proof(nsid, token)
      };
      if (token) headers.authorization = `DPoP ${token}`;
      const res = await fetch(`/xrpc/${nsid}`, { method: 'POST', headers, body });
      this.nonce = res.headers.get('dpop-nonce') ?? this.nonce;
      const stale = res.headers.get('www-authenticate')?.includes('use_dpop_nonce');
      if (!stale || tries > 0) return res;
    }
  }
}

/** Why the host refused, as a form says it. */
async function refused(res: Response): Promise<Failed> {
  return fail(res.status, { failed: await said(res) });
}

/** The group, acting in one of its spaces with a token for it. */
export class GroupSession {
  constructor(
    private readonly load: Load,
    private readonly prover: Prover,
    private readonly token: string,
    readonly group: string,
    readonly space: string
  ) {}

  /** The write `nsid` with `input`, in the session's space: nothing, or the host's refusal. */
  async write(nsid: string, input: object): Promise<Done> {
    const body = JSON.stringify({ repo: this.group, space: this.space, ...input });
    const res = await this.prover.post(this.load, nsid, body, 'application/json', this.token);
    return res.ok ? undefined : refused(res);
  }

  /** `bytes`, of the type `type`, uploaded as the group's: the blob, or the host's refusal. */
  async upload(bytes: ArrayBuffer, type: string): Promise<{ blob: unknown } | Failed> {
    const res = await this.prover.post(
      this.load,
      'com.atproto.repo.uploadBlob',
      bytes,
      type,
      this.token
    );
    return res.ok ? { blob: (await res.json()).blob } : refused(res);
  }

  /** Ends the token, done with: it stays among the group's sessions no longer. */
  async end() {
    const body = new URLSearchParams({ token: this.token });
    await this.load.fetch('/oauth/revoke', {
      method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' },
      body
    });
  }
}

/**
 * What `act` comes to, given the group's session in `space`, from a
 * form's action, as the person signed in: as far as their roles let them
 * write there. The session is ended after. If the host gives none, the
 * form says why.
 */
export async function withGroup(
  load: Load,
  group: string,
  space: string,
  act: (session: GroupSession) => Promise<Done>
): Promise<Done> {
  // A proof names the host as it names itself.
  const { resource } = await (await load.fetch('/.well-known/oauth-protected-resource')).json();
  const prover = new Prover(resource);
  const given = await prover.post(
    load,
    'fyi.opensocial.internal.getGroupAuth',
    JSON.stringify({ group, space }),
    'application/json'
  );
  if (!given.ok) return refused(given);
  const { accessJwt } = await given.json();
  const session = new GroupSession(load, prover, accessJwt, group, space);
  try {
    return await act(session);
  } finally {
    await session.end();
  }
}

/**
 * Makes the write `nsid` (com.atproto.space.createRecord, and the rest)
 * with `input` as `group`, in `space`, from a form's action, as the person
 * signed in. If the host refuses, the form says why.
 */
export async function asGroup(
  load: Load,
  group: string,
  space: string,
  nsid: string,
  input: object
): Promise<Done> {
  return withGroup(load, group, space, (session) => session.write(nsid, input));
}

/** One write, as com.atproto.space.applyWrites takes it. */
export type Write = {
  action: 'create' | 'update' | 'delete';
  collection: string;
  rkey: string;
  value?: object;
};

/** A group's members space. */
export const membersOf = (group: string) => `at://${group}/space/fyi.opensocial.members/self`;

/** Makes `writes` at once as `group`, in `space`, as {@link asGroup} makes one. */
export function applyAsGroup(load: Load, group: string, space: string, writes: Write[]) {
  const nsid = 'com.atproto.space.applyWrites';
  const listed = writes.map(({ action, collection, rkey, value }) => ({
    $type: `${nsid}#${action}`,
    collection,
    rkey,
    value
  }));
  return asGroup(load, group, space, nsid, { writes: listed });
}
