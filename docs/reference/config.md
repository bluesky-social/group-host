# Configuration

The host reads its environment as it starts (`crates/group-host/src/config.rs`),
and refuses to start on a value that does not read, saying which.

| Variable | What it is | Default |
| --- | --- | --- |
| `PORT` | the port it listens on; `0` for any free one | `2600` |
| `PUBLIC_URL` | its origin, an `http` or `https` address with no path | `http://localhost:<PORT>` |
| `SERVICE_DID` | its own DID | `did:web:` of `PUBLIC_URL`'s host, a port as `%3A` |
| `HANDLE_DOMAIN` | the domain groups' handles are made under, with its leading dot | `.test` |
| `DATA_DIR` | where its database, blobs and keys are kept | `data` |
| `RUST_LOG` | what it logs, as `tracing-subscriber`'s filter reads it; lines are JSON | `info` |

It stops on SIGTERM or ctrl-c, answering the requests under way first.

The console reads one: `GROUP_HOST`, the host it asks (default
`http://127.0.0.1:2600`).
