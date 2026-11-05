# Configuration

The host reads its environment as it starts (`crates/group-host/src/config.rs`).
It refuses to start, saying which, on a `PORT` that is no port, a
`PUBLIC_URL` that is no `http` or `https` origin, a `HANDLE_DOMAIN` with
no leading dot, a `DID_WEB_SCHEME` other than `https` or `http`, a
`BLOB_UPLOAD_LIMIT` that is no number, or a `CONSOLE_URL` that is no URL.
The others are taken as given. In a comma-separated list, each item is
trimmed of spaces and a trailing `/`.

| Variable | What it is | Default |
| --- | --- | --- |
| `PORT` | the port it listens on; `0` for any free one | `2600` |
| `PUBLIC_URL` | its origin, an `http` or `https` address with no path | `http://localhost:<PORT>` |
| `SERVICE_DID` | its own DID | `did:web:` of `PUBLIC_URL`'s host, a port as `%3A` |
| `HANDLE_DOMAIN` | the domain groups' handles are made under, with its leading dot | `.test` |
| `DATA_DIR` | where its database (`host.sqlite`), blobs and keys (`host-secrets.json`) are kept | `data` |
| `PLC_URL` | the PLC directory groups' `did:plc`s are published to | `https://plc.directory` |
| `DID_WEB_SCHEME` | how a `did:web`'s document is read (`https`, or `http` for test servers) | `https` |
| `HANDLE_RESOLVERS` | how a handle is resolved, tried in turn: `well-known:<scheme>` for its own `/.well-known/atproto-did`, or a service answering `com.atproto.identity.resolveHandle` (which reads its DNS record); a trailing `/` is dropped | `well-known:https,https://public.api.bsky.app` |
| `GROUP_FOUNDERS` | who the host starts groups for, comma-separated DIDs: by `createGroup`, `provisionGroup`, or moving a group or an account here | anyone |
| `BLOB_UPLOAD_LIMIT` | the most bytes one blob may be, uploaded, or brought with a group, an account or a space; `describeServer` answers it | 52428800 (50 MiB) |
| `CONSOLE_URL` | where the console is: where the host sends a member to approve an app or an MCP client (`/authorize`), the console's OAuth client id and callback when people sign in (`/console/…`), and, when `https`, whether the session cookie is `Secure` | `PUBLIC_URL` |
| `PROVISIONING_CLIENTS` | the apps the host makes groups for with `provisionGroup`, comma-separated OAuth client ids, each matched as written, a trailing `/` included; `*` for any | none |
| `ACTOR_SEARCH` | where `searchActors` looks for a handle being typed: `plc`, the handles in `PLC_URL`'s own export, read a page at a time and kept a minute; or the URL of a service answering `tech.waow.typeahead.searchActors` | typeahead.waow.tech's, when `PLC_URL` is `https://plc.directory`; else `plc` |
| `SIGNUP_PDS` | the PDS where someone with no account makes one, from the console's "Create one": its authorization server's sign-up (`prompt=create`); a trailing `/` dropped | `https://bsky.social` |
| `PDS_CRAWLERS` | relays told of news with `requestCrawl`, comma-separated, a trailing `/` dropped: at most every 20 minutes, and the first time after a start | none |
| `RUST_LOG` | what it logs, as `tracing-subscriber`'s filter reads it; lines are JSON | `info` |

It stops on SIGTERM or ctrl-c, answering the requests under way first.

`group-host move-host` reads the same environment, which then names the
new address.

The console reads `GROUP_HOST`, the host it asks (default
`http://127.0.0.1:2600`), and, built (`node build`), what SvelteKit's
Node adapter reads: `ORIGIN`, the address people reach it at, and
`BODY_SIZE_LIMIT`, the most a request it hands on may carry (by default
512K): set it to at least `BLOB_UPLOAD_LIMIT`, or an upload larger than
that is refused before the host sees it.

The `opensocial` command reads its own: `OPENSOCIAL_AS` and
`OPENSOCIAL_PASSWORD` (who it signs in as), `PDS_URL` (where),
`OPENSOCIAL_HOST` and `OPENSOCIAL_HOST_DID` (the host it asks),
`OPENSOCIAL_SEED_INVITE` (who `group seed` invites), and
`OPENSOCIAL_IMPORT_PASSWORD` (the account `account import` moves): see
[the commands](cli.md#opensocial).
