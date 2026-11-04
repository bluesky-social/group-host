# Moderation

A group moderates what is in its spaces, and who is in it, itself: reports
come in, labels go out, and a queue sits between (`moderation.rs`).

## Labels

A label is a record the group writes, `fyi.opensocial.label`, naming its
subject, its value, and the rule it is under if one. It lives beside what
it labels: a label on a record goes in the space the record is in, so
whoever may read the record may read its labels, and nobody else; a label
on an account has no content to sit beside, and goes in the members
space. A group labels only what is in its own spaces: another group's
content is that group's to moderate.

`applyLabel` writes one, for a role holding `label`; the values that
remove content, `!hide` and `!takedown`, take `takedown`. `negateLabel`
takes one back with a second record, `neg: true`, rather than deleting
the first: that a label was applied is part of what happened.

## The queue

Each subject (a record, by its URI; an account, by its DID) is open,
escalated or resolved, with what happened to it in order: reports,
labels, negations, notes, and the moderators' decisions. The queue is the
host's to keep, not the group's to publish: its statuses are a group's
moderators' working state.

- `listSubjects` (for `mod.read`): the queue, of a status if asked, the
  latest changed first, a page at a time.
- `getSubjectHistory` (for `mod.read`): one subject, and what happened to
  it.
- `resolveSubject` (for `mod.resolve`): resolve it, escalate it, or note
  something on it, which changes nothing else.

A label opens a subject nobody reported, and leaves a resolved one
resolved: labelling is the moderators acting, not someone asking them to.

## Reports

`com.atproto.moderation.createReport` takes a report from anyone, member
or not, as their PDS sends it on: of a record in one of a group's spaces
here, into that group's queue. A report on a resolved subject opens it
again.

## From the prototype

- The host's own queue, for what its sites module shared with everyone
  (templates, their likes), is not here, with the sites module.
- A report is of a record; an account is labelled by the group, not
  reported to it, as the prototype had it.
