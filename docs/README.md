# Docs

The group host: the PDS, space authority and OAuth server for groups on
atproto. It implements the opensocial proposal
(https://tangled.org/opensocial.group/proposal).

- [Design](design/README.md): what the host is to a group, and why each
  part works as it does: [spaces](design/spaces.md),
  [groups](design/groups.md), [membership](design/membership.md),
  [signing in](design/signing-in.md), [moderation](design/moderation.md),
  [MCP](design/mcp.md), [recovery](design/recovery.md) and
  [moving](design/moving.md).
- Reference: [the HTTP and XRPC API](reference/api.md), [the MCP
  tools](reference/mcp.md), [the commands and recipes](reference/cli.md)
  and [configuration](reference/config.md).
- [Deploying](deploying.md): the image, running it with its console
  behind a proxy, and what to back up.
- [Development](development.md): building, testing, the checks a pull
  runs, and the docs site.

These pages are also a site (`docs-site/`), for the repository's members:
[development](development.md#the-docs-site) says how it is made.
