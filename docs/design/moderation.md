# Moderation

A group's members write in their own repos, on their own PDSes. The
group cannot edit or delete what they wrote. So a group moderates by
writing its judgements beside the content, as records of its own, and by
keeping a queue of what still needs a judgement. Reports come in, labels
go out, and the queue sits between them.

Every moderation method names a group hosted here, a group account with
a members [space](spaces.md). A DID that is not one is refused with
`GroupNotFound`.

## Who moderates

Moderating is a matter of roles, as everything a member does in a group
is (see [membership](membership.md)). Four of the host's actions are
about moderation:

- `mod.read`: see the queue and a subject's history.
- `mod.resolve`: resolve, escalate or add a note to a subject.
- `label`: write a label.
- `takedown`: write the labels that ask for content to be hidden or
  taken down.

A role holds an action through an OAuth scope in the group's permissions
record, and a member holds the union of their roles' actions. There is
no deny and no hierarchy. A new group's `admin` role holds every action,
moderation included; its `member` role holds none. To make moderators, a
group gives a role these actions and gives that role to people.

Each method checks the caller's roles as they are at the moment of the
call. The caller is whoever signed in, either through service auth from
their PDS or through the console's session (see [signing
in](signing-in.md)). A caller without the action is refused `403
Forbidden`. An assistant a member connects can read the queue, label and
resolve as that member, within the same roles (see
[assistants](assistants.md)).

Details:

- The scopes behind each action are in [the API
  reference](../reference/api.md).
- A `space:*` scope covering the `fyi.opensocial.label` collection
  grants both `label` and `takedown`; the `applyLabel` method's scope
  grants `label` only.
- No caller at all is `401 AuthMissing`.

## Labels

A label is a moderation tag on a record or an account. Here it is an
`fyi.opensocial.label` record that the group writes with `applyLabel`.
It holds the `subject`, a value `val`, optionally the
`fyi.opensocial.rule` it cites as `rule`, and `createdAt`.

A label has to be readable by the people who see its subject, and only
by them. So the host writes it where the subject lives. A record's label
goes into the space the record is in, in the group's own repo there; an
account's label goes into the group's members space. Whoever may read
the space may read its labels. A group labels only what is in its own
spaces; a record in a space with another authority is refused.

A judgement can be withdrawn, but the history of it should stay. So
`negateLabel` does not delete the label. It writes a second label record
with the same subject and value and `neg: true`. A reader takes a label
as standing until a negation follows it.

The host writes labels and gives no value an effect of its own: it hides
and removes nothing. An app that reads a space reads its labels with it,
and decides what to show. The values `!hide` and `!takedown` ask for
content to be hidden or taken down, so writing or negating them needs
`takedown`; every other value needs `label`.

Details:

- A subject is an account (`com.atproto.admin.defs#repoRef`, by DID) or
  a record (`com.atproto.repo.strongRef`, by URI and CID).
- A record's URI must be a record in a space:
  `at://<authority>/space/<type>/<skey>/<repo>/<collection>/<rkey>`.
- The record key is a TID from the host's clock. `applyLabel` answers
  the label's `uri` and `cid`, and `negateLabel` the negation's.
- A label's value is 128 bytes at most. A subject's CID must parse as
  one, and a `rule` cited must be a record's URI in a space, as a rule's
  is.
- The host does not check that the subject exists, that a CID matches,
  or that a negation follows a label.
- Every label and negation is also an event in the queue.

## The queue

A moderator needs to know what is waiting for a judgement and what was
already done about it. The queue answers that. It is the host's own,
kept in its database rather than published in the group's repo, and only
those with `mod.read` see it.

A subject in the queue is a record, by its URI, or an account, by its
DID. It has a status: `open`, `escalated` or `resolved`. Its events are
kept in order, each with a kind (`report`, `label`, `negate`, `resolve`,
`escalate`, `note`, `eject`), the actor, and what applies of a reason,
value and note. A subject enters the queue with its first report, label
or ejection, opened. An ejection is kept on the member's DID, with who
put them out and the reason `ejectMember` was given.

`listSubjects` lists the queue, latest changed first, filtered by status
if asked. Every event moves a subject to the top. `getSubjectHistory`
shows one subject with its events. `resolveSubject` acts on a subject
already in the queue: `resolve` sets it `resolved`, `escalate` sets it
`escalated`, and `note` keeps a note without changing the status. A
note is 300 characters at most.
Escalating is only a status, for a group to give its own meaning; no one
is told.

A resolved subject should come back when someone reports it again. A
label is a moderator's own act, so it should not undo a resolution. So
only a report reopens a resolved subject; a label or an ejection opens a
subject nobody reported, and leaves a resolved one resolved. An escalated subject stays
escalated through new reports.

Deleting a group deletes its queue.

## Reports

Anyone signed in may report, member or not. A reporter needs no role and
need not be able to read the space. A report arrives at
`com.atproto.moderation.createReport`, by service auth or the console's
session.

A report here is of a record, by its URI. The record must be in a space
hosted here. The report goes into the queue of the group that is that
space's authority, as a `report` event with the reporter, a `reasonType`
and an optional `reason`. Nobody is notified: the report waits in the
queue for a moderator.

Details:

- A report of an account is refused; a URI outside a space hosted here
  is `NotHosted`.
- The host does not check that the reported record exists or that its
  CID matches.
- Limits and the answer's fields are in [the API
  reference](../reference/api.md).
