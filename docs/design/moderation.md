# Moderation

A group's members write in their own repos, on their own PDSes. The
group can't edit or delete what they wrote. So a group moderates by
writing its judgements beside the content, as records of its own, and by
keeping a queue of what still needs a judgement. Reports come in, labels
go out, and the queue sits between them.

Every moderation method works on a group hosted here, a group account
with a members [space](spaces.md). The host refuses any other DID with
`GroupNotFound`.

## Who moderates

Moderating comes down to roles, like everything else a member does in a
group (see [membership](membership.md)). Three of the host's actions are
about moderation:

- `mod.read`: see the queue and a subject's history, and a record
  subject itself, taken down or not.
- `mod.resolve`: resolve or reopen a subject, tag it, and add a note.
- `label`: write a label, of any value, and delete one, in the spaces
  the role's permissions reach ([labels](#labels)).

A role holds an action through a permission in its `permissions`, and a
member holds the union of their roles' actions. There's no deny and no
hierarchy. A new group's `admin` role holds every action, moderation
included. Its `member` role holds none. To make moderators, a group
gives a role these actions and gives that role to people.

Each method checks the caller's roles at the moment of the call. The
caller is the person who signed in, either through service auth from
their PDS or through the console's session (see [signing
in](signing-in.md)). Either way, they name the group as `group`. The
host refuses a caller without the action with `403 Forbidden`. An app
signed in as the group calls the queue's methods as the member who let
it in, within that member's roles, and doesn't name the group, since
its session already does. An MCP client a member connects can read the
queue, label and update subjects as that member, within the same roles.
It does that with the `mod_*` tools, which mirror the `atgrp mod`
commands (see [MCP](mcp.md)).

Details:

