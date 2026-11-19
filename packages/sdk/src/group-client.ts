import {
  type ActionOptions,
  type Agent,
  type AtIdentifierString,
  Client,
  type DidString,
  type HandleString,
  type InferMethodInputBody,
  type RecordKeyValue,
  type Service,
  type Space,
  type XrpcRequestParams,
  type XrpcResponseBody
} from '@atproto/lex';
import { GroupSpace } from './group-space.ts';
import * as com from './lexicons/com.ts';
import * as fyi from './lexicons/fyi.ts';

const {
  createReport,
  getActorGroups,
  getActorInvites,
  getGroup,
  leaveGroup,
  listMembers,
  requestJoin
} = fyi.opensocial;
const { resolveHandle } = com.atproto.identity;

export declare namespace GroupClient {
  type Options = {
    /**
     * The group host, as the member's PDS proxies to it: its
     * `did:web:…#opensocial`.
     */
    service: Service;
  };
  type GetGroupOutput = XrpcResponseBody<typeof getGroup.main>;
  type Member = XrpcResponseBody<typeof listMembers.main>['members'][number];
  type MembersOptions = Omit<XrpcRequestParams<typeof listMembers.main>, 'group' | 'cursor'> &
    ActionOptions;
  type JoinInput = Omit<InferMethodInputBody<typeof requestJoin.main>, 'group'>;
  type JoinOutput = XrpcResponseBody<typeof requestJoin.main>;
  type ReportInput = Omit<InferMethodInputBody<typeof createReport.main>, 'group'>;
  type ReportOutput = XrpcResponseBody<typeof createReport.main>;
  type SpaceOptions<S extends Space> = Omit<GroupSpace.Options, 'agent'> & {
    skey?: RecordKeyValue<S['key']>;
  };
}

/**
 * A group on a group host, for the member of `agent` (e.g. their OAuth
 * session): the host's own methods, asked through the member's PDS, and the
 * group's spaces, read and written as the member.
 *
 * @example
 * ```typescript
 * const group = await GroupClient.open(session, 'bookclub.groups.example', {
 *   service: 'did:web:groups.example#opensocial'
 * })
 * for await (const member of group.members()) console.log(member.handle)
 *
 * const room = group.space(com.example.forum.room.main, { skey: 'general' })
 * await room.sync()
 * room.records({ collections: [com.example.forum.post] })
 * ```
 */
export class GroupClient {
  /**
   * Opens a group by its DID or handle, asking the host for it (a handle is
   * first resolved by the member's PDS).
   *
   * @throws {XrpcResponseError} with `GroupNotFound` if the host has no such
   * group
   */
  static async open(
    agent: Agent,
    group: AtIdentifierString,
    { service }: GroupClient.Options
  ): Promise<GroupClient> {
    const host = new Client(agent, { service });
    const did = group.startsWith('did:')
      ? (group as DidString)
      : (
          await new Client(agent).call(resolveHandle, {
            handle: group as HandleString
          })
        ).did;
    const { group: view } = await host.call(getGroup, { group: did });
    return new GroupClient(agent, host, view.did, view.handle);
  }

  constructor(
    /** The member's agent. */
    readonly agent: Agent,
    /** A lex `Client` asking the group host, through the member's PDS. */
    readonly client: Client,
    readonly did: DidString,
    readonly handle: HandleString
  ) {}

  /** The group as the host shows it to the member (`getGroup`). */
  getGroup(options?: ActionOptions): Promise<GroupClient.GetGroupOutput> {
    return this.client.call(getGroup, { group: this.did }, options);
  }

  /** Iterates over the group's members (`listMembers`), every page. */
  async *members({ signal, ...params }: GroupClient.MembersOptions = {}): AsyncGenerator<
    GroupClient.Member,
    void,
    undefined
  > {
    let cursor: string | undefined;
    do {
      const page = await this.client.call(
        listMembers,
        { ...params, group: this.did, cursor },
        { signal }
      );
      yield* page.members;
      // A host answering the same cursor again has no more.
      if (page.cursor === cursor) return;
      cursor = page.cursor;
    } while (cursor);
  }

  /**
   * Asks to join the group (`requestJoin`); with an invite addressed to the
   * member, joins at once.
   */
  join(
    input: GroupClient.JoinInput = {},
    options?: ActionOptions
  ): Promise<GroupClient.JoinOutput> {
    return this.client.call(requestJoin, { ...input, group: this.did }, options);
  }

  /** Leaves the group (`leaveGroup`). */
  async leave(options?: ActionOptions): Promise<void> {
    await this.client.call(leaveGroup, { group: this.did }, options);
  }

  /** Reports an account or a record to the group's moderators (`createReport`). */
  report(
    input: GroupClient.ReportInput,
    options?: ActionOptions
  ): Promise<GroupClient.ReportOutput> {
    return this.client.call(createReport, { ...input, group: this.did }, options);
  }

  /**
   * A space of the group's, of the type `schema` describes, read and written
   * as the member. No request is made until it is used. `skey` defaults to the
   * type's literal key, or else to `self`.
   */
  space<const S extends Space>(
    schema: S,
    { skey, ...options }: GroupClient.SpaceOptions<S> = {}
  ): GroupSpace<S> {
    return GroupSpace.open(
      schema,
      { authority: this.did, skey },
      { ...options, agent: this.agent }
    );
  }
}

/** Lists the groups an account is in (`getActorGroups`), one page. */
export function listActorGroups(
  client: Client,
  params: XrpcRequestParams<typeof getActorGroups.main>,
  options?: ActionOptions
): Promise<XrpcResponseBody<typeof getActorGroups.main>> {
  return client.call(getActorGroups, params, options);
}

/** Lists the invitations addressed to the member (`getActorInvites`). */
export function listActorInvites(
  client: Client,
  params: XrpcRequestParams<typeof getActorInvites.main> = {},
  options?: ActionOptions
): Promise<XrpcResponseBody<typeof getActorInvites.main>> {
  return client.call(getActorInvites, params, options);
}
