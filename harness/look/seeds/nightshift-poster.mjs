// Night shift with carol in it as its poster: a role that may write every
// collection as the group, but not configure it, so she may make app
// passwords for it, and of its settings sees only those.

export default async ({ opensocial, ask, who }) => {
  const g = 'nightshift.groups.test';
  const role = 'poster';
  opensocial('bob', 'role', 'set', g, role, '--display-name', 'Poster');
  // Its admins may give it while it is new, and nobody holds or names it.
  opensocial('bob', 'role', 'grant', g, 'admin', '--assignable', role);
  opensocial('bob', 'role', 'grant', g, role, '--scopes', 'repo:*');
  await ask('carol', g);
  opensocial('bob', 'member', 'assign', g, who.carol.handle, `member,${role}`);
};
