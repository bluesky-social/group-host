/**
 * Where the group host serves an image of `did`'s, by the CID a record
 * names it by, for whoever may see that record: none for no image.
 */
export const image = (did: string, cid?: string | null) => (cid ? `/img/${did}/${cid}` : null);

/**
 * A group's links as a page draws them: each with where its site's icon
 * is served, by the CID its blob reference names (`ref.$link`, as JSON
 * carries it), or none.
 */
export const linksOf = (
  did: string,
  links: { url: string; title?: string; icon?: unknown }[] = []
) =>
  links.map(({ url, title, icon }) => ({
    url,
    title,
    icon: image(did, (icon as { ref?: { $link?: string } } | undefined)?.ref?.$link)
  }));
