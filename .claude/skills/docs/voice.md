# The docs' voice

How a page reads, sentence by sentence. Every page in `docs/` and
`README.md` is written in it, the guides included, and a voice pass
checks a page against it.

## Who it's for

Write for someone running a group host, building an app that talks to
one, or changing this one: what the host does, what it costs them, what
to watch for and what to do. The reader is an engineer. Explain the
system the way you'd explain it to a coworker at a whiteboard.

## What the voice is

- Plain declarative sentences, one idea each. Most run 12 to 25 words.
  No semicolons and no em dashes in prose.
- Conversational connectives. Sentences link up with "So", "Since",
  "That means", "In this case", "But", never with punctuation.
- Contractions everywhere: it's, don't, can't, isn't, there's, that's,
  won't.
- Person. The reader is "you" ("If you run a host behind a proxy…").
  The system is the subject of mechanism sentences ("The host signs…",
  "The console asks…", "A steward's key ranks…"). "We" is fine when
  walking through a mechanism step by step. No "I".
- Plain word order. Subject, verb, object. "With only its key on the
  document, the host is the only one that can move the group", never
  "Left so, only the host could move a group".
- Concrete before abstract. Name the situation first (an admin removes a
  steward's key), then the rule. Define a term in one plain sentence
  ("A steward is someone the group trusts with its identity") and then
  give an example.
- Numbers do the arguing. Give the default, the limit and the unit
  ("the host retries a notice for a day"), and the variable or flag that
  sets it.
- Trade-offs said plainly. State the cost, then why it's acceptable.
  One qualifier at most.
- Paragraphs of one to four sentences.
- Parentheses for asides: a default, a variable, an example. One fact
  per parenthesis.
- "May" states a permission ("a member with `group.configure` may add
  keys"). "Can" states an ability. Neither is a hedge, so keep whichever
  is true.
- Words the voice uses: so, since, that's, around, plenty, cheap, a bit,
  pretty, just (sparingly), keep up with, hold up.

## Banned patterns

Search for each of these on every page you touch.

| Pattern | Looks like | Do instead |
|---|---|---|
| Semicolon splice | "A key is `AlreadyAdded`; a fifth is `TooManyKeys`." | Two sentences, or join with "and", "but" or "so". |
| Em dash or `--` aside | "the group — and only the group — signs" | Parentheses, a comma, or a second sentence. |
| "X, not Y" flourish | "Roles decide who may add keys, not who holds them." | Say X. Say Y in its own sentence only if a reader would assume it. |
| Colon chains | "The rule is simple: one owner: always." | One colon per sentence at most, and only to introduce a list, code or an example. |
| Rule-of-three lists | "fast, cheap and simple" | Keep the items that carry information. Two is fine. One is often better. |
| Abstract nouns for actions | "the handling of", "the revocation of" | The verb: "handles", "revokes". |
| Archaic or inverted phrasing | "Left so", "is apart from", "whatever the key asked about", "as it is now" used for weight | Plain word order and everyday words. |
| Filler | crucially, importantly, in practice, it's worth noting, notably, essentially, effectively, robust, seamless, under the hood, the heart of, first-class, by design, the key insight, the trick is, the catch is, load-bearing | Delete it. If the sentence falls apart, rewrite it. |
| Bold lead-in on bullets | "- **Stewards.** A steward…" | Plain bullets. A list of named things may start each item with the name in `code` and a period. |
| Bold or italics for stress | "the host **never** sees it" | Put what matters first, or last, in the sentence. |
| One-line paragraph for drama | "The host can't undo it." alone | Fold it into the paragraph it concludes. |
| Aphorism endings | "Less to keep, less to lose." | State the consequence plainly, or cut it. |
| Narrating the page | "This page covers…", "In short:", "Below we'll look at…" | Delete. Start with the content. A one-line pointer to another page is fine. |
| Stacked qualifiers | "usually only briefly", "about ~" | One qualifier, or none. |
| Fragments as style | "Nothing else." "By design." | Full sentences. |
| Lead-ins that announce importance | "This matters because…" | Cut the lead-in, keep the reason. |
| History and comparison | "no longer", "used to", "now does", another implementation | Describe the host as it is today. |
| Exclamation marks | | None. |

## Mechanical pass

Apply these in order to every paragraph, list item and table cell you
edit.

1. Replace every semicolon in prose with a period, or with "and", "but"
   or "so". A table cell may separate short items with `·`.
2. Replace every em dash or double hyphen used as punctuation.
3. Count colons. More than one in a sentence: split it. A colon that
   isn't introducing a list, code or an example: make it a period, or
   "because" or "so".
4. Find ", not " and "rather than". Keep one only when it corrects a
   real misconception a reader would have.
5. Delete every filler word. Re-read the sentence.
6. Remove bold and italics used for stress. In a guide, bold stays on
   words the console shows (below).
7. Use contractions: it is → it's, does not → doesn't, cannot → can't.
8. Split any sentence over about 35 words, or with more than two clauses
   joined by commas.
9. Make the actor the subject: "is refused by the host" → "the host
   refuses".
10. Untangle inverted and archaic phrasing into subject, verb, object.
11. Delete sentences that describe the page instead of the host.
12. Read the paragraph aloud. If you wouldn't say it to a coworker,
    rewrite it.

A page usually comes out about the same length. If it grows by more than
about 10%, something got explained twice.

## Before and after

> Before: Left so, only the host could move a group, and nobody could
> move it without the host.
>
> After: With only that key on the document, the host is the only one
> that can move a group, and nobody can move it without the host.

> Before: Holding a key is apart from holding a role. The group's roles
> decide who may add or remove keys on the host, not who holds which key.
>
> After: Holding a key has nothing to do with holding a role. The group's
> roles decide who may add or remove keys on the host. They don't decide
> who holds which key.

> Before: A key already on the document is `AlreadyAdded`; a fifth
> recovery key is `TooManyKeys`.
>
> After: Adding a key that's already on the document fails with
> `AlreadyAdded`, and adding a fifth recovery key fails with
> `TooManyKeys`.

> Before: When a group's identity has left the host's control, the host
> stops changing it.
>
> After: Once a group's identity is out of the host's control, the host
> stops changing it.

## What a voice pass must not change

A voice pass changes wording only. Check each of these before
committing:

- Facts. Every number, unit, default, limit, variable, flag, method,
  permission, error name, record field and path stays exactly as it is.
  If a sentence goes, its fact moves to another sentence or was already
  on the page.
- Nothing new. Don't add a claim, number or feature that isn't on the
  page or in the code. If something reads wrong, flag it rather than
  fixing it in prose (or check the code and say so).
- Headings. `just docs` checks every link and anchor, and other pages
  link to headings. Leave headings as they are. If one has to change,
  update every link to it in the same commit.
- Links. Every link stays and points to the same target.
- Code. Fenced blocks and inline code are untouched. A method is named
  in code and the docs site links it, so it stays as written.
- Guides. A word in bold is a word on screen, and `just docs` checks it
  against the pictures: keep it, spelled as the console spells it. Never
  touch a `<!-- look … -->` line or the image under it.
- Design pages keep their "Details:" lists at the end of a section.
- Tables keep their columns. A reference page's cells get the same pass
  as prose, kept short.

Keep the wrapping the page has (about 72 columns), and run `just docs`
after every page.

## One home for each fact

Each fact has one home page. Other pages state it in a clause and link
there. Design pages say why, the reference lists what, a guide shows
how, so the details go to the reference and a design page links them.

## The README

`README.md` is the front door, in the voice and in this shape:

1. What the host is, in a paragraph or two: what it does for a group,
   and what it's made of.
2. `## Highlights`: one bullet a part, five to seven. Each starts with a
   bold label (the one place a bold lead-in belongs), says what the part
   does in two or three sentences, and ends `→ [page](…)` with the
   design or reference page that owns it.
3. `## Quickstart`: what you need, then commands runnable as written,
   from an empty checkout to something to look at, then how to deploy.
4. `## Docs`: a two-column table of the pages, each with what's in it.
5. `## The pieces`: the crates and directories in a sentence, linking
   the table in `docs/design/README.md`, which is their one home.
6. `## Development`: the recipes for testing and checking, commented.
7. `## License`.

Its facts come from the pages it links, and a change to one of those
parts updates its bullet.
