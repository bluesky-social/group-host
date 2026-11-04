/**
 * Where the group host serves an image of `did`'s, by the CID a record
 * names it by, for whoever may see that record: none for no image.
 */
export const image = (did: string, cid?: string | null) => (cid ? `/img/${did}/${cid}` : null);
