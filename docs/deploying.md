# Deploying the host

A group host is a server that other servers on the network must reach. Members' PDSes, relays and apps fetch from it at its own domain, and at every group's handle, over https. It holds the keys that sign each group's identity, and those keys can't be replaced once they're lost. So running it takes more than starting a container. You need a domain and a wildcard name, a proxy that routes by path, an upgrade that never runs two hosts at once, and backups that keep the keys safe.

The steps below set each of these up on one machine with Docker and a reverse proxy. Every variable named here is described in [the configuration reference](reference/config.md). What the host is, and what's still to come, is in [the design overview](design/README.md).

## What runs

A group is an atproto account of its own, with no password and no one person behind it. This host is each group's PDS, the authority for its private spaces, and its OAuth server. Members keep their own accounts on their own PDSes.

One image, `group-host:current`, carries three programs. The host, `group-host`, is a Rust server over SQLite. The console is a SvelteKit app where people start and run groups, built into `/app/build`. The command is `opensocial`. The host and the console run as two containers from that image, behind one domain.

## Building the image

Build from the repository root, and name the commit you're building:

```sh
docker build --build-arg GROUP_HOST_COMMIT=$(git rev-parse HEAD) -t group-host:current .
```

The commit is compiled into the host, so a running host can tell you which build it is. Without the argument, the host reports its commit as `null`. The console is built from `web/` with `lexicons/` beside it, since its types are made from the lexicons.

Details:

- The Rust stage uses `rust:1.98-bookworm`, the toolchain CI tests with, and builds with `--locked` in the `deploy` profile. That's `release` without LTO and with 16 codegen units, so a rebuild is quick and uses every core. The cargo registry and `target/` are cache mounts, so a second build on the same machine rebuilds only the crates that changed.
- The console stage and the final image use `node:24.22.1-bookworm-slim`, the version `.node-version` names. The console stage runs `npm ci` and `npm run build`. adapter-node bundles the console's dependencies, so no `node_modules` is copied.
- The final image adds `ca-certificates` and `tini`, which runs as PID 1. It runs as uid 1000, with `DATA_DIR=/data`, `PORT=2600`, and `group-host` as its default command.

## Running it with compose

`deploy/docker-compose.yml` runs the host and the console. Copy it onto the machine, and put a `.env` beside it. The compose project is named `group-host`.

The two services are `host`, running `group-host` on port 2600, and `console`, running `node /app/build` on port 2601. Neither publishes a port. A reverse proxy in front terminates TLS and reaches them over a docker network they share. That network is external to the compose file, so create it first and attach the proxy to it:

```sh
docker network create group-host
```

On that network the host answers as `group-host` and the console as `group-host-console`. The console reaches the host at `http://host:2600`, over the project's own network.

A `.env` for a host at `group.example.com`, whose groups take handles under `groups.example.com`:

```sh
GROUP_HOST_DOMAIN=group.example.com
GROUP_HOST_HANDLE_DOMAIN=.groups.example.com
GROUP_HOST_CRAWLERS=https://bsky.network
GROUP_HOST_DATA=/var/lib/group-host
```

Compose reads these and passes each to the host under the name the host knows:

| `.env` | Sets | Unset |
| --- | --- | --- |
| `GROUP_HOST_DOMAIN` | host `PUBLIC_URL` as `https://<domain>`, console `ORIGIN` the same | required |
| `GROUP_HOST_HANDLE_DOMAIN` | `HANDLE_DOMAIN`, with its leading dot | required |
| `GROUP_HOST_PLC_URL` | `PLC_URL` | `https://plc.directory` |
| `GROUP_HOST_FOUNDERS` | `GROUP_FOUNDERS` | anyone may start a group |
| `GROUP_HOST_PROVISIONING_CLIENTS` | `PROVISIONING_CLIENTS` | none |
| `GROUP_HOST_CRAWLERS` | `PDS_CRAWLERS` | none |
| `GROUP_HOST_SIGNUP_PDS` | `SIGNUP_PDS` | `https://bsky.social` |
| `GROUP_HOST_BLOB_UPLOAD_LIMIT` | host `BLOB_UPLOAD_LIMIT` and console `BODY_SIZE_LIMIT` | `52428800` |
| `GROUP_HOST_LOG` | `RUST_LOG` | `info` |
| `GROUP_HOST_ADMINS` | `HOST_ADMINS`, the operators who read its logs and stats | none |
| `GROUP_HOST_LOGS_KEEP` | `LOGS_KEEP` | `500000` |
| `GROUP_HOST_DATA` | the directory mounted at `/data` | `/var/lib/group-host` |
| `GROUP_HOST_PROXY_NETWORK` | the proxy's docker network | `group-host` |

