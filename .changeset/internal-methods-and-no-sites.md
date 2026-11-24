---
default: minor
---

# The host's own methods are internal, and groups no longer have websites

The methods that are this host's own, not the opensocial proposal's, moved to `fyi.opensocial.internal.*`, with the views they answer in `fyi.opensocial.internal.defs`. A caller names them by their new NSIDs.

A group's website is gone: `putSite`, `deleteSite`, the `fyi.opensocial.site` record, the `fyi.opensocial.siteSpace` space type, the site kit and the page the host served around a site, `opensocial site` and the `site_publish` and `site_delete` MCP tools. A group's address now only redirects, to its own domain or to its page in the console.

A group's own domain stays, as `fyi.opensocial.internal.putGroupDomain` and `deleteGroupDomain`, `opensocial group domain` and the `group_domain` MCP tool, and the group's view names it as `domain`. `SITE_CHECK_AT` is now `DOMAIN_CHECK_AT`, and the host no longer reads `SITE_SHELL_DIR`, `SITE_HANDLE_RESOLVER` or `SITE_PLC_URL`.

`@opensocial/sdk` is gone.
