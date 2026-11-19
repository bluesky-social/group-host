# Working on Kicker

For every agent working here: CLAUDE.md is this file, imported.

An example: a skate club's members-only site, the group's site on the
group host, with no server of its own, run against the group host of the
repository it sits in. `README.md` says how it works.

- The site is `site/`, a Svelte 5 app built with Vite into `site/dist`,
  which the seed publishes with `index.html` as its fallback. It asks the
  host only through `os` (`/_os/kit.js`, via `site/src/lib/place.js`),
  and only the reads and writes `harness/seed.mjs` publishes it with; a
  new one goes there.
- Its look is `DESIGN.md`: read it before changing anything under `site/`.
  Every colour, size, space, radius and weight is a token in
  `site/src/tokens.css`, taken by name in `site/src/app.css`, where every
  rule is; a component has no styles of its own. Components are in atomic
  layers under `site/src/lib` (`node harness/layers.mjs`); the rooms
  (`site/src/rooms`) fill the template. Sentence case; an empty list says
  what is empty with `Empty.svelte`. Look with `just look` before pushing.
- The space types and records are `lexicons/com/example/place`. A new
  space type is published to the reference PDS by `harness/pds.mjs`,
  which a PDS needs to grant a scope for it.
- The group host is this repository, built by `just host` before each
  world; `harness/bootstrap.sh` builds the spaces alpha at the commit the
  host's `harness/alpha.sh` names.
- Run `just check` and `just e2e` before pushing: they are the checks.
- Commit titles are prose.
