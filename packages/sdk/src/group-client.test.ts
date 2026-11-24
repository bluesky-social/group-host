import { Client, XrpcResponseError } from '@atproto/lex';
import { describe, expect, it } from 'vitest';
import { GroupClient, listActorGroups, listActorInvites } from './group-client.ts';

const GROUP = 'did:plc:group';
const ALICE = 'did:plc:alice';
const SERVICE = 'did:web:groups.example#opensocial';

type Asked = {
  nsid: string;
  proxy: string | null;
  params: URLSearchParams;
  body: unknown;
};

/**
 * Alice's agent, its PDS answering each method with `routes[nsid]`, as many
 * times as listed; it records what was asked.
 */
function stubAgent(routes: Record<string, ((asked: Asked) => Response)[]>) {
  const asked: Asked[] = [];
  const agent = {
    did: ALICE,
    fetchHandler: async (path: string, init: RequestInit) => {
      const url = new URL(path, 'https://pds.example');
      const nsid = url.pathname.replace('/xrpc/', '');
      const body = typeof init.body === 'string' ? JSON.parse(init.body) : undefined;
      const proxy = new Headers(init.headers).get('atproto-proxy');
      const request = { nsid, proxy, params: url.searchParams, body };
      asked.push(request);
      const route = routes[nsid]?.shift();
      if (!route) throw new Error(`Unexpected request for ${nsid}`);
      return route(request);
    }
  } as const;
  return { agent, asked };
}

const group = () => Response.json({ group: { did: GROUP, handle: 'club.groups.example' } });
const member = (did: string) => ({ actor: { did }, roles: [] });

describe(GroupClient, () => {
  it('opens a group by handle, asking the host through the member PDS', async () => {
    const { agent, asked } = stubAgent({
      'com.atproto.identity.resolveHandle': [() => Response.json({ did: GROUP })],
      'fyi.opensocial.getGroup': [group]
    });

    const club = await GroupClient.open(agent, 'club.groups.example', {
      service: SERVICE
    });
    expect([club.did, club.handle]).toEqual([GROUP, 'club.groups.example']);
    expect(asked.map(({ nsid, proxy }) => [nsid, proxy])).toEqual([
      ['com.atproto.identity.resolveHandle', null],
      ['fyi.opensocial.getGroup', SERVICE]
    ]);
    expect(asked[1]?.params.get('group')).toBe(GROUP);
  });

  it('surfaces the host refusing as an XrpcResponseError by name', async () => {
    const { agent } = stubAgent({
      'fyi.opensocial.getGroup': [
        () => Response.json({ error: 'GroupNotFound', message: 'No such group' }, { status: 400 })
      ]
    });

    const opening = GroupClient.open(agent, GROUP, { service: SERVICE });
    await expect(opening).rejects.toBeInstanceOf(XrpcResponseError);
    await expect(opening).rejects.toMatchObject({ error: 'GroupNotFound' });
  });

  it('lists every page of members', async () => {
    const { agent, asked } = stubAgent({
      'fyi.opensocial.getGroup': [group],
      'fyi.opensocial.listMembers': [
        () => Response.json({ members: [member(ALICE)], cursor: 'page2' }),
        () => Response.json({ members: [member('did:plc:bob')] })
      ]
    });
    const club = await GroupClient.open(agent, GROUP, { service: SERVICE });

    const members = [];
    for await (const m of club.members({ limit: 1 })) members.push(m.actor.did);
    expect(members).toEqual([ALICE, 'did:plc:bob']);
    expect(asked.slice(1).map(({ params }) => [params.get('cursor'), params.get('limit')])).toEqual(
      [
        [null, '1'],
        ['page2', '1']
      ]
    );
  });

  it('joins, leaves and reports in the group', async () => {
    const { agent, asked } = stubAgent({
      'fyi.opensocial.getGroup': [group],
      'fyi.opensocial.requestJoin': [() => Response.json({ status: 'pending' })],
      'fyi.opensocial.leaveGroup': [() => Response.json({})],
      'fyi.opensocial.createReport': [
        () => Response.json({ id: '1', createdAt: '2026-11-18T00:00:00.000Z' })
      ]
    });
    const club = await GroupClient.open(agent, GROUP, { service: SERVICE });

    await club.join({ message: 'Hello' });
    await club.leave();
    await club.report({ subject: 'did:plc:mallory', reasonType: 'spam' });
    expect(asked.slice(1).map(({ nsid, proxy, body }) => [nsid, proxy, body])).toEqual([
      ['fyi.opensocial.requestJoin', SERVICE, { group: GROUP, message: 'Hello' }],
      ['fyi.opensocial.leaveGroup', SERVICE, { group: GROUP }],
      [
        'fyi.opensocial.createReport',
        SERVICE,
        { group: GROUP, subject: 'did:plc:mallory', reasonType: 'spam' }
      ]
    ]);
  });

  it("lists an account's groups and invites", async () => {
    const { agent, asked } = stubAgent({
      'fyi.opensocial.getActorGroups': [() => Response.json({ groups: [] })],
      'fyi.opensocial.getActorInvites': [() => Response.json({ invites: [] })]
    });
    const client = new Client(agent, { service: SERVICE });

    expect(await listActorGroups(client, { actor: ALICE })).toEqual({
      groups: []
    });
    expect(await listActorInvites(client)).toEqual({ invites: [] });
    expect(asked.map(({ nsid, proxy }) => [nsid, proxy])).toEqual([
      ['fyi.opensocial.getActorGroups', SERVICE],
      ['fyi.opensocial.getActorInvites', SERVICE]
    ]);
  });
});
