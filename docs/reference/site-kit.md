# The site kit

The site kit is a script, `/_os/kit.js`, that lets a page of a group's site ask the host for the visitor's data. A page can ask who they are and what the group's methods answer them. It can read every record of the group's spaces they may read, and write records in their own repo. Only a live site has the kit. That's a site whose record names at least one of `reads`, `spaces`, `acts` or `writes`. Such a site's pages run in a sandboxed frame inside a page of the host's, which holds the visitor's session ([the design page on sites](../design/sites.md) says why).

## Loading it

Load the kit as a classic script, after the page's `<title>`:

```html
<!doctype html><title>Rides</title>
<script src="/_os/kit.js"></script>
```

It defines `window.os`, the object that has every ask below as a method, and the custom element `<os-sign-in>`. Every page of the site that asks anything loads it.

As it loads, and on each `history.pushState`, `history.replaceState`, `popstate` and `hashchange`, the kit tells the host's page where the frame's page is and what it's called. The host's page puts that path in the address bar and the title on the tab, so a reload or a shared link opens the same page.

On a site that names none of `reads`, `spaces`, `acts` or `writes`, the host serves the site's files alone, with no page around them. There `/_os/kit.js` is an ordinary path of the site, and it's a 404 unless the site has a file there.

## What a site declares

A site declares what its pages may ask in its `fyi.opensocial.site` record. Give the lists with `site publish --reads`, `--spaces`, `--acts` and `--writes` ([commands](cli.md)), or as `reads`, `spaces`, `acts` and `writes` to `putSite` ([the API](api.md)). Each list holds at most 20 entries.

`reads` is a list of `{method}`, each one of the host's methods `os.call` may ask:

- `fyi.opensocial.getGroup`
- `fyi.opensocial.listMembers`
- `fyi.opensocial.listSpaceRecords`
- `fyi.opensocial.getSpaceBlob`
- `fyi.opensocial.getActor`
- `fyi.opensocial.getViewer`

`spaces` is a list of `{space, key}`. `space` is a type of the group's spaces whose records a page may read whole with [`os.space`](#osspace), such as `fyi.opensocial.siteSpace`. `key`, if given, narrows the entry to the one space of that type with that key, such as `board`. Without it, the entry covers every space of the type. No pair of type and key may be named twice. A type named both whole and by a key is allowed.

`acts` is a list of `{method}`, each a procedure `os.act` may ask, always of the site's own group:

| Act | What it does for the visitor |
| --- | --- |
| `fyi.opensocial.createReport` | reports what they see, or appeals what was done to them ([moderation](../design/moderation.md)) |
| `fyi.opensocial.requestJoin` | joins the group, or asks to, as its join policy says. It answers `status`, `admitted` or `pending`, and redeems an invite waiting for them |
| `fyi.opensocial.cancelJoinRequest` | cancels their request to join while it waits |
| `fyi.opensocial.leaveGroup` | takes them out of the group. The host refuses with `LastConfigurer` if nobody would be left able to configure it |

Whether they're a member, or have a request waiting, is in `getGroup`'s `viewer` (`member`, `request`).

