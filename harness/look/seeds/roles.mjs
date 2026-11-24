// Lantern Hikers' roles, after `full`, in each way a role holds what it
// does: a greeter role with single actions only (invite and admit), the
// moderator role `full` makes (the moderation permission set, and labels
// in every space), and a steward role with the whole group's permission
// set alone, which dan holds: he runs the group, but may not give a role
// labels in every space, nor spaces to make, change or end.

export default async ({ opensocial, who }) => {
  const g = 'lanterns.groups.test';
  const members = (write) =>
    `space:fyi.opensocial.members?skey=self&collection=fyi.opensocial.membership&action=${write}`;
  const invite =
    'space:fyi.opensocial.invites?authority=*&skey=self&collection=fyi.opensocial.invite&action=read_self&action=create&action=update&action=delete';
  opensocial(
    'alice',
    'role',
    'set',
    g,
    'greeter',
    '--display-name',
    'Greeter',
    '--scopes',
    [invite, members('create')].join(','),
    '--assignable',
    'member'
  );
  opensocial(
    'alice',
    'role',
    'set',
    g,
    'steward',
    '--display-name',
    'Steward',
    '--scopes',
    'include:fyi.opensocial.authManageGroup',
    '--assignable',
    'member,greeter,moderator,steward'
  );
  opensocial('alice', 'member', 'assign', g, who.dan.handle, 'member,steward');
};
