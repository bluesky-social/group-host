/**
 * A name as a handle's first label: lowercase, its accented letters as the
 * letters, and nothing but letters, digits and the dashes between them, 63
 * at most. Made as it is typed, so the address beside the host's domain is
 * the one that will be asked for; a dash at the end, while more may follow,
 * is dropped only by `finished`.
 */
export function labelOf(text: string) {
  return text
    .toLowerCase()
    .normalize('NFKD')
    .replace(/\p{M}+/gu, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+/, '')
    .slice(0, 63);
}

/** A label as it is asked for: no dash left at its end. */
export const finished = (label: string) => labelOf(label).replace(/-+$/, '');
