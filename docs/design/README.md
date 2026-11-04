# Design

The group host is one server. It is the PDS for every group it hosts, the
authority for their private spaces, their OAuth server, and the web server
for the console. Members keep their own accounts on their own PDSes,
anywhere on the network. The prototype's `docs/design/README.md`
(`~/code/opensocial`) explains why a group needs each of those.

## From the prototype

opensocial.fyi is a prototype, built in TypeScript on `@atproto/*`
packages (an embedded reference PDS and the space alpha's libraries among
them), Express and React. It found what a group host has to do. This host
does what the prototype's core does, built as knotless is: Rust for the
server, with the atproto it needs written for it and held to a 100%
coverage gate; SvelteKit for the console, its types made from the
lexicons; the lexicons checked against the style guide.

It is not a one-to-one port. Each part keeps what the prototype got right
and changes what it did not, and its methods and records take the shape
that is right here; the pull that builds a part says what changed. The
lexicons start as the prototype's core and are this host's to change until
they are published from here.

## The pieces

| Crate or directory | What it is |
| --- | --- |
| `crates/atproto` | DRISL and CIDs, keys, the MST, CAR files, PLC operations, service auth, JOSE |
| `crates/group-host` | the server: accounts and repos, spaces, membership and roles, OAuth, moderation, and the console's queries |
| `crates/group-host-mcp` | the core's methods as MCP tools, asked as the member who connected it |
| `crates/opensocial` | a client for group hosts, and the `opensocial` command |
| `web/` | the console, where people start and run groups and approve apps |
| `lexicons/` | the schemas, starting from the prototype's core |

Only the prototype's core comes over. What its modules bring (the sites
at each group's address, templates, drafts, the network index,
notifications) is not part of this host; if something like them comes
later, it comes as crates beside `group-host`, following it rather than
called by it.

## The order

Each step is a pull, or a few, with its tests first. A step names the
prototype's code it learns from, under `packages/group-host/src` unless it
says otherwise.

1. **Scaffold.** The workspace, the console's shell, the lexicons, the
   checks. The host answers `_health` and refuses every other method as
   not implemented.
2. **atproto primitives.** What `@atproto/repo`, `@atproto/crypto`,
   `@atproto/identity` and `@did-plc/lib` give it, checked against
   atproto's interop test vectors (`crates/atproto/tests/interop`):
   - the data model: DRISL and CIDs, TIDs, the identifiers' syntax, and
     secp256k1 and P-256 keys as multikeys and `did:key`s (done);
   - the repo: the MST, CAR files, signed commits, and the firehose's
     commit proofs (done);
   - identity: PLC operations, DID documents, service-auth JWTs (done),
     checked against @did-plc/lib and @atproto/xrpc-server
     (`crates/atproto/tests/reference`) and against knotless's own.
3. **Accounts and repos.** The SQLite store, group accounts and their
   keys, `did:plc` creation, records and commits, `sync.*` and `did:web`
   for the host (done); the firehose (`subscribeRepos`, sync 1.1's
   commits with their proofs) and crawlers (done); blobs, kept on disk
   until no record references them (done):
   `pds/*`, `db/*`, `didWeb.ts`, `actors.ts`.
4. **Groups and spaces.** First the space alpha's primitives in
   `atproto` (`space.rs`: the LtHash set hash, space commits and record
   MACs, the KEM a MAC's secret is agreed by, STAR archives; done), then
   its tokens and signed requests (`space_auth.rs`: delegation tokens,
   space credentials, client attestations, HTTP message signatures; done),
   then the repos the host keeps in spaces, each group with its
   `#atproto_kem` key, read with an authority's credential and each write
   told to the authority (`space.rs`, `space_api.rs`; done, see
   [spaces](spaces.md)), others' DID documents read for a `did:plc` or a
   `did:web` and kept a few minutes (`identity.rs`; done), then being the
   authority for its groups' spaces, with an outbox that tries a
   `notifyWrite` again for a day (`authority.rs`, `authority_api.rs`;
   done).
   Then the
   groups: `createGroup`, `getGroup`, `listGroups`, with their meta and
   members spaces, access records and roles (`group.rs`, `group_api.rs`,
   `rbac.rs`; done, see [groups](groups.md)), their profile, images and
   rules (`group_config.rs`; done), switching them off and on and
   deleting them (`lifecycle.rs`, `group_lifecycle.rs`; done), handles
   on their own domains (`group_handle.rs`; done), spaces made and
   configured by members (`group_spaces.rs`; done), and the space index
   (`index.rs`, `index_api.rs`; done, see [spaces](spaces.md)):
   `group.ts`, `bootstrap.ts`, `methods/{groups,handle,profile,rules,lifecycle,spaces,spaceRecords}.ts`,
   `spaceIndex.ts`. (`checkUserAccess`, for invite inboxes, comes with
   invites.)
5. **Membership and roles.** Roles and permissions, assigning, leaving
   and ejecting, with credentials revoked for whoever is out
   (`group_members.rs`; done, see [membership](membership.md)); joining
   and join requests (`group_join.rs`; done); invites, with
   `checkUserAccess` for their inboxes (`group_invites.rs`; done): `rbac.ts`, `methods/{roles,membership,join,invites}.ts`.
6. **Signing in.** Service auth for callers; the OAuth server an app signs
   in to as a group (PAR, DPoP, consent, refresh), with its JWKs and JWTs
   in `atproto::jose` (`oauth*.rs`, `authorization.rs`; done, see
   [signing in](signing-in.md)); writes as the group with its tokens,
   within their scope and the member's roles (`oauth_resource.rs`,
   `scopes.rs`, `token_writes.rs`; done); group tokens, and the grants
   that act as a group with their log (`group_tokens.rs`; done); app
   passwords and their sessions (`app_passwords.rs`; done); and
   provisioning (`provision.rs`; done): `auth.ts`, `pds/auth.ts`, `pds/oauth/*`,
   `methods/{authorization,groupAuth,appPasswords,connectedApps,provision}.ts`.
   The console signs its people in at their own PDS here, as a
   confidential client, as knotless does, keeping only a signed cookie
   (`console_login.rs`; done).
7. **The console.** Signing in as oneself (step 6); who is looking
   (`getViewer`, as knotless's) and their groups (`getActorGroups`), on
   home, and approving an app (`/authorize`; `actors.rs`; done);
   starting a group (`/new`) and a group's page (`/g/<handle>`: its
   profile and rules, joining and leaving, its members, and admitting
   those asking; done); its members run from its page (roles given and
   taken, people put out, invites by handle, and invitations taken up or
   turned down on home; done); and its settings (`/g/<handle>/settings`:
   profile, rules, roles, the apps acting as it with their log, and app
   passwords; done): the console's pages in
   `apps/web/src/pages`. Each is used end to end in `harness/e2e`.
8. **Moderation, recovery and moving.** Reports, labels, subjects and
   history; recovery keys; importing an account; migrating a group to
   another host and moving the host itself:
   `methods/{moderation,recovery,importAccount,migration}.ts`,
   `hostLabels.ts`, `scripts/move-host.ts`.
9. **MCP and the command.** The MCP tools over what steps 4 to 8 brought,
   and the `opensocial` command's `create-group`, `inspect`, `membership`,
   `migrate`, `move-host` and seeds, as `packages/cli` has them.

The MCP tools and the command's subcommands may come in with each step
instead, beside the methods they call, where that is the smaller pull.
