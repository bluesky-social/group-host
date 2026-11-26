// Why something was reported, in the words a moderator reads, rather than
// as a report names it (`org.atgrp.mod.defs#reasonSpam`).

import { m } from '#lib/paraglide/messages.js';

const DEFS = 'org.atgrp.mod.defs';

/** The reason types the spec names, each by its token, in the order a report offers them. */
export const REASONS = [
  `${DEFS}#reasonSpam`,
  `${DEFS}#reasonMisleading`,
  `${DEFS}#reasonRude`,
  `${DEFS}#reasonNsfw`,
  `${DEFS}#reasonViolation`,
  `${DEFS}#reasonOther`
];

const NAMED: Record<string, () => string> = {
  [`${DEFS}#reasonSpam`]: m.reasons_spam,
  [`${DEFS}#reasonViolation`]: m.reasons_violation,
  [`${DEFS}#reasonMisleading`]: m.reasons_misleading,
  [`${DEFS}#reasonNsfw`]: m.reasons_sexual,
  [`${DEFS}#reasonRude`]: m.reasons_rude,
  [`${DEFS}#reasonOther`]: m.reasons_other
};

/**
 * A reason type, named: one of the known ones by its name here, any other
 * by its last word, split apart, as "Ban evasion" for
 * `com.example.defs#reasonBanEvasion`.
 */
export function reason(type: string): string {
  if (NAMED[type]) return NAMED[type]();
  const name = type.split('#').pop() ?? type;
  const words = name.replace(/^reason/, '').replace(/([a-z])([A-Z])/g, '$1 $2');
  return words ? words[0].toUpperCase() + words.slice(1).toLowerCase() : type;
}
