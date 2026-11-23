# Limits

A group host answers anyone on the network, and some requests cost it
far more than they cost the caller. An app's pushed sign-in makes the
host fetch the app's metadata from wherever it says. A new MCP client is
a row the host keeps for a day. A handle change is an operation in the
PLC directory, which limits the host in turn. And every group on a host
shares its one database, so one caller's flood slows everyone. So the
host limits how much one caller may ask, and how often.

## Windows and points

A limit is a number of points a caller may spend in a window of time.
The window starts at the caller's first request and ends a fixed time
later (five minutes, an hour or a day). Then the next request starts a
new one. Most requests spend one point. A write as a group spends three
for a create, two for an update and one for a delete, so a burst of
deletes costs less than a burst of new records.

A request past a limit gets a 429, `RateLimitExceeded`, and spends
nothing. Its `Retry-After` header says how many seconds until the window
ends, and `RateLimit-Limit`, `RateLimit-Remaining`, `RateLimit-Reset`
and `RateLimit-Policy` say which limit it hit. A browser app on any
origin can read them.

The host keeps its windows in memory. Only one host ever runs over a
data directory ([deploying](../deploying.md)), so the counts are exact
without a shared store. A restart starts every window over, which is
cheap to accept since a restart takes a second or two.

`RATE_LIMITS=off` turns every limit off ([config](../reference/config.md)).
That's for a test network, where one address asks for every browser and
command on it. The dev world and the e2e checks run that way.

## Who a caller is

A limit counts by one of three things: the address a request came from,
the group a token writes as, or the person signed in.

The address is the one the connection came from, unless that's this
machine or its private network (loopback, `10/8`, `172.16/12`,
`192.168/16`, link-local, or IPv6 unique-local). A proxy in front of the
host sits there, so the host reads `X-Forwarded-For` from the right and
takes the first address that isn't one of those. Caddy, as
[deploying](../deploying.md) sets it up, replaces whatever a client sent
with the address it saw, so a client can't name someone else. The
console passes that header on with each method it asks for a page. So a
person browsing the console is counted as themselves, not as the
console.

An IPv6 address counts with the rest of its /64. A single machine often
holds a whole /64, so counting each address alone would let it spread a
flood over billions of them.

A method the host calls inside itself, as an MCP tool does, has no
address and spends nothing by address. It still counts against its
group's and its person's limits.

## The limits

| What | Points | Window | Counted by |
| --- | --- | --- | --- |
| every request, but those below that have their own | 3000 | 5 minutes | address |
| `com.atproto.sync.getRepo` | 6000 | 5 minutes | address |
| `createGroup`, `provisionGroup` | 100 | 5 minutes | address |
| `/oauth/par` | 300 | 5 minutes | address |
| `/mcp/register` | 30 | 5 minutes | address |
| `/console/login`, `/console/move/login`, together | 30 | 5 minutes | address |
| `searchActors` | 600 | 5 minutes | address |
| `com.atproto.server.createSession` | 30, and 300 | 5 minutes, and a day | handle and address |
| writes as a group, `com.atproto.repo.*` and `com.atproto.space.*` | 5000, and 35000 | an hour, and a day | group |
| `com.atproto.repo.uploadBlob` | 1000 | a day | group |
| `updateGroupHandle` | 10, and 50 | 5 minutes, and a day | group |
| `createReport` | 10, and 100 | 5 minutes, and a day | person |
| `requestJoin` | 10, and 100 | 5 minutes, and a day | person |

A request to a method with a limit of its own spends from the
every-request limit too. `com.atproto.sync.getRepo` is the one
exception, since a relay reads whole repos in bursts. The host's health
check, `/tls/ask` and the firehose (`com.atproto.sync.subscribeRepos`)
spend nothing.

The limits by address guard what anyone can call: each of those methods
makes the host ask another server, or keeps something it was sent. The
limits by group guard the group's repo, where every write is a commit
and an event on the firehose. The limits by person guard what fills a
group's queues for its admins and moderators.

Details:

- The limits are in `crates/group-host/src/limits.rs`.
- A handle change counts only when the handle differs from the group's
  current one, and a report only once the host takes it into the queue.
- An upload counts before the host reads its body, so a refused upload
  costs the host nothing to read.
- Behind a CDN, the proxy has to trust the CDN's addresses
  ([deploying](../deploying.md#routing-through-the-proxy)).
