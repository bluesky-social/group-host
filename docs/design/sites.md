# Sites

A group can have a website: a front page that says who it is, or as many
pages as it likes. The host serves it at the group's address,
`https://<address>/`, as static files, the way they were published. It has
no templates, no build step and no rendering on the server. A site that
names `reads` may also show the group as each visitor may see it, once
they sign in on it, through a page the host serves around its own
([live data](#live-data)).

## What a site is

A site is one record, `fyi.opensocial.site`, kept at `self` in the group's
public repo. It lists the site's files, each with:

- its `path` under the site's root, such as `index.html` or
  `rides/index.html`;
- its `blob`, the file's bytes, a blob of the group's;
- optionally its `contentType`. Without one, the type comes from the
  path's extension, and a file with an extension the host does not know
  is `application/octet-stream`.

It may also name two of those files:

- `fallback`, served with status 200 for a page no file has, so that a
  single-page app can route in the browser;
- `notFound`, served with status 404 for any other path no file has.

And it may name `reads`, up to 20 of the host's methods its pages may
ask as whoever signs in on it: `fyi.opensocial.getGroup`, `listMembers`,
`listSpaceRecords`, `getSpaceBlob`, `getActor` and `getViewer`, and no
others.

A site has 1 to 2000 files. Each publish replaces the record whole, and
deleting the record takes the site down. A blob only the old record held
is deleted with it, as any blob is once no record holds it.

Because the record is public, any app can read a group's site, and its
files are the group's blobs like any other.

## Publishing

`fyi.opensocial.putSite` publishes a site whole from the files given, each
as `text` or as `bytes`. The host keeps each file as a blob of the
group's, and uses again one the group holds already: a blob's CID is the
hash of its bytes, so a file that has not changed is the same blob. It
answers how many files were new. `fyi.opensocial.deleteSite` takes the
site down.

Both require `group.configure`. In the console, a group's own page links
to its site while it has one, and its settings have a Site page, which
says where the site is, how many files it has and when it was published,
and publishes a folder chosen in the browser, or deletes the site, asked
to be sure of. Publishing takes a `404.html` at the folder's top as
`notFound`, and keeps `fallback` and `reads` under Advanced, ticked and
filled as the site has them. The `opensocial site publish` command reads a folder
and sends its files, with `--reads` naming methods by NSID or by their
last part; `site delete` takes the site down. Both leave out files whose
names start with a dot, send each file as text if it is UTF-8 and as
bytes otherwise, and take the folder's own name off each path. An AI app
connected over MCP publishes with `site_publish`, writing each file as
text.

An app signed in as the group may also write the record itself, with
`com.atproto.repo.putRecord`, after uploading the blobs. The host holds
that write to the same rules: the record is kept at `self`, the member who
approved the app must hold `group.configure`, every path must be one a
file may be at and none given twice, every file must name a blob,
`fallback` and `notFound` must name files of the site, and `reads` must
name only the methods above. A refused site is `InvalidSite`.

A path is `/`-separated, with no leading `/`, and no empty, `.` or `..`
segment, backslash or control character.

## Serving

A request whose `Host` is a group's address here is answered from its site,
except under `/.well-known/` and `/xrpc/`, which the host keeps for itself
there: while the address is the group's handle, its `atproto-did` is how
anyone checks that the handle names the group, and the page around a site
that reads asks the host's methods at its own origin. The address is where
the group was made ([groups](groups.md#its-address)), and stays its site's
when the group takes a handle on a domain of its own; that domain is the
group's name, not its site's. A request to the host's own name,
`PUBLIC_URL`, is never a site's.

Only `GET` and `HEAD` are answered; anything else is 405. A path is read
with its `%XX` escapes decoded, then answered in this order:

1. `/` and any path ending in `/` serve the `index.html` under it.
2. Any other path serves the file at it.
3. A path with an `index.html` under it, such as `/rides`, redirects to
   `/rides/`.
4. A page serves `fallback` with 200. A page is a request that accepts
   `text/html`, or a path whose last segment has no extension.
5. Anything else serves `notFound` with 404, or, with none, a plain 404.

A group with no site redirects `/` to its page in the console, and answers
404 elsewhere. A group switched off answers 404. A site that came with a
group moved or imported here was not checked as it was kept, so a file of
it that names no blob is served as if it were not there.

Each file is sent with its CID as its `ETag` and `Cache-Control: public,
max-age=60`. A browser that holds that ETag already is answered 304.

## The sandbox

A group's address is a name under the host's domain, so a browser takes
every group's pages for the same site as the console, at the host's own
name. Left alone, a page's scripts could set a cookie for the whole
domain, which the console would then receive. They could also send a
request or a form to the host that carried a member's console session,
since `SameSite` cookies only stop requests from other sites. The host
names its console's cookies `__Host-`, so no page at a group's address
can set one in their place, and takes no cookie from a request whose
`Origin` is not the console's ([signing in](signing-in.md)); the sandbox
keeps a page's requests from carrying the cookie at all.

So every answer to a request for a site, redirects and errors included,
carries:

```
Content-Security-Policy: sandbox allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox
Access-Control-Allow-Origin: *
X-Content-Type-Options: nosniff
Referrer-Policy: strict-origin-when-cross-origin
```

The `sandbox` directive gives each page an origin of its own that no other
page shares. Its scripts run, its forms submit, and its links open, but it
can neither read nor set a cookie, nor keep anything in storage. What it
sends the host goes as from another site, without the console's session.
`Access-Control-Allow-Origin: *` lets it still load its own scripts,
fonts and data, which from that origin are cross-origin requests; its
files are public anyway. `nosniff` stops a browser from taking a file for
a type other than the one it is served as.

The sandbox is in how the host serves a site, not in what publishes one,
because anyone whose roles carry `group.configure` can write the record,
from any app.

## Live data

A page in the sandbox has no session, so it can show only what it was
published with. A group often wants more on its site: its rules, who is
in it, what members post. All of that is the group's to show only to
those its roles allow, so it can't be published into the page for
everyone. Instead, each visitor signs in on the site, and the page asks
the host as them.

### The page around a site

A site that names `reads` is served differently. Each page of it is the
host's own page, which holds the visitor's session and shows the
site's page in a sandboxed frame beneath it, filling the window. The
site's files are under `/_site/` there, sandboxed as above, and a file
that is not a page is still served at its own path. The host's page
answers with the status the site's page would have (404 where the site
has no page, as for a static site), and with the site's page's own
`<title>` and the `<meta>` tags a link preview is read from
(`description`, `theme-color`, `twitter:*` and Open Graph's `og:*`),
each written afresh with its text escaped, so nothing of the site's
markup reaches the host's page. A crawler runs no script, so without them
every link would preview as the bare address.

The host's page runs only the host's script (`/_os/shell.js`, built from
`web/shell` and read from `SITE_SHELL_DIR`), under a policy that allows no
other script and frames only its own origin. It is an atproto OAuth
client of its own, a public one whose client id is
`https://<address>/client-metadata.json`, and whose callback is
`/_os/callback` there; behind a proxy that names the host it was asked
as `X-Forwarded-Host`, as Caddy does, both are at that name instead. Its
scope is `atproto` and, for each of the site's
`reads` and `getViewer`, `rpc:<method>?aud=<the host's service DID>#opensocial`:
it can ask the host those methods and nothing else. The session is kept
in the browser at the address's origin, and goes to the visitor's PDS
alone; the site's frame never holds it.

As the visitor moves through the site, the host's page puts the frame's
page in the address bar and its title on the tab, so that a reload or a
copied link opens the page they are on.

### What a page asks

A page loads `/_os/kit.js`, which gives it `os`. Each ask is a message to
the host's page, which answers it:

| Ask | Answer |
| --- | --- |
| `os.group()` | `{did, handle}` of the group whose site it is |
| `os.me()` | `{signedIn, did, handle}` of who is looking |
| `os.call(method, params)` | the method's answer, asked as the visitor |
| `os.signIn({hint})` | opens the sign-in dialog, `hint` filled in as the handle |
| `os.signOut()` | signs the visitor out |
| `os.onChange(fn)` | calls `fn` with `os.me()`'s answer whenever it changes |

`<os-sign-in>` is a button in the page's own markup, which the page's own
styles style: "Sign in" while nobody is, the visitor's handle and "Sign
out" once someone is.

`os.call` is refused for a method the site does not name in `reads`.
Asked while nobody is signed in, it is refused with `locked` and the
reason `sign in`; when the host refuses the visitor (401 or 403), with
`locked` and `not for you`. A page shows what it may, and asks the
visitor to sign in for the rest. The call goes through the visitor's PDS
to the host, as any app's call does ([signing in](signing-in.md)), and
the host answers it as it answers that person anywhere else.

### Signing in

The dialog is the console's, drawn by the host's page over the site:
a handle, suggested as it is typed by `searchActors` at the address, and
"Create one", which makes an account at `SIGNUP_PDS`. A page may suggest
a handle, but cannot send anyone anywhere: the visitor sees where they
are signing in, types or confirms the handle, and enters their password
only at their own PDS. The handle typed is resolved through
`SITE_HANDLE_RESOLVER`. Back from approving, the visitor lands on the page
they started from, signed in.

The host's page at the address is not sandboxed, as it must keep a
session; it is the host's own code, the same for every group. The site's
pages, which anyone with `group.configure` can write, stay in the frame,
sandboxed, and only ask.
