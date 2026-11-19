# Sites

A group can have a website, from a front page that says who it is to as
many pages as it likes. The host serves it at the group's address,
`https://<address>/`, as static files, the way they were published. It has
no templates, no build step and no rendering on the server. A site that
names `reads` may also show the group as each visitor may see it. One
that names `spaces` may read the group's spaces whole as each visitor
may, and one that names `acts` may act for each visitor, such as
reporting what they see. One that names `writes` may write in the
group's spaces as each visitor may. Each works once the visitor signs in
on the site, through a page the host serves around its own
([live data](#live-data)).

## What a site is

A site is one record, `fyi.opensocial.site`, kept at `self` in the group's
public repo. It lists the site's files, each with:

- its `path` under the site's root, such as `index.html` or
  `rides/index.html`.
- its `blob`, the file's bytes, which is a blob of the group's.
- optionally its `contentType`. Without one, the type comes from the
  path's extension, and a file with an extension the host doesn't know
  is `application/octet-stream`.

It may also name two of those files:

- `fallback`, served with status 200 for a page no file has, so a
  single-page app can route in the browser.
- `notFound`, served with status 404 for any other path no file has.

It may also name `reads`, up to 20 of the host's methods its pages may
call as the visitor who signs in on it. Those are
`fyi.opensocial.getGroup`, `listMembers`, `listSpaceRecords`,
`getSpaceBlob`, `getActor` and `getViewer`, and no others.

It may name `acts`, up to 20 of the host's procedures its pages may call
as that person, each acting for them alone, and only in this group: the
host's page sets each call's `group` itself. Those are
`fyi.opensocial.createReport`, to report what they see, and
`requestJoin`, `cancelJoinRequest` and `leaveGroup`, so a site can let
people ask to join, cancel the request, and leave. It may name `spaces`, up to 20 of
the group's spaces its pages may read whole as that person. Each is a
type, such as `fyi.opensocial.siteSpace`, and may give a `key` to name
one space of that type, such as `board`. Without a key, it's every space
of the type. And it may name `writes`, up to 20 kinds of record its
pages may write in that person's own repo. Each is a type of the group's
spaces, a key if it's for one space of them, and a collection there,
such as `com.example.note` in `fyi.opensocial.siteSpace` under `board`.

A site has 1 to 2000 files. Each publish replaces the record whole, and
deleting the record takes the site down. A blob only the old record held
is deleted with it, just as any blob is once no record holds it.

Since the record is public, any app can read a group's site, and its
files are the group's blobs like any other.

## Publishing

`fyi.opensocial.putSite` publishes a site whole from the files given, each
as `text` or as `bytes`. The host keeps each file as a blob of the
group's, and reuses one the group already holds. A blob's CID is the
hash of its bytes, so a file that hasn't changed is the same blob. The
answer says how many files were new. `fyi.opensocial.deleteSite` takes
the site down.

Both require `group.configure`. In the console, a group's own page links
to its site while it has one, and its settings have a Site page. That
page says where the site is, how many files it has and when it was
published. It publishes a folder you choose in the browser, or deletes
the site after asking you to confirm. Publishing takes a `404.html` at
the folder's top as `notFound`. It keeps `fallback`, `reads`, `spaces`,
`acts` and `writes` under Advanced, filled in as the site has them.

The `opensocial site publish` command reads a folder and sends its
files. `--reads` and `--acts` name methods by NSID or by their last
part, `--spaces` names each space as `<type>` or `<type>/<key>`, and
`--writes` names each write as `<type>:<collection>` or
`<type>/<key>:<collection>`. `site delete` takes the site down. Both
leave out files whose names start with a dot. They send each file as
text if it's UTF-8 and as bytes otherwise, and they take the folder's
own name off each path. An AI app
connected over MCP publishes with `site_publish`, writing each file as
text.

An app signed in as the group may also write the record itself, with
`com.atproto.repo.putRecord`, after uploading the blobs. The host holds
that write to the same rules. The record is kept at `self`, and the
member who approved the app must hold `group.configure`. Every path must
be one a file may be at, with none given twice, and every file must name
a blob. `fallback` and `notFound` must name files of the site. `reads`
and `acts` must name only the methods above. Each of `spaces` must name
a type, and may name no type and key twice. Naming a type whole and also
by one of its keys is allowed. A site that breaks a rule fails with
`InvalidSite`.

A path is `/`-separated, with no leading `/`, and no empty, `.` or `..`
segment, backslash or control character.

## A space for a site

A member's PDS grants a site permission to a space type only once it can
read that type's schema, published under a DNS lexicon authority. A site
with space types of its own would need a domain, a `_lexicon` TXT record
and its schemas published there before anyone could sign in to it.

So the host publishes one space type with its own lexicons,
`fyi.opensocial.siteSpace`, for a site to keep its members' records in.
It lists no collections, and a group makes one space of it for each part
of its site, under a key of its own, such as `board` or `calendar`. The
site names those spaces by key in `spaces` and `writes`, so each sign-in
asks for those spaces only, not every space of the type. The collections
written there never need publishing either: a write permission names
its collection, and a PDS doesn't look collections up.

A group makes such a space with `space create`, as any other:

```sh
opensocial space create <group> fyi.opensocial.siteSpace --skey board \
  --name Board --access '{"grants": [{"role": "member", "scopes": ["space:fyi.opensocial.siteSpace?action=read"]}]}'
```

Then the site names it:

```sh
opensocial site publish <group> <dir> --spaces fyi.opensocial.siteSpace/board \
  --writes fyi.opensocial.siteSpace/board:com.example.note
```

## Serving

When a request's `Host` is a group's address here, the host answers it
from the group's site. The exceptions are `/.well-known/` and `/xrpc/`,
which the host keeps for itself there. While the address is the group's
handle, its `atproto-did` is how anyone checks that the handle names the
group. And the page around a site that reads calls the host's methods at
its own origin.

The address is where the group was made
([groups](groups.md#its-address)). It stays the site's address when the
group takes a handle on a domain of its own, until a steward moves the
site there ([on its own domain](#on-its-own-domain)). A request to the
host's own name, `PUBLIC_URL`, never goes to a site.

The host answers only `GET` and `HEAD`, and anything else gets 405. It
decodes a path's `%XX` escapes, then answers in this order:

1. `/` and any path ending in `/` serve the `index.html` under it.
2. Any other path serves the file at it.
3. A path with an `index.html` under it, such as `/rides`, redirects to
   `/rides/`.
4. A page serves `fallback` with 200. A page is a request that accepts
   `text/html`, or a path whose last segment has no extension.
5. Anything else serves `notFound` with 404, or a plain 404 if there's
   no `notFound`.

A group with no site redirects `/` to its page in the console, and answers
404 elsewhere. A group that's switched off answers 404. A site that came
with a group moved or imported here wasn't checked when it was kept. So
if one of its files names no blob, the host serves it as if it weren't
there.

The host sends each file with its CID as its `ETag` and `Cache-Control:
public, max-age=60`. A browser that already holds that ETag gets 304.

## On its own domain

A group with a handle on a domain of its own, such as `riders.org`, may
want its site there too. The handle proves the group controls the domain.
It doesn't prove the domain's web traffic reaches this host, since its A
record may still name an old site the group is leaving. So the site
moves only when a steward moves it, with `fyi.opensocial.putSiteDomain`
(for `group.configure`). It moves only if the domain then answers as
this host. The host asks
`https://<domain>/xrpc/com.atproto.server.describeServer`, or
`SITE_CHECK_AT` with the domain as its `Host`, for its own DID. If the
handle is the group's address, the call fails with `NoOwnDomain`. If the
domain doesn't answer as this host, it fails with `NotReaching`.

Once moved, the domain serves the site just as the address did, and it
keeps `/.well-known/` and `/xrpc/` for the host. A request to the
address, or to `www.` under the domain, redirects (301) to the same path
and query at the domain. The host's own paths at the address don't
redirect. The group's view names the domain as `siteDomain`, and
`putSite` answers with the site's `url` there.
`fyi.opensocial.deleteSiteDomain` serves the site at its address alone
again. So does the group taking another handle, since the host never
checked that handle's domain.

In the console, the Site page's Domain card says where the site is. It
moves the site to the domain, telling you which DNS records to add
first, and it moves it back. `opensocial site domain <group>` moves it,
and `--undo` moves it back. An AI app connected over MCP does the same
with `site_domain`.

The host's proxy gets each domain's certificate the first time it's
asked for. It does that once the host says the domain is a group's own
domain, or `www.` under it, while the group is on
([deploying](../deploying.md#serving-sites-on-groups-own-domains)).

## The sandbox

A group's address is a name under the host's domain. So a browser treats
every group's pages as the same site as the console, at the host's own
name. Without a sandbox, a page's scripts could set a cookie for the
whole domain, which the console would then receive. They could also
send a request or a form to the host that carried a member's console
session, since `SameSite` cookies only stop requests from other sites.

The host names its console's cookies `__Host-`, so no page at a group's
address can set one in their place. It also takes no cookie from a
request whose `Origin` isn't the console's
([signing in](signing-in.md)). The sandbox keeps a page's requests from
carrying the cookie at all.

So every answer to a request for a site, redirects and errors included,
carries:

```
Content-Security-Policy: sandbox allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox
Access-Control-Allow-Origin: *
X-Content-Type-Options: nosniff
Referrer-Policy: strict-origin-when-cross-origin
```

The `sandbox` directive gives each page an origin of its own that no other
page shares. Its scripts run, its forms submit, and its links open. But
it can't read or set a cookie, and it can't keep anything in storage.
What it sends the host arrives as if from another site, without the
console's session. `Access-Control-Allow-Origin: *` lets it still load
its own scripts, fonts and data, which are cross-origin requests from
that origin. Its files are public anyway. `nosniff` stops a browser from
treating a file as a type other than the one it's served as.

The sandbox lives in how the host serves a site, and not in what
publishes one. That's because anyone whose roles carry
`group.configure` can write the record, from any app.

## Live data

A page in the sandbox has no session, so it can show only what it was
published with. A group often wants more on its site, like its rules,
who's in it and what members post, and a way for members to post there
too. The group's roles decide who may see each of those, so it can't be
published into the page for everyone. And what a member writes is
theirs, in their own repo. Instead, each visitor signs in on the site.
The page asks the host as them, and writes at the visitor's own PDS as
them.

### The page around a site

A site that names `reads`, `spaces`, `acts` or `writes` is served
differently. Each of its pages is the host's own page, which holds the
visitor's session. It shows the
site's page in a sandboxed frame beneath it, filling the window. The
site's files are under `/_site/` there, sandboxed as above, and a file
that isn't a page is still served at its own path.

The host's page answers with the status the site's page would have (404
where the site has no page, as for a static site). It also carries the
site's page's own `<title>` and the `<meta>` tags a link preview is read
from (`description`, `theme-color`, `twitter:*` and Open Graph's
`og:*`). The host writes each one afresh with its text escaped, so none
of the site's markup reaches the host's page. A crawler runs no script,
so without those tags every link would preview as the bare address.

The host's page runs only the host's script (`/_os/shell.js`, built from
`web/shell` and read from `SITE_SHELL_DIR`). Its policy allows no other
script and frames only its own origin. It's an atproto OAuth client of
its own. It's a public client whose client id is
`https://<address>/client-metadata.json` and whose callback is
`/_os/callback` there. A proxy may name the host it was asked as in
`X-Forwarded-Host`, as Caddy does, and then both are at that name
instead.

Its scope is `atproto`, plus these:

- for each of the site's `reads` and `acts`, and `getViewer`,
  `rpc:<method>?aud=<the host's service DID>#opensocial`.
- for each of its `spaces`,
  `space:<type>?authority=<the group>&skey=<key>&action=read`.
- for each of its `writes`,
  `space:<type>?authority=<the group>&skey=<key>&collection=<collection>&action=create&action=update&action=delete`.
- `blob:image/*`, if it has any `writes`.

An entry without a key leaves out `&skey=<key>`, and its scope covers
every space of the type. So the site can call those methods on the
host, read those of this group's spaces, and write those records in the
visitor's own repo in them.
It can't do anything else. The session is kept in the browser at the
address's origin and goes only to the visitor's PDS. The site's frame
never holds it.

The visitor's PDS must support spaces for them to sign in to a site
that names `spaces` or `writes`, since it must grant the `space:`
scopes. bsky.social doesn't yet.

As the visitor moves through the site, the host's page puts the frame's
page in the address bar and its title on the tab. So a reload or a
copied link opens the page they're on.

### What a page asks

A page loads `/_os/kit.js`, which gives it `os`. Each ask is a message to
the host's page, which answers it. [The site kit](../reference/site-kit.md)
gives each ask's parameters, answers and refusals, and an example page:

| Ask | Answer |
| --- | --- |
| `os.group()` | `{did, handle, console}` of the group whose site it is, where `console` is the address of the host's console |
| `os.me()` | `{signedIn, did, handle}` of the visitor |
| `os.call(method, params)` | the method's answer, asked as the visitor. That's its JSON, or a `Blob` for a method that answers with a file (`getSpaceBlob`) |
| `os.act(method, input)` | the procedure's answer, asked as the visitor with `input` as its JSON, its `group` the site's own. For `createReport` that's its `id` and `createdAt`; for `requestJoin`, its `status` |
| `os.space(uri)` | one of the group's spaces (`at://<group>/space/<type>/<skey>`), with the SDK's `GroupSpace` methods below. The same `uri` gives the same object |
| `space.sync()` | `{failed}`, once the page's records of a space are up to date as the visitor may read them. The site must name the space in `spaces`, by its type alone or by its type and key. `failed` lists the writers whose PDS didn't answer |
| `space.records({collections})` | every record held, right away, each as `{uri, repo, collection, rkey, cid, value}`. Given `collections`, only those collections' records |
| `space.onChange(fn)` | calls `fn` with every record held after each sync that changes them |
| `space.create(collection, value, {rkey})` | the `uri` and `cid` of `value`, written in the visitor's own repo in the space |
| `space.put(collection, value, {rkey})` | the same, at `rkey`, replacing what's there |
| `space.delete(collection, {rkey})` | deletes the visitor's record at `rkey` |
| `os.upload(file)` | the blob reference of `file`, a picture uploaded to the visitor's own PDS for a record the site writes to point to |
| `os.signIn({hint})` | opens the sign-in dialog, with `hint` filled in as the handle |
| `os.signOut()` | signs the visitor out |
| `os.onChange(fn)` | calls `fn` with `os.me()`'s answer when it changes in place, as on signing out. Signing in loads the page again |

`<os-sign-in>` is a button in the page's own markup, styled by the
page's own styles. It shows "Sign in" while nobody's signed in, and the
visitor's handle and "Sign out" once someone is.

`os.call` fails for a method the site doesn't name in `reads`, and
`os.act` fails for one it doesn't name in `acts`. A write fails for a
collection or a space its `writes` don't name, or for a space whose
authority isn't the group. Called while nobody's signed in, each fails
with `locked` and the reason `sign in`. When the host or the visitor's
PDS refuses the visitor (401 or 403), it fails with `locked` and `not
for you`. A page shows what it may, and asks the visitor to sign in for
the rest.

A read goes through the visitor's PDS to the host, as any app's call
does ([signing in](signing-in.md)). The host answers it just as it
answers that person anywhere else. A write is made at the visitor's PDS,
as any app's write in a space is. The space's access record still says
whether they may write there ([spaces](spaces.md)), so `writes` asks no
more of the visitor than the group lets them do. The host hears of the
write as it hears of any member's, so it's in the next
`listSpaceRecords`. Say the visitor signed in before the site named a
write or an act. The PDS refuses the call for want of scope, so the
host's page forgets that sign-in and fails with `sign in`. Signing in
again grants the scope.

A site that names `writes` may also upload pictures. Its sign-in asks
the visitor's PDS for `blob:image/*` too, and `os.upload` takes only a
file of an `image/` type. The picture is the visitor's, in their own
repo. A record in a space that points to it makes it the space's. Anyone
who reads the space reads it with `getSpaceBlob`, so a site names that
in `reads` to show it. A site with no `writes` uploads nothing.

A page that lists, counts or threads what members wrote reads the space
whole with `os.space`, and works those out itself. The host's page reads
each writer's repo in the space from their own PDS, as any app reading a
space does. It uses the visitor's credential from the space's authority,
and checks each repo against its commit. So the host keeps no copy of a
space for the page to ask, and the access record decides what each
visitor reads, just as it does at every repo. What the page read is kept
in the browser at the address's origin. So a later read, on this visit
or the next, fetches only what changed. That holds up for a group whose
spaces hold a few thousand records. Past that, the first read of each
visit gets slow, and a group wants an index of its own beside the host.
The host's page does this with [the SDK](../reference/sdk.md).

An act, like a read, goes through the visitor's PDS to the host. The
host takes it just as it takes the same from that person anywhere else.
So a report from a site's page is checked and kept in the group's queue,
as any report is ([moderation](moderation.md)). A page can send
moderators to it with the console's address that `os.group()` answers.
The group's Moderation page there,
`<console>/g/<handle>/moderation?review=<subject>`, opens the Review of
the named account (by its DID) or record (by its URI), whatever its
status.

### Signing in

The dialog is the console's, drawn by the host's page over the site. It
has a handle field, which `searchActors` at the address fills with
suggestions as you type, and "Create one", which makes an account at
`SIGNUP_PDS`. A page may suggest a handle, but it can't send anyone
anywhere. The visitor sees where they're signing in, types or confirms
the handle, and enters their password only at their own PDS. The page
resolves the typed handle through `SITE_HANDLE_RESOLVER`, and the DID
that names through `SITE_PLC_URL`, both from the visitor's browser
([configuration](../reference/config.md)). Back from approving, the
visitor lands signed in on the page they started from.

The host's page at the address isn't sandboxed, since it must keep a
session. It's the host's own code, the same for every group. The site's
pages, which anyone with `group.configure` can write, stay sandboxed in
the frame and only ask.
