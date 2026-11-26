# Rules and roles

This guide shows you how to write your group's rules and give members
roles. Rules tell everyone how the group works. Roles let you share the
running of it, such as letting someone approve new members.

## Before you begin

You need:

- a group where you have the admin role
- at least one other member to give a role to

The examples use Riders, a group with alice, bob and carol in it.

## Where are the group's rules, and who sees them?

1. Open your group's page and press **Settings** at the top right of its
   card.
2. Press the **Rules** tab. A new group has no rules yet, and the card
   says so.

The rules show on the group's page, under its name, in the order on this
tab. Everyone sees them there, members and people who aren't in the
group yet. Only those whose roles let them change the group's settings
see **Settings**. If anyone else opens the settings, the page tells them
"Nothing here is yours to change".

## How do I add a rule?

1. Press **Add a rule**.
2. In **Rule**, type the rule in a few words, such as "Ride with lights
   after dark". It can be 64 characters long.
3. In **What it means**, say more, such as "Front and back
   lights on from sunset, every ride." You can leave it empty.

   <!-- look /g/riders.groups.test/settings/rules --as alice --click 'button[popovertarget=add-rule]' --type '[name=title]=Ride with lights after dark' --type '[name=text]=Front and back lights on from sunset, every ride.' --element ':popover-open' -->
   ![The Add a rule dialog, with Ride with lights after dark in Rule, a sentence in What it means, and Cancel and Add](shots/rules-add.png)

4. Press **Add**. The dialog closes and the rule shows last in the list,
   with its number.

Each new rule goes at the end. The console can't edit a rule or move
one. To change a rule or its place, delete it and add it again.

## How do I delete a rule?

Say you added "Wait at the top" and "No headphones" as well, and now
you don't want the last one.

1. Press **Delete** beside the rule. The console asks **Delete this
   rule?** and says the rule will no longer be one of the group's rules,
   here or in other apps.

   <!-- look /g/riders.groups.test/settings/rules --as alice --click 'button[popovertarget=add-rule]' --type '[name=title]=Ride with lights after dark' --type '[name=text]=Front and back lights on from sunset, every ride.' --click ':popover-open button[type=submit]' --click 'button[popovertarget=add-rule]' --type '[name=title]=Wait at the top' --type '[name=text]=Nobody rides on until the last rider is up.' --click ':popover-open button[type=submit]' --click 'button[popovertarget=add-rule]' --type '[name=title]=No headphones' --click ':popover-open button[type=submit]' --click "button[aria-label='Delete “No headphones”']" --element ':popover-open' -->
   ![The Delete this rule? dialog for No headphones, with Cancel and Delete](shots/rules-delete.png)

2. Press **Delete**. The rule is gone, and the rules after it move up a
   number.

   <!-- look /g/riders.groups.test/settings/rules --as alice --click "button[aria-label='Delete “No headphones”']" --click ':popover-open button[type=submit]' --element section -->
   ![Rules 2: Ride with lights after dark and Wait at the top, numbered 1 and 2, each with Delete](shots/rules-list.png)

On the group's page, the rules are folded under **Rules** and its count.
A member presses it to read them.

<!-- look /g/riders.groups.test --as carol --seed riders-members --click '.rules summary' --element '.group-card' -->
![Riders' card as carol sees it, Rules 2 unfolded to the two numbered rules, and You're a member.](shots/rules-member.png)

## What is a role, and what can it let a member do?

A role is a name you give to members, such as "Ride leader", and what it
lets them do. Each thing it lets them do is a permission. Every group
has two roles to start. Admin has every permission. Member is the role
everyone in the group holds, and it adds no permissions.

The **Roles** tab in **Settings** lists the roles and the permissions of
each. They show by their short names:

| Permission | What it lets a member do |
| --- | --- |
| `mod.read` | See what's waiting for moderation. They get **Moderation** on the group's page. |
| `mod.resolve` | Settle what's waiting for moderation. |
| `label` | Put a label on a post or an account, a takedown too, anywhere in the group. |
| `invite` | Invite people. They get **Invite someone**. |
| `admit` | Let in people who ask to join. They get **Asking to join**. |
| `eject` | Remove members, with **Remove from the group** in the ⋯ menu on a member's row. |
| `role.assign` | Change members' roles, with **Change roles** in the ⋯ menu on a member's row. |
| `space.create`, `space.configure`, `space.delete` | Make a space (a part of the group with its own say over who reads and writes in it), change who can read and write one, and delete one. |
| `group.configure` | Change the group's settings: its profile, rules, roles, apps and recovery keys. |

