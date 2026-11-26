// Lantern Hikers' roles, after `full`, in each way a role holds what it
// does: a greeter role with single actions only (invite and admit), the
// moderator role `full` makes (the moderation permission set, and labels
// in every space), and a steward role with the whole group's permission
// set alone, which dan holds: he runs the group, but may not give a role
// labels in every space, nor spaces to make, change or end.

export default async ({ atgrp, who }) => {
  const g = 'lanterns.groups.test';
  const members = (write) =>
    `space:org.atgrp.members?skey=self&collection=org.atgrp.membership&action=${write}`;
  const invite =
    'space:org.atgrp.inbox?authority=*&skey=self&collection=org.atgrp.invite&action=read_self&action=create&action=update&action=delete';
  atgrp(
    'alice',
    'role',
    'set',
    g,
    'greeter',
    '--display-name',
    'Greeter',
    '--permissions',
    [invite, members('create')].join(','),
    '--can-assign',
    'member'
  );
  atgrp(
    'alice',
    'role',
    'set',
    g,
    'steward',
    '--display-name',
    'Steward',
    '--permissions',
    'include:org.atgrp.authManageAdmin',
    '--can-assign',
    'member,greeter,moderator,steward'
  );
  atgrp('alice', 'member', 'assign', g, who.dan.handle, 'member,steward');
};
