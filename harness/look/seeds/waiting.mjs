// Riders waiting on people, and people on it: bob asking to join it with
// a note, and an invite to it waiting for carol.

export default async ({ opensocial, ask, who }) => {
  await ask('bob', 'riders.groups.test', 'I ride most weekends.')
  opensocial('alice', 'invite', 'send', 'riders.groups.test', who.carol.handle, '--message', 'Ride with us.')
}
