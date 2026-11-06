# Moderating

This guide shows you how to handle what members report in your group.
You read the reports, label or take down an account, remove someone from
the group, and close the report.

## Before you begin

You need a role in the group that lets you see the moderation queue. If
you started the group, your admin role already does everything here.

## Who can moderate, and where?

Moderation happens on the group's Moderation page. A member sees the way
there only when one of their roles lets them read the moderation queue.

1. Open your group in the console. If you may moderate, its card has
   **Moderation** at the top right, beside **Settings** if you are an
   admin.

   <!-- look /g/gardens.groups.test --as dan --seed moderating --element .group-card -->
   ![Garden share's card, with Moderation and Settings at its top right](shots/moderating-group.png)

2. Press **Moderation**. The page opens on its **Open** tab.

A member without the role does not see the button, and if they open the
page anyway it tells them there is nothing there for them. Each part of
the page needs its own permission in a role. It takes `mod.read` to see
the queue, `mod.resolve` to resolve or escalate, `label` to label
someone, and `takedown` to take an account down. You give these to a role as in
[Rules and roles](rules-and-roles.md).

## How does something come to be reported?

A report is a member flagging a post or an account to the group's
moderators. Members report from the apps they use in the group, not from
the console. Each report lands on this page, with the reason the member
chose and what they wrote. A member can report an account only while it
is in the group, and a post only in a part of the group they can read.

## What is on the Moderation page?

1. Look at the **Open** tab. Each row is one subject: an account, or a
   post that was reported, under the handle of the account that posted
   it. Under the handle, whether it is an account or a post and when it
   was reported, then the reasons members chose, then what the latest
   report said.

   <!-- look /g/gardens.groups.test/moderation --as dan --element main -->
   ![The Moderation page on its Open tab, Subjects 2: maximilianaquinces.test's account reported for Spam and frank.test's post reported as Rude, each with Review](shots/moderating-queue.png)

2. Press **History** under a row to see everything that happened to it,
   oldest first: each report, label, escalation or resolution, who did
   it, when, and what they wrote.

The tabs sort subjects by where they are:

- **Open**: reported and not yet dealt with.
- **Escalated**: passed on, for someone else in the group to decide.
- **Resolved**: dealt with. A new report on a resolved subject puts it
  back in **Open**.

Nothing tells anyone when a subject moves. Whoever moderates looks at
the tabs.

## How do I pass a report on?

When a report is not yours to decide, escalate it.

1. Press **Review** on the post's row. The dialog **Resolve or escalate**
   opens.
2. Under **A note, if you like**, type why, such as "Third time this
   month; over to the admins."
3. Press **Escalate**. The dialog closes, and the subject leaves the
   **Open** tab.

   <!-- look /g/gardens.groups.test/moderation --as frank --click 'button[popovertarget=resolve-1]' --type '#resolve-1 [name=note]=Third time this month; over to the admins.' --click '#resolve-1 button[value=escalated]' --element main -->
   ![The Open tab after escalating, Subjects 1: only the account reported for spam is left](shots/moderating-escalate-sent.png)

The post now waits on the **Escalated** tab, your note in its history.

## How do I label an account?

A label is a tag the group puts on an account, such as "spam". Apps that
follow the group's labels decide what to do with it. Two labels mean
more: `!takedown` and `!hide`, which the next part shows. Any other
word is yours to choose.

The console cannot take a label back. Only a terminal can, so check the
label before you send it.

The dialog asks for the account's handle, as the queue shows it.

1. Press **Label someone**.
2. Under **Their handle**, type the account's handle, such as
   "maximilianaquinces.test".
3. Under **Label**, type the label, such as "spam".

   <!-- look /g/gardens.groups.test/moderation --as dan --click 'button[popovertarget=label]' --type '#label-handle=maximilianaquinces.test' --type '[name=val]=spam' --element ':popover-open' -->
   ![The Label someone dialog, maximilianaquinces.test under Their handle and spam under Label, with Cancel and Label them](shots/moderating-label-dialog.png)

4. Press **Label them**. The dialog closes, and the account's history
   gains a line saying it was labelled.

   <!-- look /g/gardens.groups.test/moderation --as dan --click 'button[popovertarget=label]' --type '#label-handle=maximilianaquinces.test' --type '[name=val]=spam' --click ':popover-open button[type=submit]' --click 'li.subject:has-text("maximilianaquinces.test") summary' --element 'li.subject:has-text("maximilianaquinces.test")' -->
   ![The account's row, its history open and ending in Labelled by dan.test, spam](shots/moderating-labelled.png)

If the handle is not found, the dialog stays open and says so. The
dialog labels accounts only.

## How do I take an account down?

A takedown asks apps to hide what an account posted. The host writes
the label but hides nothing itself. Apps that follow the group's labels
do the hiding. The account stays in the group until you remove it, as
the next part shows.

There is no takedown button. You type the label `!takedown` in the same
dialog, and it needs a role with `takedown`.

1. Press **Label someone**.
2. Under **Their handle**, type "maximilianaquinces.test".
3. Under **Label**, type "!takedown".
4. Press **Label them**. The account's history gains a line for the
   takedown.

   <!-- look /g/gardens.groups.test/moderation --as dan --click 'button[popovertarget=label]' --type '#label-handle=maximilianaquinces.test' --type '[name=val]=!takedown' --click ':popover-open button[type=submit]' --click 'li.subject:has-text("maximilianaquinces.test") summary' --element 'li.subject:has-text("maximilianaquinces.test")' -->
   ![The account's row, its history ending in labels spam and !takedown, both by dan.test](shots/moderating-takedown.png)

The label `!hide` works the same way and needs the same permission. A
moderator without it is refused, and the dialog stays open with the
words "missing takedown".

## How do I remove someone from the group?

Removing takes away their membership. It does not hide what they posted;
a takedown does that.

1. Press **← Garden share** at the top of the page to go back to the
   group's page. In your group, the link shows your group's name.
2. In the members list, press the ⋯ at the end of the member's row, then
   **Remove from the group**.

   <!-- look /g/gardens.groups.test --as dan --click 'li.member:has-text("maximilianaquinces.test") .action-menu summary' --click 'li.member:has-text("maximilianaquinces.test") .menu button:has-text("Remove from the group")' --element ':popover-open' -->
   ![The Remove this member? dialog for maximilianaquinces.test, with Cancel and Remove](shots/moderating-remove-dialog.png)

3. Press **Remove** in the dialog **Remove this member?**. The member
   leaves the list, and its count goes down by one.

   <!-- look /g/gardens.groups.test --as dan --click 'li.member:has-text("maximilianaquinces.test") .action-menu summary' --click 'li.member:has-text("maximilianaquinces.test") .menu button:has-text("Remove from the group")' --click ':popover-open button[type=submit]' --element 'section:has(.member) .section-head' -->
   ![The Members card's heading, Members 2, with Invite someone](shots/moderating-removed.png)

They lose access to what only members see, and their history on the
Moderation page notes that they were removed. Removing does not block them:
they can ask to join again.

## How do I resolve a report?

1. On the Moderation page, press **Review** on the account's row.
2. Under **A note, if you like**, type what was done, such as "Taken down
   and removed from the group."

   <!-- look /g/gardens.groups.test/moderation --as dan --click 'button[popovertarget=resolve-0]' --type '#resolve-0 [name=note]=Taken down and removed from the group.' --element ':popover-open' -->
   ![The Resolve or escalate dialog for maximilianaquinces.test's account, the note Taken down and removed from the group. typed, with Cancel, Escalate and Resolve](shots/moderating-resolve-dialog.png)

3. Press **Resolve**. The subject moves to the **Resolved** tab, with
   all that was done and your note in its history.

From **Resolved**, the row's **Review** can still escalate a subject.

## From a terminal

Each act here has a command in [the `opensocial` command](../reference/cli.md):

- Read the queue: `opensocial mod list gardens.groups.test --status open`,
  or `escalated` or `resolved`.
- Read one subject's history, with who did each thing:
  `opensocial mod history gardens.groups.test <did or at:// uri>`.
- Escalate or resolve: `opensocial mod resolve gardens.groups.test <subject> --status escalated --note "…"`,
  or `--status resolved`.
- Label an account: `opensocial mod label gardens.groups.test <did> spam`.
  A takedown is the same with `!takedown`.
- Label a post: `opensocial mod label gardens.groups.test <at:// uri> <label> --cid <cid>`.
  Only the terminal labels posts.
- Take a label back: the same command with `--undo`. Only the terminal
  does this.
- Remove a member: `opensocial member eject gardens.groups.test maximilianaquinces.test`.

No command makes a report. An AI app connected to the group has the same
acts as tools, listed in [the MCP reference](../reference/mcp.md).

Next, give moderators a role of their own in
[Rules and roles](rules-and-roles.md), or read
[how moderation works](../design/moderation.md).
