// Night shift with carol in it as its poster: a role that may write every
// collection as the group, but not configure it, so she may make app
// passwords for it, and of its settings sees only those. Its admins may
// give it, as they may give every role.

export default async ({ opensocial, ask, who }) => {
  const g = 'nightshift.groups.test';
  const role = 'poster';
  opensocial('bob', 'role', 'set', g, role, '--display-name', 'Poster', '--scopes', 'repo:*');
  await ask('carol', g);
  opensocial('bob', 'member', 'assign', g, who.carol.handle, `member,${role}`);
};
