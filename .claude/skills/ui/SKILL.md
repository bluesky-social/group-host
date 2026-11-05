---
name: ui
description: Build or change the console's pages (web/) so they look right, not only work. Use for any change to a page, component or style under web/. Look at every state of what you touch with `just look` before pushing, hold it to the house rules here, and have it critiqued by eyes that did not write it.
---

# Building the console's pages

The e2e flows say a page works; they do not say it looks right, and
they only ever see the states their flow passes through. A page nobody
looked at shows it: a group with no banner gets a great empty block, an
empty list is two loose sentences, the button that leaves the group is
the heaviest thing on the card. So look, at every state, before pushing.

AGENTS.md has the mechanics (tokens, component layers, inline text).
This is how the pages should read, and how to check they do.

## Looking

`just look` draws pages as this checkout's source has them, over a world
of the reference PDS, the host and the console's dev server, and writes
pictures to `target/look` for you to open with Read. Starting a world
takes half a minute: run `just look-up` in the background once, and each
look after uses it and takes seconds. A change to a page shows at the
next look; a change to the host needs the world started again.

```sh
just look /                                   # signed out: desktop light, phone dark
just look / --as alice                        # signed in, at alice's PDS, as a person does
just look /g/lanterns.groups.test --as alice --seed full   # a group with everything in it
just look /g/riders.groups.test --as bob      # a group bob is not in
just look /g/lanterns.groups.test --as alice --click 'button[popovertarget=invite]'  # a dialog open
just look /g/lanterns.groups.test/settings --as alice --element .group-card --all   # one element, in all four
just look / --as alice --phone --dark --click '.user-menu summary'                 # a menu, on a phone
just look /new --as alice --type '[name=displayName]=A name long enough to wrap twice over on a phone'  # typed into, then shot
just look /g/lanterns.groups.test --as alice --out target/look/before --name group  # pictures to keep apart
```

Each world starts with alice, bob, carol, dan, erin, frank, harbor and
maximilianaquinces (the longest handle the PDS takes), and Riders
(riders.groups.test), alice's, with nothing else in it: no banner, no
picture, no words, no one else. That is the bare state. A seed puts in
the rest, once per kept world:

- `full`: Lantern Hikers (lanterns.groups.test), alice's, with a banner,
  a picture, words, links, two rules and a moderator role; bob, carol
  (moderator), dan, erin and frank in it, past a page of members; harbor
  asking to join; an invite waiting for maximilianaquinces.
- `long`: a group of dan's with the longest handle, name, words, link
  and rule the host takes; frank asking with a long note.
- `waiting`: bob asking to join Riders; carol invited to it.
- `apps`: an app signed in to Riders with alice's app password
  "Phone", having posted as it, and taken a post down.

A state none of them makes is a seed of your own: a module that writes
it, `--seed path/to/it.mjs` (the top of `harness/look/look.mjs` says
what it is given). Add it to `harness/look/seeds` when others will want
it too.

It says when a page is wider than the screen, naming the element that
runs past the edge: fix that before anything else, as the page then
scrolls sideways on a phone. With `--click`, it centres what it clicks
and shoots the screen, not the whole page, as a dialog or menu is seen.

## The look

Warm paper, not a white sheet (`--bg`); each part of a page a card
lifted softly off it (`Section.svelte`, `.card`); one colour that acts,
`--primary`, so what to press is never in doubt; headings in the
system's rounded face (`--font-round`). Everything comes from the shared
rules in `web/src/app.css`, over the tokens in `web/src/tokens.css`:
take a token by name, and add one there when none fits. A colour for
what it means is its role (`--danger`, `--success`, `--attention`,
`--done`, `--neutral`), never the hue under it. A page has no styles of
its own: a new kind of thing gets a rule in app.css, written for every
page that will show it.