Four parts in running the group bring several permissions at once,
under **Running the group** when you create or change a role:

- **Profile and rules** lets them change the group's profile and rules.
  It adds none of the permissions above.
- **Members** brings `invite`, `admit`, `eject` and `role.assign`.
- **Moderation** brings `mod.read`, `mod.resolve` and `eject`, and
  labels on the group's profile and members. To label posts everywhere
  in the group too, tick `label` as well.
- **The whole group** brings all of that, and `group.configure`.

A role also says which roles its members can give. That list limits
three things. They can change only the roles on it. They can remove only
members whose roles are all on it. Everyone they let in gets Member, so
a role that admits people needs Member on its list. Admin's list is
every role, even ones made later, so admins can give any role.

## How do I create a role?

1. In **Settings**, press the **Roles** tab, then **Create a role**.
2. In **Its ID**, type a short name for the role that never changes,
   such as "ride-leader". Use at least three lowercase letters and
   hyphens, and nothing else.
3. In **Its name**, which you may leave empty, type the name people
   see, such as "Ride leader".
4. Under **Running the group**, tick a part if the role should bring
   all it gives. A ride leader needs none. Under **It may**, tick what
   else the role lets its members do. A ride leader here gets
   `mod.read`, `invite` and `admit`.
5. Under **People with it can give**, tick the roles its members can
   give. Here, **Member**. If you can give every role yourself, **Any
   role, even ones made later** lets its members give every role too.
6. Tick **Every new member gets it** if everyone let in should have this
   role too.

   <!-- look /g/riders.groups.test/settings/roles --as alice --click 'button[popovertarget=define-role]' --type '#define-role [name=role]=ride-leader' --type '#define-role [name=name]=Ride leader' --click '#define-role input[value=invite]' --click '#define-role input[value=admit]' --click '#define-role input[value="mod.read"]' --click '#define-role input[name=canAssign][value=member]' --element ':popover-open' -->
   ![The Create a role dialog: ride-leader in Its ID, Ride leader in Its name, mod.read, invite and admit ticked under It may, Member ticked under People with it can give](shots/rules-create-role.png)

7. Press **Create**. The role shows last in the list, with its
   permissions.

   <!-- look /g/riders.groups.test/settings/roles --as alice --click 'button[popovertarget=define-role]' --type '#define-role [name=role]=ride-leader' --type '#define-role [name=name]=Ride leader' --click '#define-role input[value=invite]' --click '#define-role input[value=admit]' --click '#define-role input[value="mod.read"]' --click '#define-role input[name=canAssign][value=member]' --click '#define-role button[type=submit]' --element section -->
   ![Roles 3: Admin, Member, and Ride leader ride-leader with admit, invite and mod.read, each with a ⋯ menu](shots/rules-role-made.png)

You can give a role only permissions your own roles hold, and the
dialog offers only those.

## How do I change a role?

