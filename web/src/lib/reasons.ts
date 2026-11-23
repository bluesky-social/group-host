// Why something was reported, in the words a moderator reads, rather than
// as a report names it (`com.atproto.moderation.defs#reasonSpam`).

import { m } from '#lib/paraglide/messages.js';

const NAMED: Record<string, () => string> = {
  reasonSpam: m.reasons_spam,
  reasonViolation: m.reasons_violation,
  reasonMisleading: m.reasons_misleading,
  reasonSexual: m.reasons_sexual,
  reasonRude: m.reasons_rude,
  reasonOther: m.reasons_other,
  reasonAppeal: m.reasons_appeal
};

/**
 * A reason type, named: one of atproto's own by its name here, any other
 * by its words split apart, as "Ban evasion" for `#reasonBanEvasion`.
 */
export function reason(type: string): string {
  const name = type.split('#').pop() ?? type;
  if (NAMED[name]) return NAMED[name]();
  const words = name.replace(/^reason/, '').replace(/([a-z])([A-Z])/g, '$1 $2');
  return words ? words[0].toUpperCase() + words.slice(1).toLowerCase() : type;
}
