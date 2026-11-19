// What the place's spaces hold, as last read, kept while the visitor moves
// between rooms, so a room opens with what it showed before and is drawn
// again as its records come in, never blank in between. Each room works out
// what it shows from these (views.js). Forgotten as who is looking changes.

import { held, onHeld } from './place.js';

/** The records of each of the place's spaces, by its kind, or none yet. */
export const kept = $state({ room: null, calendar: null });

/** Forgets what every space held. */
export function forget() {
  kept.room = kept.calendar = null;
}

/** Reads the space of `kind` again, keeping what it holds. */
export async function keep(ctx, kind) {
  kept[kind] = await held(ctx, kind);
}

/** Keeps what the space of `kind` holds after each later sync that changes it; returns what stops it. */
export const follow = (ctx, kind) =>
  onHeld(ctx, kind, (records) => {
    kept[kind] = records;
  });
