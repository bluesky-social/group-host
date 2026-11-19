# Kicker's design

Kicker is a skate community's members-only site: its board, events,
discussions and people, and for anyone else one page with the way in. A
member is often on a phone at the spot, checking what is on this week.

The values are `site/src/tokens.css`; this page says why they are what
they are, and what not to do with them. Read it before changing anything
under `site/`.

## Overview

After late-80s skate magazines, Poweredge (July 1989) above all, taken
minimally. Each reference has one job:

- **The cover gives the masthead.** The club's name in a tall condensed
  grotesque, capitals, pale yellow on sky blue. It is the one loud thing,
  and it appears once on each page: filling the width of the landing, and
  as a slim band above every room.
- **The inside pages give the rest.** Pictures large, captions small and
  plain, a lot of white, and a thread set the way an interview is: the
  question large, each answer under its writer's name.

Everything else on those pages (starbursts, barcodes, stacked letters,
names spaced out, typewriter captions, numbered pictures) stays in 1989.
Taken whole, they read as costume.

## Colors

- **Sky:** the masthead's band, and the one colour that acts (a button
  that does something). Nothing else is blue.
- **Masthead yellow:** the club's name on the band, and nothing else.
- **Ink and paper:** white paper, near-black ink. Quiet ink for what is
  said about a thing (who, when), and a rule colour for the lines between
  things.
- **Danger:** what was called off, deleting, and leaving the place.

In the dark, the paper and ink swap; the band and its yellow stay.

## Typography

One family, Archivo (bundled, `@fontsource-variable/archivo`), across its
widths:

- The name: the narrowest width, the heaviest weight, capitals, as large as
  its band allows.
- Headings: condensed and heavy, sentence case.
- Everything else: the normal width, regular, with lines under 70
  characters.

## Layout

The masthead band runs the full width; everything under it sits in one
column of the page's width, left aligned. A room is its heading and one act
across from it, then its things:

- Notes, in columns, each between thin rules: who and when in quiet ink,
  then the note, large.
- Events: the next one leads, its poster beside it at half the width; the
  rest follow smaller, each a poster with its caption under it.
- A discussion: the question large, the replies under it, each answer
  under its writer's name, a reply to a reply a step in.

## Do's and don'ts

- Do keep the masthead the only loud thing on a page. Before adding
  anything, take something away.
- Do say who and when in words: "Notice from @alice, 2 h ago".
- Don't use the defaults generated pages fall into: cream paper with a
  serif and a clay accent; near-black with one bright accent; hairline
  broadsheet grids with square corners; identical rounded cards under one
  soft shadow; spaced-out capitals above headings; details joined with
  middle dots; monospace for small labels; one word of a heading in
  another colour; numbers on things that are not in order.
- Don't add a colour, a size or a space outside `tokens.css`, nor a style
  in a component.
