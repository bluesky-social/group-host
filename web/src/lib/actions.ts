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