- `mod.read` is granted by `rpc:org.atgrp.mod.querySubjects`,
  `mod.resolve` by `rpc:org.atgrp.mod.updateSubject`, and `label` by
  `space:*?collection=com.atproto.label.record`, or any `space:*`
  permission covering that collection, which label in every space of
  the group's. `include:org.atgrp.authModerate` and
  `include:org.atgrp.authManageAdmin` carry `mod.read` and
  `mod.resolve`. They label only in the meta and members spaces, the
  ones their permissions name, so they don't carry `label`
  ([membership](membership.md#roles-and-what-they-grant)).
- No caller at all gets `401 AuthMissing`.

## Labels

A label is a moderation mark on a record or an account. Here it's a
`com.atproto.label.record` record that the group writes, the same way it
writes any record of its own. That means `com.atproto.space.createRecord`
and the rest, with a token for the group ([signing in](signing-in.md)).
It holds the subject's `uri` (an account's DID, or a record's URI),
optionally the `cid` of one version of a record, a value `val`, and
`cts`, when it was made.

A label has to be readable by the people who see its subject, and only
by them. So it goes where the subject lives. A record's label goes into
the space the record is in, in the group's own repo there. An account's
label goes into the group's members space. Anyone who may read the space
may read its labels.

A member also writes in their own public repo, outside the group's
spaces. An example is a post that an app such as a forum shows to the
group's public community. Such a record is labelled where its author's
labels go, in the members space, so a moderator can label one post
instead of the member who wrote it. The host refuses a label whose `uri`
isn't a DID or a record in the space it's written in. In the members
space, a record in the public repo of someone who holds a role in the
group is also allowed.

You take a label back by deleting it. The record is gone from the space,
but the queue keeps that it was written, when, and by whom.

One value has an effect here: `!takedown`, the spaces convention for a
space's authority taking content down. A record the group takes down,
with a `!takedown` label beside it, is no longer served from the host's
index of the space (`listSpaceRecords`), nor are the files it holds,
as files (`getSpaceBlob`) or as pictures (`/img/{did}/{cid}`). A label
naming a `cid` takes down only that version.
An account taken down has everything it wrote in that space taken out.
Since an account's labels go in the members space, that's every space of
the group. Only the group's own labels count, and one past its `exp`
counts no more. Deleting the label puts it all back. What's taken down
stays in its writer's repo. The host stops serving it to everyone
reading the space, moderators included, and shows it only in the queue,
to moderators reviewing it (see [reviewing what was taken
down](#reviewing-what-was-taken-down)). Every other value is for apps.
An app that reads a space reads its labels with it, and decides what to
show. The group says what its values mean in its labeler (see
[reports](#reports)).

Details:

- Writing or deleting a label in a space needs the roles' permissions
  to cover writing `com.atproto.label.record` there. The moderation
  sets cover the meta and members spaces.
  `space:*?collection=com.atproto.label.record`, or a permission naming
  every collection, covers every space. A permission the space's policy
  grants the role covers that space alone. So a moderator whose role
  holds only `include:org.atgrp.authModerate` labels accounts, and
  records in the meta and members spaces, but nothing in the group's
  other spaces. The token's scope must cover the write, as for any
  record.
- `getGroup` lists the spaces a member may label in as
  `viewer.labelSpaces`. The console offers labelling a subject only
  where its labels go in one of them.
- The record is checked against its lexicon. A value is 128 bytes at
  most, and `uri`, `val` and `cts` are required.
- The host doesn't check that the subject exists or that a CID matches.
- Every label written, and every one deleted, is also an event in the
  queue.

## The queue

A moderator needs to know what's waiting for a judgement and what was
already done about it. The queue answers that. It's the host's own, kept
in its database instead of published in the group's repo, and only
people with `mod.read` see it.

A subject is either an account or a record, sent and answered as one
`subject`: an account's DID, or a record's URI, with the `cid` of one
version beside it, if it names one. The record is one in a space,
`at:space/<group>/<type>/<skey>/<did>/<collection>/<rkey>`, or one in a
public repo, `at://<did>/<collection>/<rkey>`, and it's a
subject of its own, separate from its author's account. The queue keeps
a record as a whole, by its URI. The version a report or label names
stays on that event. A subject is either `open` or `resolved`, and has
the `tags` its moderators gave it.

Each subject in the queue comes with `actor`, the account it is or whose
record it is, by its DID and handle. So a moderator reads who it's about
instead of a DID. It also counts its reports as `reportCount`.

Its history is a list of events. Each has an `eventId`, its `subject`
and `createdAt`. It names who did it as `createdBy` and, by their DID
and handle, as `creator`. It gives the version of a record it's about
as `cid` when it names one, and one of six kinds as `event`:

- `reportEvent`: someone reported it, with a `reasonType`, an optional
  `reason` and the `rule` it cites, if any.
- `appealEvent`: its subject appealed, with an optional `note`.
- `statusEvent`: a moderator set its `status`, or added a `note`.
- `tagEvent`: a moderator gave it tags (`add`) or took them off
  (`remove`).
- `labelEvent`: a label was written, by the label record's `uri` and its
  `val`. With `deleted: true`, the label was deleted, which takes it
  back.
- `ejectEvent`: the member was removed from the group and their
  membership deleted. It says nothing more.

A subject enters the queue with its first report, label or ejection, and
starts out open.

`mod.querySubjects` lists the queue a page at a time. It lists every
status unless `status` names one, and with `tags`, only the subjects
that have any of them. `sort` orders it. That's `oldest` first by when
the subject entered the queue (the default), `newest` first, or
`updated`, the latest changed first. Every event counts as a change.
`mod.querySubjectHistory` answers a subject's events, newest first
unless `sort` says `oldest`. With a `cid` it answers only those about
that version. A subject nothing has happened to has no events.

`mod.updateSubject` changes a subject already in the queue. It sets its
`status` (`resolved`, or `open` to reopen it), adds a `note` of 2,000
characters at most, and gives it tags (`addTags`) or takes them off
(`removeTags`). Each is optional, and what's left out stays as it is.
A status or a note is a `statusEvent`, and tags are a `tagEvent`. It
answers the subject as the queue has it after the change.

A tag is lowercase letters and hyphens, 128 characters at most, and a
call adds or removes 25 at most. Tags mean what a group makes them
mean, and no one is told about them. The console uses one of its own.
Its Escalate button adds the tag `escalated`, and the queue's Escalated
tab lists the open subjects with that tag. Taking the tag off
de-escalates.

A resolved subject should come back when someone asks again. A label is
a moderator's own act, so it shouldn't undo a resolution. So only a
report or an appeal reopens a resolved subject. A label or an ejection
opens a subject nobody reported, and leaves a resolved one resolved.

Deleting a group deletes its queue.

Details:

- A subject that isn't a DID, a record in one of the group's spaces or
  a record in a public repo fails with `InvalidSubject`. Updating one
  that was never in the queue fails with `SubjectNotFound`.
- A tag that isn't lowercase letters and hyphens fails with
  `InvalidRequest`, and so does a `sort` the method doesn't know.
- A page is 50 events or subjects unless `limit` says otherwise, and 100
  at most. The `cursor` an answer gives asks for the next page. One the
  host never gave fails with `InvalidCursor`.

### Reviewing what was taken down

A moderator who judges a report, or an appeal of a takedown, has to see
what they're judging. A takedown hides it from everyone who reads the
space, moderators too, so the queue shows it instead. Reading the space
still leaves it out for everyone, so a takedown never leaks back into
an app that a moderator uses.

For a subject that's a record in one of the group's spaces,
`mod.querySubjectHistory` also answers the `record`, as the host's
index holds it, with `takenDown` saying whether the group took that
version down. `getModSubjectBlob` serves the files that record holds,
by their CID, read from whoever wrote it, as `getSpaceBlob` does. Both
need `mod.read`. The record and its files only go to a moderator whom
the space's policy lets read it, so the queue shows nobody anything
they couldn't read in the space if it were up.

The record is the version the index holds now, which may be newer than
the one a report names. An account, or a record in a public repo, has
no record here. Each record of an account taken down is a subject of
its own, shown the same way when it's reviewed.

## Reports

A report comes from someone who can see what they report. So a group
hears from the people its content reaches, and strangers can't report.
A report arrives at `org.atgrp.mod.submitReport`, by service auth or
the console's session. It names the `group`, the `subject` and its
`cid`, a `reasonType`, and optionally a `reason` and the `rule` it
cites. It needs no role. A reason is one of six tokens:
`org.atgrp.mod.defs#reasonSpam`, `#reasonViolation`,
`#reasonMisleading`, `#reasonNsfw`, `#reasonRude` and `#reasonOther`.

The subject is an account that's a member of the group, a record in one
of the group's spaces hosted here, or a record in the public repo of a
member. The reporter must be able to read the space the subject is in.
For a record that's its own space. For an account or a member's public
record it's the members space, where its labels go. The report goes into
the group's queue as a `reportEvent`, and the host answers with its
`eventId` and `createdAt`. Nobody is notified, so the report waits in
the queue for a moderator.

The group's labeler, in its members space, says what reports it takes.
Each of its lists, when present, limits them. `reasonTypes` limits the
reasons, `subjectTypes` the kinds of subject (`account`, `record`), and
`subjectCollections` the collections a reported record may be in. A new
group's labeler takes reports of accounts and records, for all six
reasons, of any collection.

Someone can file 10 reports in 5 minutes and 100 in a day, across every
group here ([limits](limits.md)). That keeps one person from flooding a
group's moderators. A refused report doesn't count.

Details:

- A group not hosted here fails with `GroupNotFound`. A subject fails
  with `InvalidSubject` if the reporter can't read it, if it isn't in
  the group, if it's a public record of someone who isn't a member, or
  if it's of a kind or collection the labeler doesn't take. A reason it
  doesn't take fails with `InvalidRequest`. A member's public record is
  a `record`, of its collection.
- A `rule` must be a record's URI in the group's members space, where
  its rules are.
- The host doesn't check that the reported record exists or that its CID
  matches.

## Appeals

Someone a group moderated should be able to answer it, and the
moderators should hear it where they hear everything else. So an appeal
goes into the queue too. It arrives at `org.atgrp.mod.submitAppeal`, by
service auth or the console's session, naming the `group`, the
`subject` and an optional `note`.

Only the subject may appeal. That's an account for itself, or a
record's author for the record (the repo its URI is in). The host
refuses anyone else's appeal with `InvalidSubject`. There has to be
something to appeal, too. That's a label the group wrote and hasn't
taken back, or an ejection. Otherwise the host answers `NotAppealable`.
An appeal goes into the queue as an `appealEvent`, reopens a resolved
subject, and the host answers with its `eventId` and `createdAt`.

Someone put out of the group holds no role and can't read the members
space, but what was done to them is still theirs to answer. So an
appeal needs no role and no read access, just the label or ejection.
The labeler doesn't limit appeals. An appeal counts against the same
limit as reports, since each asks the moderators to look.

`org.atgrp.mod.querySelfHistory` shows someone what the group did to
their account and their records, so they know what to appeal. It takes
the `group`, and optionally one `subject`, which must be theirs
(`InvalidSubject` otherwise). It answers events newest first unless
`sort` says `oldest`, a page at a time, like a subject's history. It
leaves out reports and tags, every note, and who acted, so it never
names a reporter or a moderator. It works for former members too.

## The group's own labels

A label anyone can put on anything says nothing to an app that doesn't
know what it means. So the group's labeler also lists the labels it
applies, in `policies.labelValues`. It defines its own in
`policies.labelValueDefinitions`, each the way atproto defines one. That
covers its `identifier`, its name and meaning in `locales`, and how apps
should show what wears it (`severity`, `blurs`, `defaultSetting`,
`adultOnly`). A new group applies `!hide` and `!takedown`, which atproto
defines, and none of its own.

`putLabelDefinition` defines one, or changes the one with that value,
and adds it to the values the labeler applies. A change replaces its
name and meaning in one language and keeps the others, so naming a
label in French leaves its English name. `deleteLabelDefinition`
takes both away, and labels already applied stay. Each needs
`group.configure`, and writes the labeler in the members space as the
group. A group's own label is lowercase letters and hyphens, the way
atproto has a labeler's own values, so atproto's are never redefined.
`getGroup` answers the definitions as `labels`, to anyone who may read
the members space.

Limits and the answers' fields are in [the API
reference](../reference/api.md).
