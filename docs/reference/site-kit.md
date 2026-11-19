# The site kit

The site kit is a script, `/_os/kit.js`, that lets a page of a group's site ask the host for the visitor's data: who they are, what the group's methods answer them, every record of the group's spaces they may read, and records written in their own repo. Only a live site has it: one whose record names at least one of `reads`, `spaces`, `acts` or `writes`. Such a site's pages run in a sandboxed frame inside a page of the host's, which holds the visitor's session; [the design page on sites](../design/sites.md) says why.

## Loading it

Load the kit as a classic script, after the page's `<title>`:

```html
<!doctype html><title>Rides</title>
<script src="/_os/kit.js"></script>
```

It defines `window.os`, the object every ask below is a method of, and the custom element `<os-sign-in>`. Every page of the site that asks anything loads it.

As it loads, and on each `history.pushState`, `history.replaceState`, `popstate` and `hashchange`, the kit tells the host's page where the frame's page is and what it is called. The host's page puts that path in the address bar and the title on the tab, so a reload or a shared link opens the same page.

On a site that names none of `reads`, `spaces`, `acts` or `writes`, the host serves the site's files alone, with no page around them, and `/_os/kit.js` is an ordinary path of the site: a 404 unless the site has a file there.

## What a site declares

A site declares what its pages may ask in its `fyi.opensocial.site` record. Give the lists with `site publish --reads`, `--spaces`, `--acts` and `--writes` ([commands](cli.md)), or as `reads`, `spaces`, `acts` and `writes` to `putSite` ([the API](api.md)). Each list holds at most 20 entries.

`reads` is a list of `{method}`, each one of the host's methods `os.call` may ask:

- `fyi.opensocial.getGroup`
- `fyi.opensocial.listMembers`
- `fyi.opensocial.listSpaceRecords`
- `fyi.opensocial.getSpaceBlob`
- `fyi.opensocial.getActor`
- `fyi.opensocial.getViewer`

`spaces` is a list of `{space}`, each a type of the group's spaces whose records a page may read whole with [`os.space`](#osspace), such as `com.example.place.room`. No type may be named twice.

`acts` is a list of `{method}`, each a procedure `os.act` may ask. The only one is `fyi.opensocial.createReport`, reporting what the visitor sees ([moderation](../design/moderation.md)).

