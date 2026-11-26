# Docs

The group host is the PDS, space authority and OAuth server for groups
on atproto. It implements the Atmosphere Groups spec
(https://tangled.org/atmospheregroups.org/spec).

- [Guides](guides/README.md) show how to do each thing in the console,
  step by step, with a picture of each screen:
  [starting a group](guides/starting-a-group.md), [rules and
  roles](guides/rules-and-roles.md), [moderating](guides/moderating.md),
  [signing in as a group](guides/signing-in-as-a-group.md) and [recovery
  keys](guides/recovery-keys.md).
- [Design](design/README.md) says what the host is to a group, and why
  each part works the way it does: [spaces](design/spaces.md),
  [groups](design/groups.md), [membership](design/membership.md),
  [signing in](design/signing-in.md), [moderation](design/moderation.md),
  [MCP](design/mcp.md), [recovery](design/recovery.md) and
  [moving](design/moving.md).
- Reference: [the HTTP and XRPC API](reference/api.md), [the MCP
  tools](reference/mcp.md), [the commands and recipes](reference/cli.md)
  and [configuration](reference/config.md).
- [Deploying](deploying.md) covers the image, running it with its
  console behind a proxy, and what to back up.
- [Development](development.md) covers building, testing, the checks a
  pull runs, and the docs site.

These pages are also a site (`docs-site/`) for the repository's members.
[Development](development.md#the-docs-site) says how it's made.
