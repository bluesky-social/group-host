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
just look / --phone --dark --click 'header [popovertarget=sign-in]'                # a dialog, on a phone
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
  `Empty`, `Dialog`, `MemberRow`, `GroupRow`).
- `organisms`: a part of a page (`MemberList`, `GroupCard`,
  `SiteHeader`).
- `templates`: a page's layout, without its data (`Page`, `GroupPage`,
  `Welcome`).

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
the same box, never as loose text under its heading. Its title says what
is empty in the reader's terms ("Nobody is asking to join", not "No
data"); its line says why, or what will fill it; and where there is a
way out (invite someone, start a group), it carries it as its action.
The same title, body, spacing and box on every page.

**Space.** Every block stacked on another has a gap from the space
scale; text never touches a row of controls. Things that belong together
sit closer than things that do not: a heading to its list, less than the
list to the next part.

**Weight.** One primary button on a view at most, for the act the view
is for. What undoes, removes or turns down is `class="quiet"` (Leave,
Take back, Turn down, Cancel, Sign out), and a quiet button never
outweighs what it sits beside: leaving a group is there, never the
loudest thing on the card. Quiet what is said about a thing (`.mute`,
`.small`) next to the thing itself.

**The same act, the same way.** A thing done on two pages is done the
same way on both: the same component, the same words, the same order of
controls.

- *Asking for input.* An act that takes a form (inviting, adding a
  rule, a role, a link, a recovery key, an app password) is a button that
  opens a dialog (`Dialog.svelte`) holding the form, as "Invite someone"
  does: never a form inline on one page and a dialog on another. Editing
  what is already there (a profile, its pictures) may be a form in place.
- *Confirming.* An act that cannot be undone (leaving, putting someone
  out, deleting, ending) asks in the same kind of dialog: what will
  happen, in a sentence; a quiet Cancel; the act, as the dialog's one
  button that acts.
- *Lists.* Rows in the part's card, each with its avatar or mark, its
  name, what is said about it, and its acts at the row's end, quiet ones
  before the one that acts; the empty state in the rows' place; "Show
  more" under the last row when there are more.
- *A part's heading.* `Section.svelte`: its title, with how many when
  that is worth saying, and the one act on the whole part (Invite
  someone, Add a rule) across from it, as a quiet button; never a link
  or a button lower down the card doing the same.
- *Where to go.* Ways to a group's other pages sit together, as the
  group's nav, never floating where a banner meets a card.

**Pictures.** A group or person with no picture is drawn as one with
none is meant to look (its initial on its colour), never a blank disc;
a group with no banner shows no banner block.

**People.** A person is their avatar and handle, by `MemberRow` or the
header's menu, never a bare DID where a handle is known.

## Critique

You read your own pictures kindly. Before pushing a change that moves a
page, hand the pictures to an agent that did not write it: the Agent
tool, a fresh one, given the paths under `target/look` (and a sibling
page's), this file's House rules, and only this question: "What looks
wrong, out of place, or unlike its siblings here? Be specific: element,
what is wrong, which rule." Not the code, nor what you meant: it judges
what is drawn. Fix what it finds, or say in the pull why not.

## Then

Give the pull the pictures of the states it changes, before and after,
each as an `--image` to `knotless pull` (or `knotless edit`): the
reviewer cannot see a dialog open or a list emptied otherwise.
