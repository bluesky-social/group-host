// Night shift with carol in it as its poster: a role that may write every
// collection as the group, but not configure it, so she may make app
// passwords for it, and of its settings sees only those. Its admins may
// give it, as they may give every role.

export default async ({ atgrp, ask, who }) => {
  const g = 'nightshift.groups.test';
  const role = 'poster';
  atgrp('bob', 'role', 'set', g, role, '--display-name', 'Poster', '--permissions', 'repo:*');
  await ask('carol', g);
  atgrp('bob', 'member', 'assign', g, who.carol.handle, `member,${role}`);
};