The components are layered, atomic design's way, and `node
harness/layers.mjs` holds each to its layer:

- `atoms`: one element (`Avatar`, `Tag`, `Copy`).
- `molecules`: a few atoms that mean something together (`Section`,
  `Empty`, `Dialog`, `FormDialog`, `ConfirmDialog`, `Tabs`, `UserMenu`,
  `MemberRow`, `Person`, `GroupRow`, `HandleInput`).
- `organisms`: a part of a page (`MemberList`, `GroupCard`,
  `SiteHeader`, `RulesEditor`).
- `templates`: a page's layout, without its data (`Page`, `GroupPage`,
  `Welcome`).

The pieces every page is made of:

- **A page** is `Page`: where it is under (`back`, a group's page), its
  heading, the one act on the whole page across from it (`actions`),
  and its parts' `Tabs` (`nav`) when it has more than one.
- **A page with many parts** is a page per part under it, reached by
  `Tabs`, as a group's settings are (`/g/<handle>/settings`, `/rules`,
  `/roles`, `/apps`, `/recovery`, laid out by its `+layout.svelte`), and
  the moderation queue's statuses: not one long page.
- **A part** is `Section`: a card, its title and count, one quiet act on
  the whole of it (`actions`), a line on what it is (`intro`), then its
  rows or its `Empty`.
- **What is yours** is under your handle in the header, `UserMenu`:
  your groups, starting one, moving an account here, then signing out.

Look in `web/src/lib` before making one; a component made for one page
is made at the layer it belongs to, for the next page too.

## The states to look at

For every list, page or component you touch, each that applies:

- **Signed out and signed in**; **member and not**; **admin and not**
  (alice runs Lantern Hikers, carol moderates it, bob is a member, harbor
  is asking, maximilianaquinces is invited).
- **Empty** (Riders), **one**, **many** (Lantern Hikers, past a page),
  and **long text** (`--seed long`).
- **No picture**: a group with no banner or avatar, as Riders is.
- **Phone** and **dark**: the defaults draw desktop light and phone dark.
- **Open**: every dialog and menu it has (`--click`), on a phone as well.
- **After**: what a form says once sent, and once refused.

Look at a sibling page beside yours, so the new one matches it rather
than inventing.

## House rules

**Case.** Sentence case everywhere: buttons, links, headings, menu
items, field labels, dialog titles, notes and errors ("Invite someone",
"Asking to join", "Sign out"). Only the first word and proper nouns take
a capital: Atmosphere, Bluesky, PDS, DID, AI. Labels end with no full
stop; what is said, a sentence, ends with one. Names stay as written:
opensocial, handles, roles such as `admin`. No `text-transform`.

**Empty states.** One component, `Empty.svelte`, for every empty list or
part, drawn where the list's rows would be: inside the part's card, in
the same dashed box, never as loose text under its heading. Its title
says what is empty in the reader's terms ("Nobody is asking to join",
not "No data"); its line says why, or what will fill it; and where
there is a way out that the part's heading does not already carry, it
carries it as its `action`. The same title, body, spacing and box on
every page, a page with nothing for the reader (a settings page for a
plain member, an error) included. Never style an empty state of your
own.

**Space.** Every block stacked on another has a gap from the space
scale; text never touches a row of controls. Things that belong together
sit closer than things that do not: a heading to its list, less than the
list to the next part.

**Weight.** One primary button on a view at most, for the act the view
is for (Start a group, Join, Save, a dialog's act). What undoes,
removes or declines is `class="quiet"` (Leave, Cancel invite, Decline,
Cancel, Sign out), and a quiet button never outweighs what it
sits beside: leaving a group is a small quiet button at the end of the
member's standing, never the loudest thing on the card. A list whose
rows each wait on an act (approving a request, joining from an invite) may
carry it on each row, `small`, the one thing in the row that is not
quiet. What cannot be undone is `class="danger"`, and only as the act of
a `ConfirmDialog`. A link that goes where a button would act (Start a
group, Settings) is `a.button`, quiet or not by the same rule. Quiet
what is said about a thing (`.mute`, `.what`, `.small`) next to the
thing itself.

**The same act, the same way.** A thing done on two pages is done the
same way on both: the same component, the same words, the same order of
controls.

- _Asking for input._ An act that takes a form (inviting, adding a
  rule, a link, a role, a recovery key, an app password, labelling,
  giving roles, a picture, resolving) is a button that opens a
  `FormDialog` holding the form, as "Invite someone" does: its title the
  act, a line on what it does, the fields, then a quiet Cancel and the
  act. Never a form inline on one page and a dialog on another. Only
  editing what is already there field by field (a group's profile, who
  sees it) is a form in place, and finding (a search box) is never a
  dialog. A handle is typed into `HandleInput`, wherever it is asked for.
- _In place._ An act on the page you are on keeps you where you are:
  its form is sent with `use:enhance`, by `inPlace` from
  `#lib/forms.ts`, and `FormDialog` and `ConfirmDialog` send theirs by
  `inDialog`. Done, the page is drawn again where it was, and a dialog
  closes. Refused, a dialog stays open with what was typed and says why
  inside it; elsewhere the page's notice says why, scrolled to. Never
  a plain post that reloads the page at its top. Only a form that goes
  somewhere else (signing out, starting a group, approving an app) may
  post plainly.
- _Confirming._ An act that cannot be undone (leaving, removing
  someone, deleting a rule or role, removing a key, revoking,
  disconnecting) opens a `ConfirmDialog`: its title the question
  ("Leave this group?", "Delete this rule?"), what will happen in a
  sentence, a quiet Cancel, and the act in `danger`, its verb alone
  ("Delete"). What is undone by doing it again (cancelling an invite,
  removing a picture, declining an invite) needs no asking.
