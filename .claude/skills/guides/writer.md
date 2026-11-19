# Brief for the writer

Fill in the brackets and hand it to a fresh agent.

---

Write the group host's guide to [the task] from scratch, as one Markdown
file at [scratch path]. It will become docs/guides/[guide].md.

## Your inputs

These are your only sources. Read them in full, and open every picture
the walk names, before writing:

- [context file]: the reader, what a group host is in their words, the
  words to explain, the pages you may link to, and the questions the
  guide answers, in order.
- [walk file]: the steps, each with its look line, its picture, and
  what is drawn and done at it.

Do not read existing documentation: README.md, docs/, AGENTS.md. You may
look at more of the console with `just look` to settle a doubt (the ui
skill's Looking section says how), and say so in your report. Leave out
what the walk's Odd section lists.

## How to write it

- A first heading that is the task ("Starting a group"). Under it, one
  to three sentences: "This guide shows you how to [the task]", then
  why someone would want to, in their terms. Nothing else: no "when you
  are done", no list of every step, no prerequisites.
- Then `## Before you begin`: what the reader needs first (a role, a
  member, a tool), one line, or a short list after "You need:" when
  there are two or more. Name the example group the guide uses here.
- A `##` heading per part of the task, in the order it is done, each a
  question from the context or the act ("Invite someone"). Under it,
  numbered steps, one act each: what to press or type, by its words on
  screen in bold ("Press **Start a group**"), and what then shows.
- A picture where the reader needs to see where something is, or what
  they should see now: a dialog, a form, the result. Not every step.
  Each is the walk's look line and image, kept exactly as they are, on
  two lines of their own, after the sentence that says what it shows.
  The image's text says what is in it for a reader who cannot see it.
- After the steps of a part, in a sentence or two, what the reader may
  want to know then: who else sees this, what can be changed later.
- At the end, "From a terminal": the command for each act, from the
  walk's `[cli]` facts, linked to `../reference/cli.md`; then what to do
  next, as links to other guides.
- Second person, present tense, the console's own words for everything
  on it. Explain a word the reader may not know in a clause, the first
  time.
- The voice is `.claude/skills/docs/voice.md`: read it before writing,
  and run its mechanical pass over the page when done. Bold is only for
  words on screen.
- Links: only the paths the context file lists.
- Length: about [n] lines.

## Report back

1. The headings, one line each.
2. The pictures you used, and the ones from the walk you left out.
3. What you checked with `just look`, and what you found.
4. The page length in lines.