1. On the **Roles** tab, press the ⋯ at the end of the role's row. A
   menu opens.

   <!-- look /g/riders.groups.test/settings/roles --as alice --click '.roles > li:has(strong:text-is("Ride leader")) .action-menu summary' --element '.action-menu[open] .menu' -->
   ![The menu of Ride leader's row, open: Change and Delete](shots/rules-role-menu.png)

2. Press **Change**. The **Change this role** dialog opens, with what the
   role has ticked.
3. Tick or untick what it lets its members do. A permission that comes
   with a part in running the group shows ticked and greyed, saying
   which part it comes with, as when **Moderation** is ticked here. To
   remove one of those, untick the part, then tick the permissions the
   role keeps one by one.

   <!-- look /g/riders.groups.test/settings/roles --as alice --click '.roles > li:has(strong:text-is("Ride leader")) .action-menu summary' --click '.action-menu[open] .menu button:has-text("Change")' --click ':popover-open input[name=set][value=moderate]' --element ':popover-open' -->
   ![The Change this role dialog for Ride leader: Moderation ticked under Running the group, and under It may, mod.read, mod.resolve and eject ticked and greyed, each saying Comes with moderation, invite and admit ticked, and label not ticked](shots/rules-change-role.png)

4. Press **Save**. The dialog closes, and the role's row shows what it
   lets its members do now.

A permission a role has that the dialog doesn't show, such as Admin's
making and deleting spaces, says "Comes with its other permissions".
The dialog keeps those as they are. Change them from a terminal.

## How do I give a role to a member?

You give roles on the group's page, a member at a time.

1. Open the group's page. In **Members**, press the ⋯ at the end of the
   member's row. A menu opens.

   <!-- look /g/riders.groups.test --as alice --seed riders-members --click 'li.member:has-text("bob.test") .action-menu summary' --element '.action-menu[open] .menu' -->
   ![The menu of bob.test's row, open: Change roles and Remove from the group](shots/rules-menu.png)

   Press **Change roles**. A dialog opens, with the roles you can give
   under **Roles you can give**. The dialog lists roles by their keys,
   such as ride-leader. It doesn't show their names.
2. Tick the role, here "ride-leader". Leave member ticked. Everyone
   keeps it, and the host turns down a change that leaves it off.

   <!-- look /g/riders.groups.test --as alice --click 'li.member:has-text("bob.test") .action-menu summary' --click 'li.member:has-text("bob.test") .menu button:has-text("Change roles")' --click ':popover-open input[value=ride-leader]' --element ':popover-open' -->
   ![The Change roles dialog for bob.test, with member and ride-leader ticked and admin not, and Cancel and Save](shots/rules-give.png)

3. Press **Save**. The dialog closes, and the role shows as a tag under
   the member's handle.

   <!-- look /g/riders.groups.test --as alice --click 'li.member:has-text("bob.test") .action-menu summary' --click 'li.member:has-text("bob.test") .menu button:has-text("Change roles")' --click ':popover-open input[value=ride-leader]' --click ':popover-open button[type=submit]' --element '.members li:has-text("bob.test")' -->
   ![bob.test's row in the members list, tagged ride-leader, with the ⋯ menu at its end](shots/rules-given.png)

Now bob sees **Moderation**, **Asking to join** and **Invite someone**
on the group's page, and the group's card says they're a member with
the ride-leader role. They get no ⋯ menu on anyone's row, since the role
can't change roles or remove members.

## How do I take a role off, or delete it?

1. To take a role off, press the ⋯ on the member's row, then **Change
   roles**, untick the role, and press **Save**. The tag goes from their row.
2. To delete a role, press the ⋯ at the end of its row on the **Roles**
   tab, then **Delete**. The console asks you to confirm.
3. Press **Delete**. The role goes from the list.

People with a role lose what it lets them do when it goes. You can only
delete a role nobody holds, so take it off everyone first. The host
turns down a delete while anyone holds the role.

## What can't be changed?

Everyone in the group keeps the Member role, and its menu has no
**Delete**. Admin's menu has one, but you hold Admin, so the host turns
it down.
Leave Admin as it is.
You can give and take only the roles your own roles can give, and a role
you make can have only permissions you hold. Only members whose roles
hold `group.configure` can change the rules and roles at all.

## From a terminal

The [`atgrp` command](../reference/cli.md) does each of these:

- Add a rule: `atgrp rule set <group> --title <title> --text
  <words>`. With `--rule <key>` it replaces that rule, and `--order <n>`
  sets its place.
- Delete a rule: `atgrp rule delete <group> <rule>`.
- Create a role: `atgrp role set <group> <role> --display-name
  <name> --permissions <permission,…> --can-assign <role,…>`, with
  `--default` if every new member gets it. `atgrp role grant
  <group> <role> --permissions <permission,…>` adds to what a role has.
- Give or take roles: `atgrp member assign <group> <member>
  <role,role…>`, naming all the roles they keep, `member` among them.
- Delete a role: `atgrp role delete <group> <role>`.

An AI app connected to the group has the same acts as
[tools](../reference/mcp.md): `rule_set`, `rule_delete`, `role_set`,
`role_grant`, `member_assign` and `role_delete`.

Next, [moderate the group](moderating.md) with the roles you made. For
why roles and rules work as they do, see
[membership](../design/membership.md) and [groups](../design/groups.md).