- _Titles._ A dialog's title is a few fixed words: "Remove this
  member?", "Change roles", "Invite someone". Never put a name, handle,
  or anything someone typed in it ("bob.test's roles", "Revoke
  “Phone”?"). Those are as long as people make them, and the title then
  wraps over three lines or runs off the dialog. Say which thing in the
  body, where it wraps as text: a person by `Person` ("`@bob.test` loses
  access…"), anything else by its name in the sentence ("Apps using
  “Phone” are signed out"). `node harness/dialogs.mjs` fails on a title
  built around a value.
- _Lists._ `ul` or `ol` with a class, a row per thing, inside the part's
  card: its `Avatar`, `ordinal` or picture first; its name, and under it
  what is said about it (`.row-main` with `.what`); its acts at the
  row's end in `.row-actions`, `small`, quiet ones before the one that
  acts. A person is `MemberRow`, a group `GroupRow`. Times by
  `#lib/dates.ts`, never as a record holds them. The empty state in the
  rows' place; a quiet "Show more" under the last row while there are
  more, a page at a time, never every row at once.
- _A part's heading._ `Section`: its title, with its count when that is
  worth saying, and the one act on the whole part (Invite someone, Add a
  rule, Create an app password) across from it, as a quiet button; never
  a link or a button lower down the card doing the same.
- _Where to go._ A group's own pages (Moderation, Settings) sit together
  beside its name, as quiet small `a.button`s; a page under a group says
  so with `Page`'s `back`, its name, above its heading.

**Verbs.** The words people know from other apps, the same everywhere:

- **Delete** a thing the group made and owns: a rule, a role.
- **Remove** something or someone from a list: a member ("Remove
  @bob.test from Riders?"), a link, a picture, a key.
- **Cancel** what you sent and can call back: an invite ("Cancel
  invite"), your request ("Cancel request").
- **Approve** or **Decline** someone else's request, or an invite to you.
- **Revoke** access: an app's, an app password.
- **Create** a new thing that is then listed (a role, an app password);
  **Add** one to a list (a rule, a link, a key); **Save** changes made
  in place, and as a dialog's act, the verb alone ("Create", "Add",
  "Save").
- Kept as they are: Send invite, Send request, Join, Leave the group,
  Disconnect.

Never "take away", "take back", "put out", "turn down", "let in", "take
off" or "end access". The names of commands, tools and lexicons
(`eject`, `--undo`) stay as they are.

**Plain words.** Every string on a page reads as a normal app's would:
short, concrete, in the second person, friendly without chatter. What
gives writing away as a machine's
([Signs of AI writing](https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing)),
and is never written here:

- triplets ("Its chat, its calendar, its site: …");
- colon chains, an "X: Y" where a sentence does ("Copy it now: it is
  not shown again");
- negative parallels, "not X, but Y";
- the elliptical "who may" ("someone who may lets them in"): say who,
  and what they do ("an admin approves them");
- an abstract noun for a person ("Whoever asks shows here");
- a line under a heading that says the heading again;
- default microcopy ("No items found", "Submit", "Try again later")
  where a line of this page's own fits;
- the stock words: seamless, effortless, simply, robust, delve,
  leverage, crucial; em dashes; emoji.

As it should read:

- "Its members alone: to others, just its handle" → "Members only.
  Others see just its handle."
- "Whoever asks shows here, with their note." → "Requests to join show
  up here."
- "Copy it now: it is not shown again." → "Copy it now. You won't see
  it again."
- "It can't have that address here: choose another." → "That address
  is taken. Try another."

**Pictures.** `Avatar` is given the name of whom it draws: with no
picture, it draws their initial on a hue their name gives them, never a
blank disc. A group with no banner shows no banner block: its picture,
name and nav sit at the top of its card.

**People.** A person is their avatar and handle, by `MemberRow` (a row), `Person` (in a line of text) or the
header's menu, never a bare DID where a handle is known.

## Critique

You read your own pictures kindly. Before pushing a change that moves a
page, hand the pictures to an agent that did not write it: the Agent
tool, a fresh one, given the paths under `target/look` (and a sibling
page's), this file's House rules, and only this question: "What looks
wrong, out of place, or unlike its siblings here? Be specific: element,
what is wrong, which rule." Not the code, nor what you meant: it judges
what is drawn. Ask it, too, to read every string it can see against
"Verbs" and "Plain words". Fix what it finds, or say in the pull why
not.

## Then

Give the pull the pictures of the states it changes, before and after,
each as an `--image` to `knotless pull` (or `knotless edit`): the
reviewer cannot see a dialog open or a list emptied otherwise.