The host's own default handle domain, `.test`, isn't for a deployed host, so compose refuses to start without `GROUP_HOST_HANDLE_DOMAIN`. One variable sets both upload limits, so the console never refuses an upload the host would take. An empty list is the same as an unset one.

The data directory must exist and be owned by uid 1000 before the first start. Then:

```sh
sudo install -d -o 1000 -g 1000 /var/lib/group-host
docker compose up -d
```

## Routing through the proxy

The console passes the host's paths on to it: `/console/`, `/xrpc/`, `/oauth/`, `/.well-known/` and `/img/`. It uses `fetch`, which can't carry a websocket. Relays read a PDS's repos through `com.atproto.sync.subscribeRepos`, which is a websocket, so it must reach the host directly. So the proxy splits by path. It sends `/xrpc/*`, `/oauth/*`, `/.well-known/*` and `/img/*` to `group-host:2600`, and everything else to `group-host-console:2601`. The console's pages and `/console/*` work through the console.

Both containers sit behind the one domain. The host takes the console's address to be its own (`CONSOLE_URL` defaults to `PUBLIC_URL`), and this layout makes that true. PDSes and relays fetch the host's DID document at `/.well-known/did.json` and its OAuth metadata from that origin. The host's own DID is `did:web:<domain>` unless `SERVICE_DID` says otherwise.

## Giving groups their handles

A group's handle is its name followed by the handle domain, such as `hikers.groups.example.com`. Anyone resolving the handle asks `https://hikers.groups.example.com/.well-known/atproto-did`. The host answers with the DID of the group that the request's `Host` header names. So every handle must reach the host over https, including those of groups made after the host was set up.

You need a wildcard DNS record for the handle domain, a wildcard certificate (which needs a DNS challenge) or on-demand TLS, and a proxy rule sending those names to the host. A Caddyfile for both domains:

```
group.example.com {
	request_body {
		max_size 60MB
	}
	@host path /xrpc/* /oauth/* /.well-known/* /img/*
	reverse_proxy @host group-host:2600 {
		lb_try_duration 20s
		lb_try_interval 250ms
	}
	reverse_proxy group-host-console:2601 {
		lb_try_duration 20s
		lb_try_interval 250ms
	}
}

*.groups.example.com {
	tls {
		dns <provider> ...
	}
	reverse_proxy group-host:2600
}
```

`lb_try_duration` holds each request through the second or two the host is down while it restarts. The body limit sits above the upload limit. How groups are named and made is in [Groups](design/groups.md).

## Serving sites on groups' own domains

