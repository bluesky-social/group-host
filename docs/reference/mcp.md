# MCP tools

The host offers these tools to an MCP client at `<PUBLIC_URL>/mcp`, over
MCP's Streamable HTTP. It answers each JSON-RPC message on its own, as
JSON. It speaks MCP `2025-06-18`, `2025-03-26` and `2024-11-05`, and
answers an `initialize` that asks for another version with the latest.
[MCP](../design/mcp.md) says how a client connects and signs in.

| Message                                                     | Answer                                                            |
| ----------------------------------------------------------- | ----------------------------------------------------------------- |
| `initialize`, `ping`, `tools/list`, `tools/call`            | as MCP has them                                                   |
| a notification, with no `id`                                | 202, with no body                                                 |
| any other method                                            | JSON-RPC `-32601`                                                 |
| a body that isn't JSON                                      | 400, JSON-RPC `-32700` with a null `id`                           |
| JSON that isn't one JSON-RPC 2.0 object (a batch, a string) | `-32600`                                                          |
| an unknown tool, or arguments that aren't an object         | `-32602`                                                          |
| no token, or one the host doesn't know                      | 401, with `WWW-Authenticate: Bearer` naming the resource metadata |

## The tools are the commands

Every command `opensocial <group> <action>` that acts in a group has one
tool, `<group>_<action>`. So `opensocial member assign` is `member_assign`.
A tool takes what its command takes, except the group, which is always
the one the client connected to. It takes the command's arguments by the
names its usage gives them (`member`, `roles`), and its flags in snake
case (`--display-name` is `display_name`, `--undo` is `undo`).
Where the command takes a comma-separated list, the tool takes an array.
[The commands](cli.md#opensocial) list each one's arguments. `TOOLS` in
`crates/group-host-mcp/src/lib.rs` lists the tools, and the command reads
its words by the tool's schema, so the two can't drift.

The commands with no tool make, move or delete a group or an account:
`group create`, `group seed`, `group move`, `group delete` and `account
import`. A client connected to a group can switch it off, and its
stewards can undo that, but it can't delete the group. `admin logs` and
`admin stats` have no tool either, since they read the whole host and a
client acts in one group.

The host runs each tool as the member who connected the client, in that
group. A tool does one of four things:

- It asks one of the host's methods, as the member.
- It writes one of the group's records (a profile, rule, role or
  membership) as the member's own session for the group would. It
  carries the scope a group token for that space carries, holds to the
  member's current roles, and logs the write under the client.
- It writes an invite in the invitee's inbox, or takes it back, the same
  way.
- It adds to a role's grant in its record, or takes from it.

