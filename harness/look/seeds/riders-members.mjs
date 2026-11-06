// Riders with people in it to give roles to: bob and carol, members.

export default async ({ opensocial, ask, who }) => {
  const g = 'riders.groups.test';
  for (const name of ['bob', 'carol']) {
    await ask(name, g);
    opensocial('alice', 'member', 'assign', g, who[name].handle, 'member');
  }
};