A group that takes a handle on a domain of its own, such as `riders.org`, may move its site there from its settings ([Sites](design/sites.md#on-its-own-domain)). The group points the domain's A and AAAA records, and `www.` under it, at the host's addresses. The proxy can't know those domains ahead of time, so it gets each one's certificate the first time someone asks for it. Before it does, it asks the host at `/tls/ask?domain=<domain>`. The host answers 200 only for a group's own domain, or `www.` under it, while the group is on. So nobody can point DNS at the host and spend its certificate limits. Every other name goes to the host unchanged, and the host serves the site there or answers nothing:

```
{
	on_demand_tls {
		ask http://group-host:2600/tls/ask
	}
}

https:// {
	tls {
		on_demand
	}
	reverse_proxy group-host:2600
}
```

Before it moves a site, the host checks that the domain reaches it by asking `https://<domain>/xrpc/com.atproto.server.describeServer` for its own DID. If your host can't reach its own public addresses from inside, it asks `SITE_CHECK_AT` instead, with the domain as the `Host`. Caddy passes the name a browser asked for as `X-Forwarded-Host`. A site that signs its visitors in names its OAuth client for that name.

## Checking it is up

`GET /xrpc/_health` answers with the host's version and the commit it was built from:

```sh
curl https://group.example.com/xrpc/_health
{"version":"0.1.0","commit":"dc64028..."}
```

As it starts, the host logs a `serving` line with its `public_url`, `did`, `version` and `commit`. The host's lines are JSON. Both containers log through docker's `journald` driver, so their lines stay in the machine's journal after a container is removed:

```sh
docker compose logs -f host
journalctl CONTAINER_NAME=group-host-host-1 --since today
```

The host also keeps its lines in `logs.sqlite`, in its data directory. It keeps the last 500,000 at info and below (`LOGS_KEEP`), and none longer than 30 days. Beside them, in `stats.sqlite`, it counts its requests by day and route, and the MCP tools called. Name the accounts that run it in `GROUP_HOST_ADMINS`. They can then read both from anywhere with `opensocial admin logs` and `opensocial admin stats`, signed in as themselves. Each answer carries `x-trace-id`, and `admin logs --trace` takes it to show every line of that request:

```sh
opensocial admin logs --level warn
opensocial admin logs --trace 0af7651916cd43dd8448eb211c80319c
opensocial admin stats --days 30
```

Compose checks the host at `/xrpc/_health` and the console at `/_app/version.json`, a file the build writes, which the console answers without the host. Each is checked every second for up to 60 seconds while starting, then every 30 seconds.

## Upgrading

The host's SQLite database holds the groups' repos, spaces and members. The host makes its writes take turns within its own process, so two hosts over one data directory would interleave commits to the same repos. So never run the host twice over one data directory. To upgrade, stop the old host, then start the new one:

```sh
docker build --build-arg GROUP_HOST_COMMIT=$(git rev-parse HEAD) -t group-host:current .
docker compose stop host
docker compose up -d host
docker compose up -d console
```

On SIGTERM the host answers the requests already under way, within its 30-second grace period, then exits. The proxy holds new requests until the new host answers. Compose replaces the console the same way. It keeps no state, so a rollout could start a new console beside the old one instead. Neither service sets `container_name`, so that stays possible. When it's stopped, the console finishes its requests within a 35-second grace period.

## Deploying on each merge

The host at next.opensocial.fyi deploys itself: `.knotless/workflows/deploy.yml` runs on each push to `main`. A pull is merged into `main` once its own checks (`check`, `test`, `web`, `lexicons`, `e2e` and `docs`) have passed on it merged with `main`, and the review bot has approved it. So the deploy doesn't wait for `main`'s own checks, since a workflow's step can't read a private repository's checks. The merge queue, which would check each batch as it lands, is off while knotless can't land a private repository's queue. The workflow sends a `git archive` of the commit to the machine over SSH, which builds it as `group-host:build-<commit>`. That image then becomes `group-host:current`, and the image it replaces is kept as `group-host:before-<commit>`. Compose brings the host and the console up from the compose file at that commit, as the upgrade above does. The deploy passes once `/xrpc/_health` names the commit, on the machine and then through the proxy, within a minute.

One deploy runs at a time, in the order `main` moved, and a newer commit's deploy waits for the one before it to finish instead of cutting it off.

The workflow runs only on `ci.knotless.dev`, the runner that runs only its owner's repositories. Every other workflow names `runner.knotless.dev`. One secret is sealed to `ci.knotless.dev` for `refs/heads/main`. It's `GROUP_HOST_DEPLOY_KEY`, an SSH key the machine accepts only to run its deploy command. That command can build, bring up or throw away a group-host image and can't do anything else. The deploy command, how its key is installed, and how to roll back are in the machine's own configuration (hetzner-infra's `stacks/opensocial`).

## Backing up

The data directory holds everything the host knows. There's `host.sqlite`, the database of accounts, repos, spaces, members, tokens and the index. There are the blobs, and there's `host-secrets.json`, which holds the host's keys. `logs.sqlite` and `stats.sqlite`, its logs and counts, need no backing up.

The host writes `host-secrets.json` the first time it starts, and it isn't set in config. Among its keys is the rotation key the host signs every group's PLC operations with. If it's lost, a group whose stewards hold no recovery key of their own can never change its identity again. [Recovery](design/recovery.md) explains what stewards can hold.

The database runs in WAL mode, so copying the file while the host runs can give a broken copy. Take a snapshot with SQLite instead:

```sh
sqlite3 /var/lib/group-host/host.sqlite "VACUUM INTO '/backup/host-$(date +%F).sqlite'"
```

Copy the snapshot and the blobs off the machine on a schedule. Keep `host-secrets.json` apart from them, somewhere fewer people and systems can read, since anyone holding it can change every group's identity. Then restore a backup onto a spare machine to know it works. Scheduled backups aren't in the repository yet.

## Running the command

The image carries `opensocial`, so the command runs in the host's container, or in one of its own:

```sh
docker compose exec host opensocial --help
docker run --rm group-host:current opensocial --help
```

When the host's address changes, `group-host move-host` points every group's DID document at the new one. It runs once, over the data directory, with the host stopped and `.env` already naming the new domain. Run it first with `--dry-run` to see what it would change:

```sh
docker compose stop host
docker compose run --rm host group-host move-host --from https://old.example.com --dry-run
docker compose run --rm host group-host move-host --from https://old.example.com
docker compose up -d host
```

[Moving](design/moving.md) explains what a move changes. Every command is listed in [the command reference](reference/cli.md), and every method in [the API reference](reference/api.md). Building and testing the host are in [Development](development.md).
