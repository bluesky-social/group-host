import { type DidString, XrpcResponseError, cidForLex, type l } from '@atproto/lex';
import type { SpaceReader } from '@atproto/lex-space';
import { describe, expect, it, vi } from 'vitest';
import { InviteInbox } from './invite-inbox.ts';

const ALICE = 'did:plc:alice';
const CLUB = 'did:plc:club';
const INBOX = `at://${ALICE}/space/fyi.opensocial.invites/self`;
const POLICY = { $type: 'com.atproto.memberspace.policy', publicWrite: true };

type Asked = { nsid: string; params: URLSearchParams; body: unknown };

/** Alice's agent, her PDS answering each method with `routes[nsid]`. */
function stubAgent(routes: Record<string, () => Response>) {
  const asked: Asked[] = [];
  const agent = {
    did: ALICE,
    fetchHandler: async (path: string, init: RequestInit) => {
      const url = new URL(path, 'https://pds.example');
      const nsid = url.pathname.replace('/xrpc/', '');
      const body = typeof init.body === 'string' ? JSON.parse(init.body) : undefined;
      asked.push({ nsid, params: url.searchParams, body });
      const route = routes[nsid];
      if (!route) throw new Error(`Unexpected request for ${nsid}`);
      return route();
    }
  } as const;
  return { agent, asked };
}

const exists = () =>
  Response.json({ error: 'SpaceAlreadyExists', message: 'exists' }, { status: 400 });

describe(InviteInbox, () => {
  it('makes the inbox when it is missing, with publicWrite and no members', async () => {
    const { agent, asked } = stubAgent({
      'com.atproto.space.createSpace': () => Response.json({ space: INBOX })
    });

    const inbox = await InviteInbox.open(agent);
    expect([inbox.uri, inbox.created, inbox.fits]).toEqual([INBOX, true, true]);
    expect(asked.map(({ nsid, body }) => [nsid, body])).toEqual([
      [
        'com.atproto.space.createSpace',
        { spaceType: 'fyi.opensocial.invites', skey: 'self', policy: POLICY }
      ]
    ]);
  });

  it('opens an inbox already there as it is', async () => {
    const { agent, asked } = stubAgent({
      'com.atproto.space.createSpace': exists,
      'com.atproto.space.getSpace': () => Response.json({ space: INBOX, policy: POLICY })
    });

    const inbox = await InviteInbox.open(agent);
    expect([inbox.created, inbox.fits]).toEqual([false, true]);
    expect(asked.map(({ nsid }) => nsid)).toEqual([
      'com.atproto.space.createSpace',
      'com.atproto.space.getSpace'
    ]);
    expect(asked[1]?.params.get('space')).toBe(INBOX);
  });

  it('leaves an inbox with another policy alone, and says so', async () => {
    for (const policy of [
      { $type: 'com.atproto.memberspace.policy' },
      { $type: 'com.atproto.memberspace.policy', publicWrite: true, publicRead: true },
      { $type: 'com.atproto.personalspace.policy' }
    ]) {
      const { agent, asked } = stubAgent({
        'com.atproto.space.createSpace': exists,
        'com.atproto.space.getSpace': () => Response.json({ space: INBOX, policy })
      });

      const inbox = await InviteInbox.open(agent);
      expect([inbox.created, inbox.fits]).toEqual([false, false]);
      expect(inbox.policy).toMatchObject(policy);
      expect(asked.map(({ nsid }) => nsid)).not.toContain('com.atproto.space.updateSpace');
    }
  });

  it('passes on any other refusal', async () => {
    const { agent } = stubAgent({
      'com.atproto.space.createSpace': () =>
        Response.json({ error: 'UnsupportedPolicy', message: 'no' }, { status: 400 })
    });

    const opening = InviteInbox.open(agent);
    await expect(opening).rejects.toBeInstanceOf(XrpcResponseError);
    await expect(opening).rejects.toMatchObject({ error: 'UnsupportedPolicy' });
  });

  it('reads the invites groups wrote there', async () => {
    const { agent } = stubAgent({
      'com.atproto.space.createSpace': () => Response.json({ space: INBOX })
    });
    const inbox = await InviteInbox.open(agent);

    const written = [
      {
        rkey: '3m00000000001',
        record: {
          $type: 'fyi.opensocial.invite',
          createdAt: '2026-11-20T00:00:00.000Z',
          message: 'Come skate',
          expiresAt: '2026-12-20T00:00:00.000Z'
        }
      },
      {
        rkey: '3m00000000002',
        record: { $type: 'fyi.opensocial.invite', createdAt: '2026-11-21T00:00:00.000Z' }
      }
    ];
    const records = await Promise.all(
      written.map(async ({ rkey, record }) => ({
        collection: 'fyi.opensocial.invite' as l.NsidString,
        rkey,
        cid: await cidForLex(record),
        record
      }))
    );
    const rev = '3m00000000003' as l.TidString;
    vi.spyOn(inbox.space.client, 'writers').mockImplementation(async function* () {
      yield {
        $type: 'com.atproto.space.defs#repoWrite',
        repo: CLUB as DidString,
        repoRev: rev,
        data: new Uint8Array(),
        spaceRev: rev
      } as SpaceReader.ListedWriter;
    });
    vi.spyOn(inbox.space.client, 'getRepoSnapshot').mockResolvedValue({
      commit: { rev } as unknown as SpaceReader.RepoChanges['commit'],
      head: { rev, state: new Uint8Array() },
      records
    } as Awaited<ReturnType<typeof inbox.space.client.getRepoSnapshot>>);

    await inbox.sync();
    expect(inbox.invites()).toEqual([
      {
        group: CLUB,
        uri: `${INBOX}/${CLUB}/fyi.opensocial.invite/3m00000000001`,
        rkey: '3m00000000001',
        cid: records[0]!.cid.toString(),
        createdAt: '2026-11-20T00:00:00.000Z',
        message: 'Come skate',
        expiresAt: '2026-12-20T00:00:00.000Z'
      },
      {
        group: CLUB,
        uri: `${INBOX}/${CLUB}/fyi.opensocial.invite/3m00000000002`,
        rkey: '3m00000000002',
        cid: records[1]!.cid.toString(),
        createdAt: '2026-11-21T00:00:00.000Z'
      }
    ]);
  });
});
