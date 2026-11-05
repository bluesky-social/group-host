// What a role may do, as the host's methods ask (fyi.opensocial.defs#action).
// The lexicon's types carry its known values only as a type, so the list is
// written here and held to that type both ways: a value it does not know, or
// one it knows and the list leaves out, fails svelte-check.

import type { Action } from '#lib/lexicons/fyi/opensocial/defs.defs.ts';

const KNOWN = [
  'mod.read',
  'mod.resolve',
  'label',
  'takedown',
  'invite',
  'admit',
  'eject',
  'role.assign',
  'space.create',
  'space.configure',
  'space.delete',
  'group.configure'
] as const satisfies readonly Action[];

/** The actions the lexicon names, without the open string any other is. */
type Named<T> = T extends string ? (string extends T ? never : T) : never;
/** Each action the lexicon names and the list leaves out; none. */
type Missing = Exclude<Named<Action>, (typeof KNOWN)[number]>;
const whole: [Missing] extends [never] ? true : Missing = true;
void whole;

export const ACTIONS: string[] = [...KNOWN];

const rpc = (method: string) => `rpc:fyi.opensocial.${method}?aud=*`;
const members = (write: string) =>
  `space:fyi.opensocial.members?skey=self&collection=fyi.opensocial.membership&action=${write}`;

/**
 * The scope that grants each action, as a role's grant in the group's
 * permissions record holds it: what the host reads it as.
 */
export const SCOPES: Record<(typeof KNOWN)[number], string> = {
  'mod.read': rpc('listSubjects'),
  'mod.resolve': rpc('resolveSubject'),
  label: rpc('applyLabel'),
  takedown: 'space:*?collection=fyi.opensocial.label',
  invite: 'space:fyi.opensocial.invites?authority=*&collection=fyi.opensocial.invite',
  admit: members('create'),
  eject: members('delete'),
  'role.assign': members('update'),
  'space.create': 'space:*?manage=create',
  'space.configure': 'space:*?collection=fyi.opensocial.access',
  'space.delete': 'space:*?manage=delete',
  'group.configure': 'include:fyi.opensocial.configure'
};