[MCP](../design/mcp.md#tools-and-calls) says why.

The host makes a tool's input schema from the lexicon of what it asks or
writes, named in the Lexicon column below. The schema leaves out the
group and the time a record was made, which the host fills in. A tool's
description says what it does, then what that lexicon says. A method's
result is the method's JSON. A write's result is the record's `uri`, plus
its `cid` for a put. Each comes as text and, for an object, as
`structuredContent`. A call the member's roles don't allow, or that the
host refuses, comes back `isError: true`, with the text `<error>:
<message>` in the host's words. If a required argument is missing or
isn't text, the host refuses it before asking anything:
`<name> is required, as text`.

A write replaces the record whole and drops any field not given. The
exceptions are `profile_set` and `role_grant`, which change only what
they're given. You can't upload a picture through MCP, so `profile_set`
keeps the group's pictures as they are. When `profile_set` writes a link
the profile didn't have, the host asks the favicon service `FAVICONS`
names for the site's icon and keeps it as the group's. A link the
profile already had keeps its icon.

## group

| Tool               | Command            | Lexicon                            | What it does                                                                                 |
| ------------------ | ------------------ | ---------------------------------- | -------------------------------------------------------------------------------------------- |
| `group_show`       | `group show`       | `fyi.opensocial.getGroup`          | shows the group as the member may see it: its profile, rules, member count, roles and spaces |
| `group_deactivate` | `group deactivate` | `fyi.opensocial.deactivateGroup`   | switches the group off. With `undo`, `fyi.opensocial.activateGroup` switches it back on      |
| `group_handle`     | `group handle`     | `fyi.opensocial.updateGroupHandle` | gives the group a handle on a domain that names it, or gives its own address back            |

## profile

| Tool          | Command       | Lexicon                  | What it does                                                                                                                                                            |
| ------------- | ------------- | ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `profile_set` | `profile set` | `fyi.opensocial.profile` | changes the fields given in the group's profile (at `self` in its meta space) as the group, and keeps the rest. A field given empty, an empty list or `null` is removed |

## rule

| Tool          | Command       | Lexicon               | What it does                                                                                                                                 |
| ------------- | ------------- | --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `rule_set`    | `rule set`    | `fyi.opensocial.rule` | writes a rule in the meta space as the group. It writes the one `rule` names by its key, or without one a new rule keyed by the host's clock |
| `rule_delete` | `rule delete` | `fyi.opensocial.rule` | deletes the rule `rule` names                                                                                                                |

## label

| Tool           | Command        | Lexicon                                | What it does                                                                                                                                                       |
| -------------- | -------------- | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `label_define` | `label define` | `fyi.opensocial.putLabelDefinition`    | defines or changes the group's own label `label`: its `name` and `description`, and how apps should show it (`severity`, `blurs`, `default_setting`, `adult_only`) |
| `label_delete` | `label delete` | `fyi.opensocial.deleteLabelDefinition` | stops defining the label `label`                                                                                                                                   |

## role

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `role_set` | `role set` | `fyi.opensocial.role` | writes the role `role` (its id) in the members space, as the group, with its `scopes` and the roles its holders may assign (`assignable`, `*` for every role) |
| `role_delete` | `role delete` | `fyi.opensocial.role` | deletes a role nobody holds |
| `role_grant` | `role grant` | `fyi.opensocial.role` | adds the `scopes` given, and the roles its holders may assign (`assignable`), to those in the record of `role`. With `undo`, it takes them away, or takes all of both if none are given. The rest of the record stays as it was, and the write follows the same rules as any write of it. A role the group doesn't have is refused |

## site

| Tool           | Command        | Lexicon                        | What it does                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| -------------- | -------------- | ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `site_publish` | `site publish` | `fyi.opensocial.putSite`       | publishes the group's whole website from `files`, each `{path, text}` or `{path, bytes}`. `fallback` and `not_found` name files among them, and `reads` names the host's methods its pages may ask as whoever signs in. `acts` names the procedures they may ask as that person, and `spaces` the group's spaces they may read whole, each `{space, key}`, where `key` names one space of the type and without it it's every one (`{"space": "fyi.opensocial.siteSpace", "key": "board"}`). `writes` names the records they may write in that person's own repo, each `{space, key, collection}`, with `key` the same way. `inbox` makes that person's invite inbox as they sign in, if they have none. The command reads the files from a folder |
| `site_delete`  | `site delete`  | `fyi.opensocial.deleteSite`    | takes the group's website down                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `site_domain`  | `site domain`  | `fyi.opensocial.putSiteDomain` | serves the group's website at its handle's own domain once that domain reaches this host. With `undo`, `fyi.opensocial.deleteSiteDomain` serves it at its address alone again                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |

## member

| Tool            | Command         | Lexicon                      | What it does                                                                                                                                                    |
| --------------- | --------------- | ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `member_list`   | `member list`   | `fyi.opensocial.listMembers` | lists who's in the group and their roles, a page at a time, filtered by part of a handle or DID (`q`)                                                           |
| `member_assign` | `member assign` | `fyi.opensocial.membership`  | writes the membership of `member` (a DID) with `roles` in place of theirs, `member` among them. It lets in someone waiting to join, or changes a member's roles |
| `member_eject`  | `member eject`  | `fyi.opensocial.membership`  | deletes the membership, which puts them out                                                                                                                     |

## join

| Tool        | Command     | Lexicon                           | What it does                                |
| ----------- | ----------- | --------------------------------- | ------------------------------------------- |
| `join_list` | `join list` | `fyi.opensocial.listJoinRequests` | lists the requests to join that are waiting |
| `join_deny` | `join deny` | `fyi.opensocial.denyJoinRequest`  | turns down `member`'s request               |

## invite

| Tool          | Command       | Lexicon                      | What it does                                                                                                                                                  |
| ------------- | ------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `invite_send` | `invite send` | `fyi.opensocial.invite`      | invites `person` (a DID), with an optional `message` and `expires_at`, and answers the invite's `uri`. With `undo`, it takes back the invite waiting for them |
| `invite_list` | `invite list` | `fyi.opensocial.listInvites` | lists the invites that are waiting                                                                                                                            |

## space

| Tool            | Command         | Lexicon                           | What it does                                                                                                                                                   |
| --------------- | --------------- | --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `space_create`  | `space create`  | `com.atproto.space.createSpace`   | makes a space of `type` under `policy`, an `fyi.opensocial.policy` object, as the group with a group token for it. `skey` is its key, or a new one if left out |
| `space_records` | `space records` | `fyi.opensocial.listSpaceRecords` | lists the records in the space `space`                                                                                                                         |

## mod

`subject` is an account's DID or a record's `at://` URI. Where the method
asks for a version, it comes with its `cid`, the way a label on a record
does.

| Tool | Command | Lexicon | What it does |
| --- | --- | --- | --- |
| `mod_list` | `mod list` | `fyi.opensocial.listModSubjects` | shows the moderation queue |
| `mod_history` | `mod history` | `fyi.opensocial.getModSubjectHistory` | shows what was done about a subject |
| `mod_resolve` | `mod resolve` | `fyi.opensocial.resolveModSubject` | resolves or escalates a subject |
| `mod_label` | `mod label` | `com.atproto.space.createRecord` | labels a subject `val` as the group, with a `com.atproto.label.record` record beside it. With `undo`, it deletes the label (`com.atproto.space.deleteRecord`) |
