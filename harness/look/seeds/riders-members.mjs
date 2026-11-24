// Riders with people in it to give roles to: bob and carol, members.

export default async ({ atgrp, ask, who }) => {
  const g = 'riders.groups.test';
  for (const name of ['bob', 'carol']) {
    await ask(name, g);
    atgrp('alice', 'member', 'assign', g, who[name].handle, 'member');
  }
};
