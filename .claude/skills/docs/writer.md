# Brief for the writer

Fill in the brackets and hand it to a fresh agent.

---

Write the group host's [design page / reference page] for [topic] from
scratch, as one Markdown file at [scratch path]. It will become [docs path].

## Your inputs

These two files are your only sources. Read them in full before writing:

- [context file]: the reader, background, a glossary, the pages you may
  link to, and the questions the page answers, in order.
- [facts file]: the facts, from the code and the old pages.

`[DOC WRONG]` means an old page was wrong: write what the code does,
without mentioning the old page. Leave out what the Quirks section lists,
but never state the opposite of a quirk as design: describe the intended
rule.

Do not read existing documentation: README.md, docs/, AGENTS.md. You may
read code only to settle a doubt about a fact, and say so in your report.
Describe this host as it is: never compare it with another
implementation, or say where an idea came from.

## How to write it

- Lead with the problem: [what the page's subject is up against]. Explain
  each mechanism as the answer to a problem, so a reader knows why a rule
  exists by the time they meet it. Put the lowest-level details in a
  short plain list headed "Details" at the end of their section.
- Define before use.
- Plain technical prose: present tense, active voice, one idea per
  sentence, exact names in backticks, short paragraphs. A list only for
  truly parallel items, a table only for tabular data.
- Don't number the problems and refer back to them; let the structure
  make the connection.
- Avoid the marks of machine writing:
  - inflated significance ("plays a crucial role", "serves as")
  - "not just X but Y", and reflexive groups of three
  - trailing "-ing" commentary
  - bold-headed bullet lists (`- **Thing.** text`)
  - bold for emphasis, em dashes, title case in headings
  - slogans, and summary sentences that repeat the paragraph above
  - the words delve, robust, seamless, leverage, comprehensive, powerful
- No diagrams.
- Links: only the paths the context file lists.
- Length: about [n] lines. Command syntax and flags belong in
  `../reference/cli.md` and `../reference/api.md`.

## Report back

1. The headings, one line each.
2. Facts you left off, and where each belongs.
3. What you checked in the code, and what you found.
4. The page length in lines.