`writes` is a list of `{space, collection}`: `space` a type of the group's spaces, `collection` an NSID. An entry lets a space's `create`, `put` and `delete` ([`os.space`](#osspace)) write records of that collection in any of the group's spaces of that type. A site with at least one entry in `writes` may also `os.upload` pictures. Whether a given visitor may write in a given space is still for that space's access record to say.

On the command line, `--reads` and `--acts` take NSIDs or their last parts, comma-separated (`--reads getGroup,listMembers`, `--acts createReport`), `--spaces` takes space types by NSID (`--spaces com.example.place.room,com.example.place.calendar`), and `--writes` takes `space:collection` pairs (`--writes fyi.opensocial.members:fyi.opensocial.acceptance`). A read or an act that is none of those above is refused with `InvalidSite`.

When a visitor signs in, the host's page asks their PDS for permission to each declared read and act, `fyi.opensocial.getViewer`, reading each declared space type in this group, each declared write, and uploads of `image/*` if the site declares any writes.

`os.group`, `os.me`, `os.space`, `os.signIn`, `os.signOut` and `os.onChange` need no declaration beyond the site being live; what a space's methods may do is declared as above.

## The asks

Every ask but `os.space`, `os.signIn`, `os.signOut`, `os.onChange`, and a space's `records` and `onChange`, returns a Promise. It is answered once the host's page has restored the visitor's session, and a refusal rejects it with an `Error` ([refusals](#refusals)).

### os.group

`os.group()` answers which group the site is. It answers `{did, handle, console}`: the group's DID, its handle, and the address of the host's console, where its members manage it. It is never refused.

### os.me

`os.me()` answers who is looking: `{signedIn, did, handle}`. `signedIn` is a boolean; `did` and `handle` are `undefined` while nobody is signed in. `handle` is also `undefined` if the host could not look it up. It is never refused.

### os.call

`os.call(method, params = {})` asks one of the site's `reads` as the visitor, through their PDS. `params` is an object of query parameters; each is sent as text in the query, and one that is `undefined` is left out. It answers the method's JSON, or a `Blob` for a method that answers a file, such as `getSpaceBlob` (show it with `URL.createObjectURL`). See [api.md](api.md) for each method's parameters and answer.

It is refused, unlocked, if `method` is none of the site's `reads`; locked with `sign in` if nobody is signed in; and with the method's own message if the method refuses, locked with `not for you` when it answers 401 or 403.

### os.act

`os.act(method, input = {})` asks one of the site's `acts` as the visitor, with `input` as its JSON body. It answers the method's JSON, or `{}` if it answers none.

It is refused as `os.call` is. If the visitor's sign-in does not cover the act, the host's page signs them out, `os.onChange` hears it, and the ask is refused locked with `sign in`.

### os.space

`os.space(uri)` opens one of this group's spaces, `at://<group DID>/space/<type>/<skey>`, as [the SDK](sdk.md)'s `GroupSpace` does: the page syncs its records and holds them, and writes the visitor's own records in it. It answers the same object each time the page asks for the same `uri`, and makes no request until one of its methods does. Its `uri` is the space's URI.

#### space.sync

`space.sync()` brings the records the page holds of the space up to date, and answers `{failed}`. The host's page reads the records itself, in the visitor's browser: every writer's repo in the space, from that writer's own PDS, each checked against the commit it comes to. It keeps what it read for as long as the page is open, and in the browser between visits, so the first sync reads every writer's repo in full and each later one fetches only what changed. `failed` lists, as `{repo, error}`, the writers whose PDS did not answer: they keep the records last read from them, and are read again at the next sync.

It is refused, unlocked, if `uri` is none of this group's spaces or of a type the site does not name in `spaces`; locked with `sign in` if nobody is signed in; and locked with `not for you` if the space's access record does not let the visitor read it.

#### space.records

`space.records({collections} = {})` answers, at once, the records the page holds, as `{uri, repo, collection, rkey, cid, value}` objects: `uri` the record's address in the space, `repo` its writer's DID, `value` the record as JSON. With `collections`, an array of NSIDs, it answers those collections' records alone. Before the first sync it answers none. Their order is not defined; a page sorts them as it shows them.

#### space.onChange

`space.onChange(fn)` calls `fn(records)`, with every record the page holds, after each sync that changes them, the page's first sync included. It returns a function that stops the calls.

#### space.create

`space.create(collection, value, {rkey} = {})` writes `value`, a record of `collection` (its `$type`), in the visitor's own repo in the space, at `rkey` if given. It answers `{uri, cid}`. What the page holds does not change until its next sync.

It is refused, unlocked, unless the space is one of this group's and some entry of `writes` has its type and `collection`, or if `value` is not an object; then locked with `sign in` if nobody is signed in. Refusals from the PDS are as for `os.act`: a sign-in that does not cover the write is signed out and refused with `sign in`.

#### space.put

`space.put(collection, value, {rkey})` writes `value` at `rkey`, in place of what is there. Otherwise it is as `space.create`.

#### space.delete

`space.delete(collection, {rkey})` deletes the visitor's own record at `rkey`, and answers nothing. Otherwise it is as `space.create`.

### os.upload

`os.upload(file)` uploads a picture, a `File` or `Blob` whose `type` starts with `image/`, to the visitor's own PDS, for a record the site writes to point to. It answers the blob's reference, `{$type: 'blob', ref, mimeType, size}`, for the record to hold. The size limit is the visitor's PDS's.

It is refused, unlocked, if the site declares no `writes`, or if `file` is not a picture; then locked with `sign in` if nobody is signed in. Refusals from the PDS are as for `os.act`.

### os.signIn

`os.signIn({hint} = {})` opens the host's sign-in dialog over the frame, its handle field filled with `hint` if given. It returns `undefined`; nothing answers it.

The visitor confirms and goes to their PDS to approve what the site declared. When they come back, the host's page loads the page they signed in from again, signed in: `os.me()` then answers `signedIn: true`.

### os.signOut

`os.signOut()` signs the visitor out. It returns `undefined`. The page does not load again; `os.onChange` listeners are called with `{signedIn: false, did: undefined, handle: undefined}`.

### os.onChange

`os.onChange(fn)` calls `fn(me)`, with the shape `os.me()` answers, whenever who is looking changes in place: on `os.signOut()`, and when a sign-in that does not cover an act, write or upload is signed out. It returns a function that stops the calls.

## Refusals

A refused ask rejects with an `Error` with these fields:

| Field | Value |
| --- | --- |
| `message` | What was refused, in words. |
| `locked` | `true` if the visitor may not have this now, else `false`. |
| `reason` | `'sign in'` or `'not for you'` when `locked`, else `undefined`. |

A page tests `locked` and `reason`, not `message`:

- `reason` is `'sign in'`: nobody is signed in, or the visitor was just signed out. Show `<os-sign-in>` or a call to `os.signIn()`.
- `reason` is `'not for you'`: the visitor is signed in but may not have this, such as a non-member asking for the members. Show what the page shows anyone.
- `locked` is `false`: the site asked something it did not declare, the method failed, or the network did. Fix the declaration, or show `message`.

## `<os-sign-in>`

`<os-sign-in>` is a sign-in button in the page's own markup. It draws nothing until `os.me()` answers, then redraws on each `os.onChange`.

Signed out, it holds `<button type="button">Sign in</button>`, which calls `os.signIn` with its `hint` attribute, if it has one:

```html
<os-sign-in hint="alice.example.com"></os-sign-in>
```

Signed in, it holds `<span class="handle">@handle</span>`, a space, and `<button type="button">Sign out</button>`, which calls `os.signOut()`. The span shows the DID when the handle is unknown.

It has no shadow DOM, so the page's CSS styles it with selectors such as `os-sign-in button` and `os-sign-in .handle`.

## Rules for a site's pages

- Link between the site's pages with relative links (`href="rides/"`). A root-absolute link (`href="/rides/"`) loads the host's page, which may not be framed, so the frame shows nothing.
- Load the kit after `<title>`; the end of `<body>` is safe. The tab's title is sent as the kit loads and on each move within the page; changing `document.title` alone is not reported.
- After signing in, the page loads again. `os.onChange` hears a sign-out and any other change made in place; a page learns of a sign-in by loading.
- `os.signIn()` does nothing while someone is signed in or the dialog is open, and answers nothing.
- On a live site, the host keeps `client-metadata.json`, `_os/kit.js` and `_os/shell.js` for itself. Don't publish files at those paths.
- `error.locked` is always a boolean; `error.reason` is `'sign in'` or `'not for you'` when locked, else `undefined`.
- The frame is sandboxed with `allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox` and has an origin of its own: no cookies or storage, no `alert`, `confirm` or `prompt`, no downloads, and no navigating the window around it.
- A page never holds the visitor's tokens; it asks through the kit for what the visitor may see.

## An example

A page that lists the members to those who may see them, and lets a member take up their place:

```html
<!doctype html><title>Riders' rides</title>
<script src="/_os/kit.js"></script>
<os-sign-in></os-sign-in>
<a href="rides/">Rides</a>
<ul id="members"></ul>
<button id="accept" type="button">Take up your place</button>
<script type="module">
  const list = document.getElementById('members');
  const line = (text) => Object.assign(document.createElement('li'), { textContent: text });
  const draw = async () => {
    const { did } = await os.group();
    try {
      const { members } = await os.call('fyi.opensocial.listMembers', { group: did });
      list.replaceChildren(...members.map((m) => line('@' + m.actor.handle)));
    } catch (e) {
      list.replaceChildren(line(e.locked ? 'Members only.' : e.message));
    }
  };
  document.getElementById('accept').onclick = async () => {
    const { did } = await os.group();
    const members = os.space('at://' + did + '/space/fyi.opensocial.members/self');
    const value = { createdAt: new Date().toISOString() };
    await members.put('fyi.opensocial.acceptance', value, { rkey: 'self' });
  };
  os.onChange(draw);
  draw();
</script>
```

Published with:

```sh
opensocial site publish <group> <dir> --reads getGroup,listMembers \
  --writes fyi.opensocial.members:fyi.opensocial.acceptance
```
