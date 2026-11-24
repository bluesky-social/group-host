// What a role may do, as the host's methods ask (fyi.opensocial.defs#action).
// The lexicon's types carry its known values only as a type, so the list is
// written here and held to that type both ways: a value it does not know, or
// one it knows and the list leaves out, fails svelte-check.

import type { Action } from '#lib/lexicons/fyi/opensocial/defs.defs.ts';

const KNOWN = [
  'mod.read',
  'mod.resolve',
  'label',
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
 * The scope that grants each action, as a role's record holds it among
 * its scopes: what the host reads it as.
 */
export const SCOPES: Record<(typeof KNOWN)[number], string> = {
  'mod.read': rpc('listModSubjects'),
  'mod.resolve': rpc('resolveModSubject'),
  label: 'space:*?collection=com.atproto.label.record',
  invite:
    'space:fyi.opensocial.invites?authority=*&skey=self&collection=fyi.opensocial.invite&action=read_self&action=create&action=update&action=delete',
  admit: members('create'),
  eject: members('delete'),
  'role.assign': members('update'),
  'space.create': 'space:*?manage=create',
  'space.configure': 'space:*?manage=update',
  'space.delete': 'space:*?manage=delete',
  'group.configure': 'include:fyi.opensocial.authManageGroup'
};

const set = (name: string) => `include:fyi.opensocial.${name}`;
const MODERATES = ['mod.read', 'mod.resolve', 'label', 'invite', 'admit', 'eject', 'role.assign'];

/**
 * The group's permission sets a role may carry, smallest first: each its
 * scope, the actions the host reads it as granting, and the smaller sets
 * it holds all of. The whole group's is `group.configure`'s scope, so
 * that action is chosen here, not on its own.
 */
export const SETS: { key: SetKey; scope: string; actions: string[]; within: SetKey[] }[] = [
  { key: 'profile', scope: set('authManageProfile'), actions: [], within: [] },
  { key: 'moderate', scope: set('authModerate'), actions: MODERATES, within: [] },
  {
    key: 'group',
    scope: SCOPES['group.configure'],
    actions: [...MODERATES, 'group.configure'],
    within: ['profile', 'moderate']
  }
];

export type SetKey = 'profile' | 'moderate' | 'group';

/** The actions a role's editor offers one by one: all but the one only a set gives. */
export const SINGLE = ACTIONS.filter((a) => a !== 'group.configure');
