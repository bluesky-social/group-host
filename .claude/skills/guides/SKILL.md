---
name: guides
description: Write, rewrite or check a guide (docs/guides/): how to do something in the group host's console, step by step, with pictures of each screen drawn by `just look`. Use when asked for a guide or a walkthrough, when a change to the console alters a flow a guide shows, or to audit the guides against the console as it is. Facts come from walking the flow in the console first; the page is then written fresh from what was seen, and its pictures drawn again from it.
---

# Writing the group host's guides

A guide shows a person how to do one thing in the console: start a
group, let people in, give someone a role. It is written from the
console as it is drawn, not from memory of it, nor from the code alone:
walk the flow first, picture by picture, write down what each screen
says, then write the page fresh with no old prose in view. Its pictures
are never taken by hand. Each is drawn by `just look` from the line
above it, so `just shots` draws them all again after the console
changes.

This is the docs skill's way (`.claude/skills/docs`) with the console in
place of the code: the design pages say why, the reference lists what,
a guide shows how, in the reader's own words for what they see.

## A guide's pictures

A picture in a guide is two lines, the look line and the image:

```md
<!-- look /g/lanterns.groups.test --as alice --seed full --click 'button[popovertarget=invite]' -->
![The invite dialog, with a handle typed in](shots/invite-dialog.png)
```

The look line is `just look`'s words (the top of
`harness/look/look.mjs` says every option): the page, `--as` whom,
`--seed`, then `--click`, `--type` and `--press` in turn. `just shots
docs/guides/<guide>.md` draws the guide's pictures over a fresh world of
its own, in page order, as `shots/<name>.png` and `shots/<name>-dark.png`;
the site shows the one the reader's scheme asks for. As the world is
the guide's alone, what an early picture does is there for the later
ones: a picture that types a handle and clicks Send invite leaves the
invite waiting for the next picture to show. No `just look-up` may be
running: its world would bring its own state into the pictures.

Each picture is kept as text too, `shots/<name>.txt`: what it shows, as
the page's accessibility tree, with times, DIDs, keys and passwords as
placeholders. The `e2e` check draws every guide again (`just shots
--check`) and fails when a picture's text has changed, so a console
change that moves a guide's flow fails its pull until the guide is drawn
again and read against its pictures. `just docs` fails a picture with no
look line, a look line with no picture, a name used twice, a picture no
guide shows, and a word in bold, as on screen, that no guide's picture
shows: put a word in bold only when a picture shows it, and say it in
plain words otherwise. A picture of a list in no set order (members, by
DID) changes from one world to the next: crop it to the row that
matters (`--element '.members li:has-text("bob.test")'`).

Picture what the reader looks at, and no more: a page is drawn at the
width of the guide's text, so a whole screen there is small. A dialog is
`--click` on what opens it, then `--element ':popover-open'`; a part of
a page is `--element` with its card's selector; a whole page only when
the reader needs to find something on it. A seed is what the reader's
own group would have by that step: `full` for a group in use, none for one just started. A state
no seed makes is a seed of the guide's own, in `harness/look/seeds`.

What look cannot do, a seed does. Look cannot choose a file (a picture
to upload) or open a native `select`; a seed drives the same dialog
through `page(name)`, as `bookclub-pictures` does. The PDS's own sign-in
pages are passed through by `--as` and never drawn. A field the page
checks as it is typed, after a pause (an address being free), is drawn
before it answers unless the line ends with `--press Shift`.

A look line that sends a form does so on every `just shots`, so the
guide's world is the one its lines make: a later picture of the group a
form started needs the line that sent it kept above it.

## Which job

- A console change that alters a flow a guide shows (the `e2e` check
  names the guide): `just shots` for that guide, open every picture,
  and fix the words around any that changed. Steps 1 and 4 for the
  steps it touches.
- A guide to write or rewrite: all four steps.
- An audit: `just shots`, then step 4 for each guide; fix what is
  wrong, or file an issue when the console is what's wrong.

## 1. Walk

Have an agent walk the flow in the console with `just look` and write a
walk to a scratch file: the brief is walker.md beside this file. The
walk is the steps in order, each with its look line, its picture's
path, and facts about it, one a line, tagged:

- `[seen]` drawn in the picture: a heading, a button's words, a field's
  label, what a list or an empty state says. Words exactly as drawn.
- `[code]` read in `web/src`, not drawn at this step: what a form
  refuses and what it then says, a limit, what happens after (name the
  file).
- `[both]`
- `[cli]` the `atgrp` command and MCP tool for the same act, from
  `TOOLS` in `crates/group-host-mcp/src/lib.rs` and
  `docs/reference/cli.md`.

A section headed Odd lists what looks wrong in a picture: broken
layout, a string against the ui skill's house rules, a step that dead
ends. Those are fixed in the console (the ui skill) or filed as issues
(`knotless issue create`), never written around in a guide.

## 2. Context

Write a short context file for the writer:

- The reader (context.md beside this file): a person running a group
  in the console, not a developer.
- What a group host is, in their words (context.md).
- Only the words the walk uses that the reader might not know.
- The pages it may link to, by path: the design page that says why, and
  `../reference/cli.md` for the command.
- The questions the guide answers, in the order a reader asks them:
  questions.md beside this file has the list for each guide.

## 3. Write

Give a fresh agent only the walk, its pictures and the context, and
tell it not to read `docs/`, `README.md` or `AGENTS.md`. It may look at
more of the console with `just look` to settle a doubt, and says so. The
brief, with the style rules, is writer.md beside this file. It keeps
each look line and image from the walk it uses, as they are.

## 4. Draw and check

- `just shots docs/guides/<guide>.md`, then open every picture with
  Read. Each picture shows what the sentence before it says it shows,
  and every word the page puts in quotes or bold as on screen is drawn
  in it, exactly.
- Each step against the walk, and any doubt against `web/src`.
- Hand the page and its pictures to a fresh agent as the reader: "Could
  you do this from the page alone? Where would you be lost, or see
  something the page did not tell you to expect?" Fix what it finds.
- `just docs`, then `just docs-site`, and read the guide there once, top
  to bottom, in light and in dark.
- A new guide goes in `docs/guides/README.md` and the sidebar in
  `docs-site/astro.config.mjs`.
