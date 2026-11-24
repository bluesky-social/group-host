# Brief for the walker

Fill in the brackets and hand it to a fresh agent.

---

Walk the group host's console through [the task: starting a group, from
signing in to the group's page with a picture and words], as [whom: alice,
with nothing yet], and write what you see to [scratch path]. A guide
will be written from your walk alone, so what you do not write down, it
cannot say.

## How to look

`just look` draws a page of the console as it is now and writes
pictures to open with Read. Load the `ui` skill and read its Looking
section first: it says the accounts, the seeds and the options. Start a
world with `just look-up` in the background and draw over it, but for
the walk's own record give every step the look line it would take over
a fresh world, in order: the guide's pictures are drawn that way
(`just shots`), each step's clicks and typing done anew, and what
earlier steps did (a form sent, a seed) there for the later ones.

Draw with `--desktop --light`; a picture for the guide is drawn in both
schemes later. Shoot what the reader looks at: `--click` for a dialog or
a menu (it shoots the screen), `--element` for one card, a whole page
only when the reader must find something on it.

## Each step

```md
## [n]. [What the reader does, in their words]

<!-- look [path] --as [name] [--seed ...] [--click ...] [--type ...] -->
![What it shows](shots/[guide]-[step].png)

Picture: [target/look/...png]

- [seen] ...
- [code] ... (web/src/...)
- [cli] ...
```

- `[seen]`: every heading, label, button, hint and empty state the
  reader reads at this step, exactly as drawn, and where on the screen
  it is.
- `[code]`: what the picture cannot show, read in `web/src`: what each
  field takes and refuses, what is said when it does, what happens once
  it is sent, which pages it reaches.
- `[cli]`: the `atgrp` command and MCP tool doing the same, from
  `TOOLS` in `crates/group-host-mcp/src/lib.rs`, or none.

Walk every branch a reader meets: an empty list before it fills, a
field typed wrong once, a choice with more than one option (each
option's words). Sign in as the other people the task touches (the
person invited, a member who is not an admin) and walk what they see.

## Odd

At the end, a section headed Odd: what looks broken or wrong in a
picture, a string against the ui skill's House rules, a step that dead
ends or needs something only a developer has. File and line where you
can. These are not written into the guide.

## Report back

The steps, one line each; the Odd list; what you could not draw, and
why.
