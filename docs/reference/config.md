# Configuration

The host reads its environment as it starts (`crates/group-host/src/config.rs`),
and refuses to start on a value that does not read, saying which.

| Variable | What it is | Default |
| --- | --- | --- |
| `PORT` | the port it listens on; `0` for any free one | `2600` |
| `PUBLIC_URL` | its origin, an `http` or `https` address with no path | `http://localhost:<PORT>` |
| `SERVICE_DID` | its own DID | `did:web:` of `PUBLIC_URL`'s host, a port as `%3A` |
| `HANDLE_DOMAIN` | the domain groups' handles are made under, with its leading dot | `.test` |
| `DATA_DIR` | where its database (`host.sqlite`), blobs and keys (`host-secrets.json`) are kept | `data` |
| `PLC_URL` | the PLC directory groups' `did:plc`s are published to | `https://plc.directory` |
| `DID_WEB_SCHEME` | how a `did:web`'s document is read (`https`, or `http` for test servers) | `https` |
| `HANDLE_RESOLVERS` | how a handle is resolved, tried in turn: `well-known:<scheme>` for its own `/.well-known/atproto-did`, or a service answering `com.atproto.identity.resolveHandle` (which reads its DNS record) | `well-known:https,https://public.api.bsky.app` |
| `GROUP_FOUNDERS` | who the host starts groups for, comma-separated DIDs | anyone |
| `BLOB_UPLOAD_LIMIT` | the most bytes one blob may be | 52428800 (50 MiB) |
| `CONSOLE_URL` | where the console is: the host sends the browser of an app signing in as a group to its `/authorize`, for a member to approve | `PUBLIC_URL` |
| `PROVISIONING_CLIENTS` | the apps the host makes groups for with `provisionGroup`, comma-separated OAuth client ids; `*` for any | none |
| `PDS_CRAWLERS` | relays told of news with `requestCrawl`, comma-separated: at most every 20 minutes, and the first time after a start | none |
| `RUST_LOG` | what it logs, as `tracing-subscriber`'s filter reads it; lines are JSON | `info` |

It stops on SIGTERM or ctrl-c, answering the requests under way first.

The console reads `GROUP_HOST`, the host it asks (default
`http://127.0.0.1:2600`), and, built (`node build`), what SvelteKit's
Node adapter reads: `ORIGIN`, the address people reach it at, and
`BODY_SIZE_LIMIT`, the most a request it hands on may carry (by default
512K): set it to at least `BLOB_UPLOAD_LIMIT`, or an upload larger than
that is refused before the host sees it.

The `opensocial` command reads its own: `OPENSOCIAL_AS` and
`OPENSOCIAL_PASSWORD` (who it signs in as), `PDS_URL` (where),
`OPENSOCIAL_HOST` and `OPENSOCIAL_HOST_DID` (the host it asks), and
`OPENSOCIAL_SEED_INVITE` (who `seed` invites): see
[the commands](cli.md#opensocial).
