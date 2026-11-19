---
name: docs
description: Write, rewrite or check a page of the group host's docs (docs/, README.md) against the code. Use when a change alters what a doc page says, when asked to write or rewrite a page, or to audit pages for drift. Facts come from the code first; the page is then written from the reader's questions, never by editing old prose.
---

# Writing the group host's docs

Rewriting a page by editing it keeps its shape and its mistakes. These
pages are written from facts instead: gather what the code does, decide
what the reader asks, and write the page fresh with no old prose in view.
Then check it against the facts.

`docs/` is a small wiki, one kind of page per directory:

- `docs/design/`: why it works the way it does. One mechanism per page,
  each explained as the answer to a problem.
- `docs/reference/`: every route, method, tool, command and variable, to
  look up.
- `docs/development.md`: building, testing and the checks.

A guide (`docs/guides/`), how to do something in the console, is the
`guides` skill's: its facts come from the console as drawn, not the code.

The map from code to pages is in AGENTS.md, under Docs. The pages are
also a site (`docs-site/`); a new page goes in its sidebar.

The docs describe this host as it is. They never compare it with another
implementation or say where an idea came from.

Every page is written in one voice, set out in voice.md beside this
file: plain sentences, contractions, the reader as "you", and a list of
patterns to search for and remove. Read it before writing anything here.

## Which job

- A small change to what a page says (a new method, a changed limit):
  edit the page and the reference that lists it, in the pull that makes
  the change. Check the edit against the code it describes. Steps 1 and 4
  below are enough.
- A page to write or rewrite: all four steps.
- An audit (are these pages still true?): steps 1 and 4 for each page,
  then fix what is wrong, or file an issue when the code is what's wrong.
- A voice pass (does this page read well?): voice.md's mechanical pass
  over the page as it stands, changing wording only, then `just docs`.
  Its "What a voice pass must not change" is the check.

## 1. Facts

Have an agent read the code the page covers, and the page as it stands,
and write a fact inventory to a scratch file. Each fact is one line,
tagged:

- `[code]` read in the code (name the file and function)
- `[doc]` said by a page, not found in the code
- `[both]`
- `[DOC WRONG]` a page says something the code contradicts: give both

A section headed Quirks lists what looks like a bug, with file and line.
Those are filed as issues (`knotless issue create`), not written up as
design. Drop `[doc]` facts the code does not bear out.

## 2. Context

Write a short context file for the writer:

- The reader: who they are, what they already know, and why they came to
  this page (see context.md beside this file).
- The group host in three lines (context.md).
- A glossary of only the atproto and host terms the facts use.
- The pages it may link to, by path.
- The questions the page answers, in the order a reader asks them. The
  lists for the design pages are in questions.md beside this file; start
  from those and add what the facts raise.

## 3. Write

Give a fresh agent only the facts and the context, and tell it not to
read `docs/`, `README.md` or `AGENTS.md`. It may read code to settle a
doubt, and says so. The brief, with the style rules, is writer.md beside
this file.

It reports the facts it left off and where each belongs. Place those in
the reference pages yourself (or with another agent), checking each
against the code as you go: details belong there, not on a design page.

## 4. Check

- Every fact on the page against the inventory, and any doubt against the
  code. Agents drop true facts no source stated, and keep false ones a
  page did: read the diff of what was there before.
- Pages that state the same thing agree. The design overview draws on the
  others, so check it last.
- `just docs`: every relative link and anchor lands. The `docs` check runs
  it on the pull, and builds the site.
- `just docs-site`, and read the page once as its reader would, top to
  bottom.