`writes` is a list of `{space, key, collection}`, where `space` is a type of the group's spaces, `key` is optional, and `collection` is an NSID. An entry lets a space's `create`, `put` and `delete` ([`os.space`](#osspace)) write records of that collection in the group's space of that type with that key, or in any space of the type if it has no key. A site with at least one entry in `writes` may also `os.upload` pictures. The space's access record still decides whether a given visitor may write in a given space.

A site usually keeps what its members write in spaces of `fyi.opensocial.siteSpace`, one per part of the site under its own key ([sites](../design/sites.md#a-space-for-a-site)).

On the command line, `--reads` and `--acts` take NSIDs or their last parts, comma-separated (`--reads getGroup,listMembers`, `--acts createReport`). `--spaces` takes spaces as `<type>` or `<type>/<key>` (`--spaces fyi.opensocial.siteSpace/board,fyi.opensocial.siteSpace/calendar`), and `--writes` takes `<type>:<collection>` or `<type>/<key>:<collection>` (`--writes fyi.opensocial.siteSpace/board:com.example.note`). The host refuses a read or an act that's none of those above with `InvalidSite`.

When a visitor signs in, the host's page asks their PDS for permission to each declared read and act, and to `fyi.opensocial.getViewer`. It also asks to read each declared space in this group, for each declared write, and for uploads of `image/*` if the site declares any writes. Their PDS must support spaces to grant the space permissions.

`os.group`, `os.me`, `os.space`, `os.signIn`, `os.signOut` and `os.onChange` need no declaration beyond the site being live. What a space's methods may do is declared as above.

## The asks

Every ask returns a Promise, except `os.space`, `os.signIn`, `os.signOut`, `os.onChange`, and a space's `records` and `onChange`. The Promise is answered once the host's page has restored the visitor's session, and a refusal rejects it with an `Error` ([refusals](#refusals)).

### os.group

`os.group()` answers which group the site is. It answers `{did, handle, console}`, which are the group's DID, its handle, and the address of the host's console, where its members manage it. It's never refused.

### os.me

`os.me()` answers who's looking, as `{signedIn, did, handle}`. `signedIn` is a boolean. `did` and `handle` are `undefined` while nobody is signed in, and `handle` is also `undefined` if the host couldn't look it up. It's never refused.

### os.call

`os.call(method, params = {})` asks one of the site's `reads` as the visitor, through their PDS. `params` is an object of query parameters. Each is sent as text in the query, and one that's `undefined` is left out. It answers the method's JSON, or a `Blob` for a method that answers a file, such as `getSpaceBlob` (show it with `URL.createObjectURL`). See [api.md](api.md) for each method's parameters and answer.

It's refused, unlocked, if `method` is none of the site's `reads`. It's refused locked with `sign in` if nobody is signed in. If the method refuses, it's refused with the method's own message, and locked with `not for you` when the method answers 401 or 403.

### os.act

`os.act(method, input = {})` asks one of the site's `acts` as the visitor, with `input` as its JSON body. Its `group` is always the site's own: the host's page sets it, whatever the page sent, so a site can't act for its visitors in another group. It answers the method's JSON, or `{}` if the method answers none.

It's refused the same way `os.call` is. If the visitor's sign-in doesn't cover the act, the host's page signs them out and `os.onChange` hears it. Then the ask is refused locked with `sign in`.

### os.space

`os.space(uri)` opens one of this group's spaces, `at://<group DID>/space/<type>/<skey>`, the way [the SDK](sdk.md)'s `GroupSpace` does. The page syncs the space's records and holds them, and writes the visitor's own records in it. It answers the same object each time the page asks for the same `uri`, and makes no request until one of its methods does. Its `uri` is the space's URI.

#### space.sync

`space.sync()` brings the records the page holds of the space up to date, and answers `{failed}`. The host's page reads the records itself, in the visitor's browser. It reads every writer's repo in the space from that writer's own PDS, and checks each against the commit it comes with. It keeps what it read for as long as the page is open, and in the browser between visits. So the first sync reads every writer's repo in full, and each later one fetches only what changed. `failed` lists the writers whose PDS didn't answer, as `{repo, error}`. They keep the records last read from them, and the next sync reads them again.

It's refused, unlocked, if `uri` is none of this group's spaces or no entry of `spaces` covers it: one with its type and either no key or its key. It's refused locked with `sign in` if nobody is signed in, and locked with `not for you` if the space's access record doesn't let the visitor read it.

#### space.records

`space.records({collections} = {})` answers the records the page holds, at once, as `{uri, repo, collection, rkey, cid, value}` objects. `uri` is the record's address in the space, `repo` is its writer's DID, and `value` is the record as JSON. With `collections`, an array of NSIDs, it answers only those collections' records. Before the first sync it answers none. Their order isn't defined, so a page sorts them as it shows them.

#### space.onChange

`space.onChange(fn)` calls `fn(records)` with every record the page holds, after each sync that changes them, the page's first sync included. It returns a function that stops the calls.

#### space.create

`space.create(collection, value, {rkey} = {})` writes `value`, a record of `collection` (its `$type`), in the visitor's own repo in the space, at `rkey` if given. It answers `{uri, cid}`. What the page holds doesn't change until its next sync.

It's refused, unlocked, unless the space is one of this group's and some entry of `writes` has its type, `collection`, and either no key or the space's key. It's also refused unlocked if `value` isn't an object. After those checks, it's refused locked with `sign in` if nobody is signed in. Refusals from the PDS work as they do for `os.act`, so a sign-in that doesn't cover the write is signed out and refused with `sign in`.

#### space.put

`space.put(collection, value, {rkey})` writes `value` at `rkey`, in place of what's there. Otherwise it works like `space.create`.

#### space.delete

`space.delete(collection, {rkey})` deletes the visitor's own record at `rkey`, and answers nothing. Otherwise it works like `space.create`.

### os.upload

`os.upload(file)` uploads a picture to the visitor's own PDS, for a record the site writes to point to. The picture is a `File` or `Blob` whose `type` starts with `image/`. It answers the blob's reference, `{$type: 'blob', ref, mimeType, size}`, for the record to hold. The visitor's PDS sets the size limit.

It's refused, unlocked, if the site declares no `writes` or if `file` isn't a picture. After those checks, it's refused locked with `sign in` if nobody is signed in. Refusals from the PDS work as they do for `os.act`.

### os.signIn

`os.signIn({hint} = {})` opens the host's sign-in dialog over the frame, with its handle field filled in with `hint` if given. It returns `undefined`, and nothing answers it.

The visitor confirms and goes to their PDS to approve what the site declared. When they come back, the host's page loads the page they signed in from again, signed in. `os.me()` then answers `signedIn: true`.

### os.signOut

`os.signOut()` signs the visitor out and returns `undefined`. The page doesn't load again. Instead, `os.onChange` listeners are called with `{signedIn: false, did: undefined, handle: undefined}`.

### os.onChange

`os.onChange(fn)` calls `fn(me)`, with the shape `os.me()` answers, whenever who's looking changes in place. That happens on `os.signOut()`, and when the host's page signs out a sign-in that doesn't cover an act, write or upload. It returns a function that stops the calls.

## Refusals

A refused ask rejects with an `Error` with these fields:

| Field | Value |
| --- | --- |
| `message` | What was refused, in words. |
| `name` | The method's own error name when it refused with one, such as `AlreadyMember` or `LastConfigurer`, as its lexicon lists them. Otherwise `'Error'`. |
| `locked` | `true` if the visitor may not have this now, and `false` otherwise. |
| `reason` | `'sign in'` or `'not for you'` when `locked`, and `undefined` otherwise. |

A page tests `locked`, `reason` and `name`. It shouldn't test `message`.

- `reason` is `'sign in'`. Nobody is signed in, or the visitor was just signed out. Show `<os-sign-in>` or a call to `os.signIn()`.
- `reason` is `'not for you'`. The visitor is signed in but may not have this, such as a non-member asking for the members. Show what the page shows anyone.
- `locked` is `false`. The site asked something it didn't declare, or the method or the network failed. Fix the declaration, or show `message`.

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
- Load the kit after `<title>`. The end of `<body>` is safe. The kit sends the tab's title as it loads and on each move within the page, so changing `document.title` alone isn't reported.
- After signing in, the page loads again. `os.onChange` hears a sign-out and any other change made in place, and a page learns of a sign-in by loading.
- `os.signIn()` does nothing while someone is signed in or the dialog is open, and answers nothing.
- On a live site, the host keeps `client-metadata.json`, `_os/kit.js` and `_os/shell.js` for itself. Don't publish files at those paths.
- `error.locked` is always a boolean. `error.reason` is `'sign in'` or `'not for you'` when locked, and `undefined` otherwise.
- The frame is sandboxed with `allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox` and has an origin of its own. So it has no cookies or storage, no `alert`, `confirm` or `prompt`, and no downloads, and it can't navigate the window around it.
- A page never holds the visitor's tokens. It asks through the kit for what the visitor may see.

## An example

This page lists the members to those who may see them, and lets a member take up their place:

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

You publish it with:

```sh
opensocial site publish <group> <dir> --reads getGroup,listMembers \
  --writes fyi.opensocial.members/self:fyi.opensocial.acceptance
```
