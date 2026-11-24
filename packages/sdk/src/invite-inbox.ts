import {
  type Agent,
  type DidString,
  XrpcResponseError,
  type XrpcResponseBody,
  xrpc
} from '@atproto/lex';
import { SpaceClient } from '@atproto/lex-space';
import { GroupSpace } from './group-space.ts';
import * as com from './lexicons/com.ts';
import * as fyi from './lexicons/fyi.ts';

const { invite, invites } = fyi.opensocial;
const { getSpace } = com.atproto.space;

export declare namespace InviteInbox {
  type Options = Omit<GroupSpace.Options, 'agent'>;
  /** The inbox's policy, as its PDS answers `getSpace`. */
  type Policy = XrpcResponseBody<typeof getSpace.main>['policy'];
  /** An invite a group wrote in the inbox. */
  type Invite = {
    /** The inviting group's DID: the repo the invite is in. */
    group: DidString;
    uri: string;
    rkey: string;
    cid: string;
    createdAt: string;
    message?: string;
    expiresAt?: string;
  };
}

/** The policy an inbox is made with: any group may write, only the owner reads. */
const POLICY = { $type: 'com.atproto.memberspace.policy', publicWrite: true } as const;

/**
 * The signed-in person's invite inbox, `at://<did>/space/fyi.opensocial.invites/self`
 * at their PDS: where groups write `fyi.opensocial.invite` records to them.
 * `open` makes it first if it is missing, with `com.atproto.memberspace.policy`,
 * `publicWrite` set and no members; reading it is `sync()`, then `invites()`.
 *
 * @example
 * ```typescript
 * const inbox = await InviteInbox.open(session)
 * if (!inbox.fits) console.warn('inbox has another policy', inbox.policy)
 * await inbox.sync()
 * for (const invite of inbox.invites()) console.log(invite.group, invite.message)
 * ```
 */
export class InviteInbox {
  /**
   * Makes the inbox of the person of `agent` if they have none, and opens
   * it. An inbox already there is opened as it is, never changed, even with
   * another policy: `fits` says whether its policy is the one an inbox is
   * made with.
   *
   * @throws {XrpcResponseError} if the PDS refuses, e.g. `UnsupportedPolicy`,
   * or the session lacks the `space:fyi.opensocial.invites?manage=create` scope
   */
  static async open(agent: Agent, options: InviteInbox.Options = {}): Promise<InviteInbox> {
    const space = GroupSpace.open(
      invites.main,
      { authority: agent.did as DidString },
      { ...options, agent }
    );
    try {
      await SpaceClient.createSpace(agent, invites.main, { policy: POLICY });
      return new InviteInbox(space, true, POLICY);
    } catch (error) {
      if (!(error instanceof XrpcResponseError) || error.error !== 'SpaceAlreadyExists')
        throw error;
    }
    const ref = space.uri as com.atproto.space.getSpace.$Params['space'];
    const { body } = await xrpc(agent, getSpace, { params: { space: ref } });
    return new InviteInbox(space, false, body.policy);
  }

  constructor(
    /** The inbox, read whole as its owner. */
    readonly space: GroupSpace<typeof invites.main>,
    /** Whether `open` made it. */
    readonly created: boolean,
    /** Its policy. */
    readonly policy: InviteInbox.Policy
  ) {}

  /** `at://<did>/space/fyi.opensocial.invites/self` */
  get uri(): string {
    return this.space.uri;
  }

  /**
   * Whether its policy is an inbox's: `com.atproto.memberspace.policy` with
   * `publicWrite` set and `publicRead` not. One made elsewhere with another is
   * left as it is.
   */
  get fits(): boolean {
    const policy = this.policy as { $type?: string; publicWrite?: boolean; publicRead?: boolean };
    return (
      policy.$type === POLICY.$type && policy.publicWrite === true && policy.publicRead !== true
    );
  }

  /** Brings the invites up to date, as `GroupSpace.sync()`. */
  sync(): Promise<GroupSpace.SyncResult> {
    return this.space.sync();
  }

  /** The invites held now, each from the group that wrote it. */
  invites(): InviteInbox.Invite[] {
    return this.space
      .records({ collections: [invite.main] })
      .map(({ repo, uri, rkey, cid, value }) => {
        const { createdAt, message, expiresAt } = value as fyi.opensocial.invite.Main;
        return {
          group: repo as DidString,
          uri,
          rkey,
          cid,
          createdAt,
          ...(message === undefined ? {} : { message }),
          ...(expiresAt === undefined ? {} : { expiresAt })
        };
      });
  }
}
