# Kicker

An example app on the group host: a skate club's members-only site, with
no server of its own. It is the group's website on the group host of
this repository, and everything in it is the group's, and private.

The site has four rooms:

- **Board.** Members pin notes (a notice, an ask, an offer). The
  moderators fold or hide one, and its author appeals.
- **Events.** Hosts and admins publish sessions and events, each with its
  poster; members answer them, going or interested; a host calls their
  own off.
- **Discussion.** Members start discussions and reply, a reply to a reply
  under what it answers.
- **People.** Every member, with their roles, and the card each writes:
  who they are, what they can help with, and a picture of them, if they
  put one on it.

## How it works

The site is a Svelte app (`site/`), built into static files and published
as the group's `fyi.opensocial.site` record, its `index.html` the fallback
for every room's address. The host serves it in a frame inside a page of
its own, which signs the visitor in with OAuth at their own PDS and makes
what the app asks of it as them, through `os` from `/_os/kit.js`. The
rooms are routes of one page: moving between them changes the address
(the kit shows it in the bar) and nothing loads again, and a room opens
with what it held last time while it reads afresh.

|       | What                                           | How                                                                                                                                |
| ----- | ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Read  | the group, its members and their roles         | `os.call('fyi.opensocial.getGroup' / 'listMembers')`                                                                               |
| Read  | everything members wrote in the place's spaces | `os.space(uri)`, synced by the host's page with each writer's repo and held whole; each room works out what it shows (`site/src/lib/views.js`) |
| Write | a note, an answer, a card, an event | the space's `create` / `put` / `delete` (`os.space`), at the visitor's own PDS, in their own repo in one of the place's spaces |
| Write | an event's poster, or a picture on a member's card | `os.upload`, to the visitor's own PDS, the record pointing to it |
| Read | those pictures | `os.call('fyi.opensocial.getSpaceBlob')`, which the host reads from whoever uploaded it |
| Act | a report of a note, or an appeal of one's own | `os.act('fyi.opensocial.createReport')`, into the group host's moderation queue |
| Read | the moderators' labels on notes | the group's own `com.atproto.label.record` records in the same spaces, shown as the group defines each (`getGroup`'s `labels`) |

The site declares those reads, and its writes (each a space, by its type
and key, and a collection), when it is published; the visitor's sign-in
asks their PDS for exactly those, in this group's spaces alone. Both
spaces are of the host's `fyi.opensocial.siteSpace` type, published with
its lexicons, so the site publishes nothing for a PDS to read: its
collections never need it. What each person may write is the spaces' to
say:

| Space                               | Holds                         | Who writes                      |
| ----------------------------------- | ----------------------------- | ------------------------------- |
| `fyi.opensocial.siteSpace/room`     | notes, answers, cards         | any member                      |
| `fyi.opensocial.siteSpace/calendar` | events, each with its picture | hosts and admins (`writableBy`) |

Every member reads each. Moderation is the group host's own. A member
reports a note from the site; a moderator decides in the group host's
console, from its queue, with a label, such as the place's own
`off-topic`, or atproto's `!hide`; the site shows a note as its labels
say, folded behind a warning or hidden from all but its author and the
moderators, and its author appeals from the site. A moderator's link,
Review, opens the note in the console.

Anyone else, signed out or not let in yet, sees the club's landing: its
name as the masthead, its line, its (made-up) address and hours, and the
way in. That page asks the host nothing, so what it says is published
with the site, in `site/src/lib/about.js`.

## Running it

```sh
just bootstrap   # once: the reference PDS on the spaces alpha, and the packages
just dev         # the site at https://kicker.places.localtest.me, behind portless
just e2e         # the flows, in Chromium, signed in with OAuth
just look        # the rooms as pictures in target/look (--as bob)
just check       # svelte-check, and the components' layers
```

The look, after late-80s skate magazines, is `DESIGN.md`: what it takes
from them, what it leaves, and the defaults it stays clear of. Its values
are `site/src/tokens.css`, taken by name in `site/src/app.css`. Its
components are in atomic layers under `site/src/lib` (atoms, molecules,
organisms, the `Place` template), each using only its own layer and
those below, which `harness/layers.mjs` checks; the rooms in
`site/src/rooms` fill the template.

`harness/seed.mjs` makes the place with the `opensocial` command (the
group, its host role, its spaces, its site) and writes what each person
wrote at their own PDS, each event with a two-ink poster that
`harness/posters.mjs` draws as SVG and its host uploads. `harness/world.mjs`
and `pds.mjs` are adapted from the group host's own harness, with the PDS
told to resolve the host's space types, the site's among them; the https
front is the host's `harness/tls-proxy.mjs`. `just dev`, `just e2e` and
`just look` build the host, its command and its console first (`just host`).
