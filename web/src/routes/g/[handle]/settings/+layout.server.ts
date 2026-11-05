import type { LayoutServerLoad } from './$types';
import { settings } from '#lib/server/settings.ts';

/**
 * A group's settings, each part a page of its own: its profile, rules and
 * roles, recovery keys and the apps acting as it, to its configurers; its
 * app passwords, to them and to whoever may make one.
 */
export const load: LayoutServerLoad = (event) => settings(event);
