// What a role may do, as the host's methods ask (org.atgrp.internal.defs#action).
// The lexicon's types carry its known values only as a type, so the list is
// written here and held to that type both ways: a value it does not know, or
// one it knows and the list leaves out, fails svelte-check.

import type { Action } from '#lib/lexicons/org/atgrp/internal/defs.defs.ts';

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

const rpc = (method: string) => `rpc:org.atgrp.${method}?aud=*`;
const members = (write: string) =>
  `space:org.atgrp.members?skey=self&collection=org.atgrp.membership&action=${write}`;

/**
 * The scope that grants each action, as a role's record holds it among
 * its permissions: what the host reads it as.
 */
export const SCOPES: Record<(typeof KNOWN)[number], string> = {
  'mod.read': rpc('mod.querySubjects'),
  'mod.resolve': rpc('mod.updateSubject'),
  label: 'space:*?collection=com.atproto.label.record',
  invite:
    'space:org.atgrp.inbox?authority=*&skey=self&collection=org.atgrp.invite&action=read_self&action=create&action=update&action=delete',
  admit: members('create'),
  eject: members('delete'),
  'role.assign': members('update'),
  'space.create': 'space:*?manage=create',
  'space.configure': 'space:*?manage=update',
  'space.delete': 'space:*?manage=delete',
  'group.configure': 'include:org.atgrp.authManageAdmin'
};

const set = (name: string) => `include:org.atgrp.${name}`;
const MANAGES_MEMBERS = ['invite', 'admit', 'eject', 'role.assign'];
// Not `label`, labels in every space: the sets label only in the meta and members spaces.
const MODERATES = ['mod.read', 'mod.resolve', 'eject'];

/**
 * The group's permission sets a role may carry, smallest first: each its
 * scope, the actions the host reads it as granting, and the smaller sets
 * it holds all of. The whole group's is `group.configure`'s scope, so
 * that action is chosen here, not on its own.
 */
export const SETS: { key: SetKey; scope: string; actions: string[]; within: SetKey[] }[] = [
  { key: 'basic', scope: set('authManageBasic'), actions: [], within: [] },
  { key: 'members', scope: set('authManageMembers'), actions: MANAGES_MEMBERS, within: [] },
  { key: 'moderate', scope: set('authModerate'), actions: MODERATES, within: [] },
  {
    key: 'admin',
    scope: SCOPES['group.configure'],
    actions: [...new Set([...MANAGES_MEMBERS, ...MODERATES]), 'group.configure'],
    within: ['basic', 'members', 'moderate']
  }
];

export type SetKey = 'basic' | 'members' | 'moderate' | 'admin';

/** The actions a role's editor offers one by one: all but the one only a set gives. */
export const SINGLE = ACTIONS.filter((a) => a !== 'group.configure');
