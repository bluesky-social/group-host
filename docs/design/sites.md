# Sites

A group can have a website: a front page that says who it is, or as many
pages as it likes. The host serves it at the group's handle,
`https://<handle>/`, as static files, the way they were published. It has
no templates, no build step and no rendering on the server. A page that
wants live data, such as the group's events or members, asks the host's
public methods from the browser.

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

Both require `group.configure`. The `opensocial site publish` command
reads a folder and sends its files, leaving out those whose names start
with a dot; `site delete` takes the site down. An AI app connected over
MCP publishes with `site_publish`, writing each file as text.

An app signed in as the group may also write the record itself, with
`com.atproto.repo.putRecord`, after uploading the blobs. The host holds
that write to the same rules: the record is kept at `self`, the member who
approved the app must hold `group.configure`, every path must be one a
file may be at and none given twice, every file must name a blob, and
`fallback` and `notFound` must name files of the site. A refused site is
`InvalidSite`.

A path is `/`-separated, with no leading `/`, and no empty, `.` or `..`
segment, backslash or control character.

## Serving

A request whose `Host` is a group's handle here is answered from its site,
except under `/.well-known/`, which the host keeps for itself there: the
handle's `atproto-did` is how anyone checks that the handle names the
group. A request to the host's own name, `PUBLIC_URL`, is never a site's.

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

A group's handle is a name under the host's domain, so a browser takes
every group's pages for the same site as the console, at the host's own
name. Left alone, a page's scripts could set a cookie for the whole
domain, which the console would then receive. They could also send a
request or a form to the host that carried a member's console session,
since `SameSite` cookies only stop requests from other sites.

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
