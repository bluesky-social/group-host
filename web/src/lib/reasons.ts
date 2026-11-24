// Why something was reported, in the words a moderator reads, rather than
// as a report names it (`spam`, `nsfw`).

import { m } from '#lib/paraglide/messages.js';

const NAMED: Record<string, () => string> = {
  spam: m.reasons_spam,
  violation: m.reasons_violation,
  misleading: m.reasons_misleading,
  nsfw: m.reasons_sexual,
  rude: m.reasons_rude,
  other: m.reasons_other,
  appeal: m.reasons_appeal
};

/**
 * A reason type, named: one of the known ones by its name here, any other
 * by its last word, split apart, as "Ban evasion" for `banEvasion` or
 * `#reasonBanEvasion`.
 */
export function reason(type: string): string {
  if (NAMED[type]) return NAMED[type]();
  const name = type.split('#').pop() ?? type;
  const words = name.replace(/^reason/, '').replace(/([a-z])([A-Z])/g, '$1 $2');
  return words ? words[0].toUpperCase() + words.slice(1).toLowerCase() : type;
}
