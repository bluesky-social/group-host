# Configuration

The host reads its environment when it starts (`crates/group-host/src/config.rs`).
It refuses to start, and says which variable is wrong, on a `PORT` that
isn't a port, a `PUBLIC_URL` that isn't an `http` or `https` origin, a
`HANDLE_DOMAIN` without a leading dot, a `DID_WEB_SCHEME` other than
`https` or `http`, a `BLOB_UPLOAD_LIMIT` or `LOGS_KEEP` that isn't a
number, or a `CONSOLE_URL` that isn't a URL.
It takes the others as given. In a comma-separated list, it trims each
item of spaces and a trailing `/`.

| Variable | What it is | Default |
| --- | --- | --- |
| `PORT` | the port it listens on, or `0` for any free one | `2600` |
| `PUBLIC_URL` | its origin, an `http` or `https` address with no path | `http://localhost:<PORT>` |
| `SERVICE_DID` | its own DID | `did:web:` of `PUBLIC_URL`'s host, a port as `%3A` |
| `HANDLE_DOMAIN` | the domain it makes groups' handles under, with its leading dot | `.test` |
| `DATA_DIR` | where it keeps its database (`host.sqlite`), blobs and keys (`host-secrets.json`), the lines it logged (`logs.sqlite`) and its usage counts (`stats.sqlite`) | `data` |
| `PLC_URL` | the PLC directory it publishes groups' `did:plc`s to | `https://plc.directory` |
| `DID_WEB_SCHEME` | how it reads a `did:web`'s document (`https`, or `http` for test servers) | `https` |
| `HANDLE_RESOLVERS` | how it resolves a handle, trying each in turn: `well-known:<scheme>` for the handle's own `/.well-known/atproto-did`, or a service answering `com.atproto.identity.resolveHandle` (which reads its DNS record). It drops a trailing `/` | `well-known:https,https://public.api.bsky.app` |
| `GROUP_FOUNDERS` | who the host starts groups for, as comma-separated DIDs, by `createGroup`, `provisionGroup`, or moving a group or an account here | anyone |
| `BLOB_UPLOAD_LIMIT` | the most bytes one blob may be, whether uploaded or brought with a group, an account or a space. `describeServer` answers it | 52428800 (50 MiB) |
| `CONSOLE_URL` | where the console is. The host sends a member there to approve an app or an MCP client (`/authorize`). It's the console's OAuth client id and callback when people sign in (`/console/…`) and the one `Origin` the host takes its session cookie from. When it's `https`, the cookie is `Secure` and named `__Host-` | `PUBLIC_URL` |
| `FAVICONS` | where the host asks for a link's site icon when an MCP client writes a new link to a group's profile. The link's address follows, encoded, and the service answers a PNG. Empty asks nowhere | `https://cardyb.bsky.app/v1/favicon?url=` |
| `PROVISIONING_CLIENTS` | the apps the host makes groups for with `provisionGroup`, as comma-separated OAuth client ids. It matches each as written, a trailing `/` included. `*` allows any | none |
| `ACTOR_SEARCH` | where `searchActors` looks up a handle as someone types it. `plc` reads the handles in `PLC_URL`'s own export a page at a time and keeps them a minute. Otherwise it's the URL of a service answering `tech.waow.typeahead.searchActors` | typeahead.waow.tech's when `PLC_URL` is `https://plc.directory`, else `plc` |
| `SIGNUP_PDS` | the PDS where someone without an account makes one, from the console's "Create one", through its authorization server's sign-up (`prompt=create`). It drops a trailing `/` | `https://bsky.social` |
| `SITE_SHELL_DIR` | where the built page around a site that names `reads` lives, with its `shell.js` (`web/build-shell`, which `npm run build` in `web/` makes, and the image sets `/app/build-shell`) | none, so such a site's pages load no script |
| `SITE_CHECK_AT` | where the host checks that a group's own domain reaches this host before it serves the group's site there. It asks this URL in the domain's place, with the domain as its `Host`. Use it when a host can't reach its own public addresses | the domain itself, over https |
| `PDS_CRAWLERS` | relays the host tells of news with `requestCrawl`, comma-separated, a trailing `/` dropped. It tells them at most every 20 minutes, and the first time after a start | none |
| `RUST_LOG` | what it logs, as `tracing-subscriber`'s filter reads it. Lines are JSON, written to stdout and kept in `logs.sqlite` for its operators | `info` |
| `HOST_ADMINS` | its operators, as comma-separated DIDs. These accounts may read its logs (`getLogs`) and how it's used (`getStats`) | none |
| `LOGS_KEEP` | how many lines logged at info and below it keeps to read back. It keeps a warning or an error the full 30 days, which is the most it keeps any line | 500000 |

It stops on SIGTERM or ctrl-c, and finishes the requests under way
first.

`group-host move-host` reads the same environment, which then names the
new address.

The console reads `GROUP_HOST`, the host it asks (default
`http://127.0.0.1:2600`). It reads `FAVICONS`, as the host does, for a
link added in its settings. Once built (`node build`), it also reads
what SvelteKit's Node adapter reads. That's `ORIGIN`, the address people
reach it at, and `BODY_SIZE_LIMIT`, the most a request it hands on may
carry (512K by default). Set `BODY_SIZE_LIMIT` to at least
`BLOB_UPLOAD_LIMIT`, or the console refuses a larger upload before the
host sees it.

The `opensocial` command reads its own: `OPENSOCIAL_AS` and
`OPENSOCIAL_PASSWORD` (who it signs in as), `PDS_URL` (where),
`OPENSOCIAL_HOST` and `OPENSOCIAL_HOST_DID` (the host it asks),
`OPENSOCIAL_SEED_INVITE` (who `group seed` invites),
`OPENSOCIAL_IMPORT_PASSWORD` (the account `account import` moves), and
`FAVICONS`, as the host does, for a link `profile set` writes. See
[the commands](cli.md#opensocial).
